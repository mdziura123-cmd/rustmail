//! Lokalna baza SQLite (tryb offline). Włączana wyłącznie na życzenie użytkownika.
//! UWAGA: baza NIE jest szyfrowana. Chroni ją tylko uprawnienia plików (katalog 0700, plik 0600)
//! i ewentualne szyfrowanie dysku. Tokeny i hasła nigdy do niej nie trafiają (są w pęku kluczy).

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

use crate::gmail::{ThreadDetail, ThreadSummary};
use crate::{dbkey, offline};

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS threads (
    id TEXT PRIMARY KEY,
    last_date_ms INTEGER NOT NULL,
    labels TEXT NOT NULL,
    summary_json TEXT NOT NULL,
    detail_json TEXT,
    search_text TEXT NOT NULL,
    updated_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS threads_date ON threads (last_date_ms DESC);
CREATE TABLE IF NOT EXISTS outbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_ms INTEGER NOT NULL,
    account TEXT NOT NULL,
    to_summary TEXT NOT NULL,
    subject TEXT NOT NULL,
    thread_id TEXT,
    draft_id TEXT,
    raw BLOB NOT NULL,
    state TEXT NOT NULL DEFAULT 'queued',
    error TEXT
);
"#;

/// Po awarii w trakcie wysyłania nie wiadomo, czy wiadomość wyszła – nie wysyłamy jej drugi raz sami.
pub const RECOVER_OUTBOX: &str = "UPDATE outbox SET state = 'uncertain' WHERE state = 'sending'";
pub const ENQUEUE: &str = r#"
INSERT INTO outbox (created_ms, account, to_summary, subject, thread_id, draft_id, raw, state)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'queued')
"#;
pub const COUNT_OUTBOX: &str = "SELECT COUNT(*) FROM outbox";
pub const LIST_OUTBOX: &str =
    "SELECT id, created_ms, to_summary, subject, state, error FROM outbox WHERE account = ?1 ORDER BY id";
pub const NEXT_OUTBOX: &str =
    "SELECT id, raw, thread_id, draft_id FROM outbox WHERE account = ?1 AND state = 'queued' ORDER BY id LIMIT 1";
pub const SET_OUTBOX_STATE: &str = "UPDATE outbox SET state = ?2, error = ?3 WHERE id = ?1 AND account = ?4";
pub const RETRY_OUTBOX: &str =
    "UPDATE outbox SET state = 'queued', error = NULL WHERE id = ?1 AND account = ?2 AND state IN ('failed', 'uncertain')";
pub const DELETE_OUTBOX: &str = "DELETE FROM outbox WHERE id = ?1 AND account = ?2";

/// Podsumowanie z listy. Zapisany szczegół wątku zostaje tylko wtedy, gdy data ostatniej wiadomości się nie zmieniła.
pub const UPSERT_SUMMARY: &str = r#"
INSERT INTO threads (id, last_date_ms, labels, summary_json, detail_json, search_text, updated_ms)
VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6)
ON CONFLICT(id) DO UPDATE SET
    detail_json = CASE WHEN threads.last_date_ms = excluded.last_date_ms THEN threads.detail_json ELSE NULL END,
    search_text = CASE WHEN threads.last_date_ms = excluded.last_date_ms AND length(threads.search_text) > length(excluded.search_text)
                       THEN threads.search_text ELSE excluded.search_text END,
    last_date_ms = excluded.last_date_ms,
    labels = excluded.labels,
    summary_json = excluded.summary_json,
    updated_ms = excluded.updated_ms
"#;

pub const SAVE_DETAIL: &str = r#"
INSERT INTO threads (id, last_date_ms, labels, summary_json, detail_json, search_text, updated_ms)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(id) DO UPDATE SET
    last_date_ms = excluded.last_date_ms,
    labels = excluded.labels,
    summary_json = excluded.summary_json,
    detail_json = excluded.detail_json,
    search_text = excluded.search_text,
    updated_ms = excluded.updated_ms
"#;

pub const PRUNE_THREADS: &str = r#"
DELETE FROM threads WHERE id NOT IN (SELECT id FROM threads ORDER BY last_date_ms DESC LIMIT ?1)
"#;

pub const PRUNE_DETAILS: &str = r#"
UPDATE threads SET detail_json = NULL
WHERE detail_json IS NOT NULL
  AND id NOT IN (SELECT id FROM threads WHERE detail_json IS NOT NULL ORDER BY updated_ms DESC LIMIT ?1)
"#;

pub const INVALIDATE_DETAIL: &str = "UPDATE threads SET detail_json = NULL WHERE id = ?1";
pub const DELETE_THREAD: &str = "DELETE FROM threads WHERE id = ?1";
pub const HAS_THREAD: &str = "SELECT 1 FROM threads WHERE id = ?1";
pub const GET_DETAIL: &str = "SELECT detail_json, labels FROM threads WHERE id = ?1 AND detail_json IS NOT NULL";
pub const COUNT_THREADS: &str = "SELECT COUNT(*) FROM threads";
pub const SET_META: &str = "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value";
pub const GET_META: &str = "SELECT value FROM meta WHERE key = ?1";
pub const DELETE_META: &str = "DELETE FROM meta WHERE key = ?1";
pub const CLEAR_THREADS: &str = "DELETE FROM threads";

const DB_FILE: &str = "rustmail.enc.db";
const LEGACY_FILE: &str = "rustmail.db";
const MAX_OUTBOX: i64 = 50;
const MAX_OUTBOX_BYTES: usize = 30 * 1024 * 1024;
const MAX_THREADS: i64 = 2000;
const MAX_DETAILS: i64 = 200;
const KEY_ENABLED: &str = "offline_enabled";
const KEY_HISTORY: &str = "history_id";

/// Nadpisuje plik zerami i usuwa go (na dyskach SSD nie daje to gwarancji fizycznego wymazania).
fn wipe_and_remove(p: &Path) {
    use std::io::Write;
    if let Ok(meta) = std::fs::metadata(p) {
        if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(p) {
            let zeros = vec![0u8; 64 * 1024];
            let mut left = meta.len();
            while left > 0 {
                let n = left.min(zeros.len() as u64) as usize;
                if f.write_all(&zeros[..n]).is_err() {
                    break;
                }
                left -= n as u64;
            }
            let _ = f.sync_all();
        }
    }
    let _ = std::fs::remove_file(p);
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Uchwyt do bazy. Gdy bazy nie udało się otworzyć, jest „wyłączony” i wszystkie operacje zwracają błąd
/// lub puste wyniki; aplikacja działa dalej online.
#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Option<Connection>>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxItem {
    pub id: i64,
    pub created_ms: i64,
    pub to: String,
    pub subject: String,
    /// queued | sending | failed | uncertain
    pub state: String,
    pub error: Option<String>,
}

pub struct QueuedMail {
    pub id: i64,
    pub raw: Vec<u8>,
    pub thread_id: Option<String>,
    pub draft_id: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStatus {
    pub available: bool,
    pub enabled: bool,
    pub ready: bool,
    pub threads: i64,
    pub db_path: String,
}

impl Store {
    pub fn disabled() -> Store {
        Store { conn: Arc::new(Mutex::new(None)) }
    }

    /// Otwiera (lub tworzy) zaszyfrowaną bazę w podanym katalogu. Przy błędzie zwraca wyłączony Store.
    pub fn open(dir: &Path, key: &str) -> Store {
        match Self::try_open(dir, key) {
            Ok(c) => Store { conn: Arc::new(Mutex::new(Some(c))) },
            Err(_) => Store::disabled(),
        }
    }

    fn open_encrypted(path: &Path, key: &str) -> Result<Connection, String> {
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch(&format!("PRAGMA key = {};", dbkey::sql_literal(key))).map_err(|e| e.to_string())?;
        // Złe hasło lub uszkodzony plik wychodzą dopiero przy pierwszym odczycie.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;").map_err(|e| e.to_string())?;
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        Ok(conn)
    }

    fn try_open(dir: &Path, key: &str) -> Result<Connection, String> {
        if !dbkey::valid_key(key) {
            return Err("Niepoprawny klucz bazy.".to_string());
        }
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
        let path = dir.join(DB_FILE);
        let conn = match Self::open_encrypted(&path, key) {
            Ok(c) => c,
            Err(_) if std::fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false) => {
                // Klucz nie pasuje (np. wyczyszczony portfel kluczy) lub plik jest uszkodzony.
                // Niczego nie kasujemy: plik odkładamy na bok i zaczynamy od czystej bazy.
                for ext in ["", "-wal", "-shm"] {
                    let from = dir.join(format!("{DB_FILE}{ext}"));
                    let _ = std::fs::rename(&from, dir.join(format!("{DB_FILE}.unreadable{ext}")));
                }
                Self::open_encrypted(&path, key)?
            }
            Err(e) => return Err(e),
        };
        Self::migrate_legacy(dir, &conn);
        conn.execute(RECOVER_OUTBOX, []).map_err(|e| e.to_string())?;
        Ok(conn)
    }

    /// Starsze wersje (etapy 7–11) trzymały bazę bez szyfrowania w `rustmail.db`. Przenosimy z niej kolejkę wysyłania
    /// i ustawienie trybu offline (zapisane wątki zbudują się od nowa), a stary plik nadpisujemy zerami i usuwamy.
    fn migrate_legacy(dir: &Path, conn: &Connection) {
        use std::io::Read;
        let old = dir.join(LEGACY_FILE);
        let mut head = [0u8; 16];
        let plain = std::fs::File::open(&old).and_then(|mut f| f.read(&mut head)).map(|n| n == 16).unwrap_or(false)
            && dbkey::is_plain_sqlite(&head);
        if !plain {
            return;
        }
        let attach = format!("ATTACH DATABASE {} AS old KEY '';", dbkey::sql_literal(&old.to_string_lossy()));
        if conn.execute_batch(&attach).is_err() {
            return;
        }
        let missing = |r: &Result<(), rusqlite::Error>| match r {
            Ok(()) => true,
            Err(e) => e.to_string().contains("no such table"),
        };
        let r1 = conn.execute_batch(
            "INSERT OR IGNORE INTO outbox (id, created_ms, account, to_summary, subject, thread_id, draft_id, raw, state, error) \
             SELECT id, created_ms, account, to_summary, subject, thread_id, draft_id, raw, state, error FROM old.outbox;",
        );
        let r2 = conn.execute_batch(
            "INSERT OR IGNORE INTO meta (key, value) SELECT key, value FROM old.meta WHERE key = 'offline_enabled';",
        );
        let _ = conn.execute_batch("DETACH DATABASE old;");
        if missing(&r1) && missing(&r2) {
            for ext in ["", "-wal", "-shm"] {
                wipe_and_remove(&dir.join(format!("{LEGACY_FILE}{ext}")));
            }
        }
    }

    fn with<R>(&self, f: impl FnOnce(&Connection) -> Result<R, rusqlite::Error>) -> Result<R, String> {
        let guard = self.conn.lock().map_err(|_| "Błąd dostępu do lokalnej bazy.".to_string())?;
        let c = guard.as_ref().ok_or_else(|| "Lokalna baza jest niedostępna.".to_string())?;
        f(c).map_err(|e| format!("Błąd lokalnej bazy: {e}"))
    }

    fn meta(&self, key: &str) -> Option<String> {
        self.with(|c| c.query_row(GET_META, [key], |r| r.get::<_, String>(0)).optional()).ok().flatten()
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<(), String> {
        self.with(|c| c.execute(SET_META, params![key, value]).map(|_| ()))
    }

    pub fn available(&self) -> bool {
        self.conn.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    /// Czy użytkownik włączył tryb offline.
    pub fn enabled(&self) -> bool {
        self.meta(KEY_ENABLED).as_deref() == Some("1")
    }

    pub fn history_id(&self) -> Option<String> {
        self.meta(KEY_HISTORY)
    }

    /// Pamięć jest używana tylko po włączeniu i ustaleniu punktu odniesienia synchronizacji.
    /// Dzięki temu nic nie trafia do bazy bez możliwości sprawdzenia, czy jest aktualne.
    pub fn ready(&self) -> bool {
        self.enabled() && self.history_id().is_some()
    }

    pub fn set_history_id(&self, id: &str) -> Result<(), String> {
        self.set_meta(KEY_HISTORY, id)
    }

    /// Włączenie zaczyna od pustej bazy. Wyłączenie usuwa całą zapisaną treść.
    pub fn set_enabled(&self, on: bool) -> Result<(), String> {
        self.clear()?;
        self.set_meta(KEY_ENABLED, if on { "1" } else { "0" })
    }

    /// Usuwa wątki i punkt odniesienia synchronizacji (ustawienie włączenia zostaje).
    pub fn clear(&self) -> Result<(), String> {
        self.with(|c| {
            c.execute(CLEAR_THREADS, [])?;
            c.execute(DELETE_META, [KEY_HISTORY])?;
            // zwolnienie miejsca na dysku po usunięciu treści
            let _ = c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            Ok(())
        })
    }

    /// Usuwa wątki, ale zostawia punkt odniesienia (wywoływane przed ustawieniem nowego).
    pub fn clear_threads(&self) -> Result<(), String> {
        self.with(|c| c.execute(CLEAR_THREADS, []).map(|_| ()))
    }

    pub fn count(&self) -> i64 {
        self.with(|c| c.query_row(COUNT_THREADS, [], |r| r.get::<_, i64>(0))).unwrap_or(0)
    }

    pub fn status(&self, db_path: String) -> CacheStatus {
        CacheStatus {
            available: self.available(),
            enabled: self.enabled(),
            ready: self.ready(),
            threads: self.count(),
            db_path,
        }
    }

    pub fn has_thread(&self, id: &str) -> bool {
        self.with(|c| c.query_row(HAS_THREAD, [id], |_| Ok(())).optional()).ok().flatten().is_some()
    }

    pub fn put_summaries(&self, items: &[ThreadSummary]) -> Result<(), String> {
        if items.is_empty() {
            return Ok(());
        }
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let now = now_ms();
            for t in items {
                let json = match serde_json::to_string(t) {
                    Ok(j) => j,
                    Err(_) => continue,
                };
                let text = offline::build_search_text(&[&t.subject, &t.participants, &t.snippet], 2000);
                tx.execute(
                    UPSERT_SUMMARY,
                    params![t.id, t.last_date_ms, offline::labels_column(&t.labels), json, text, now],
                )?;
            }
            tx.execute(PRUNE_THREADS, [MAX_THREADS])?;
            tx.commit()
        })
    }

    pub fn put_detail(&self, d: &ThreadDetail, summary: &ThreadSummary) -> Result<(), String> {
        let detail_json = serde_json::to_string(d).map_err(|e| e.to_string())?;
        let summary_json = serde_json::to_string(summary).map_err(|e| e.to_string())?;
        let mut parts: Vec<String> = vec![d.subject.clone(), summary.participants.clone()];
        for m in &d.messages {
            parts.push(format!("{} {} {} {}", m.from, m.from_email, m.to, m.cc));
            parts.push(m.body.clone());
        }
        let refs: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
        let text = offline::build_search_text(&refs, offline::MAX_SEARCH_TEXT);
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            tx.execute(
                SAVE_DETAIL,
                params![
                    d.id,
                    summary.last_date_ms,
                    offline::labels_column(&d.labels),
                    summary_json,
                    detail_json,
                    text,
                    now_ms()
                ],
            )?;
            tx.execute(PRUNE_THREADS, [MAX_THREADS])?;
            tx.execute(PRUNE_DETAILS, [MAX_DETAILS])?;
            tx.commit()
        })
    }

    pub fn invalidate_detail(&self, id: &str) -> Result<(), String> {
        self.with(|c| c.execute(INVALIDATE_DETAIL, [id]).map(|_| ()))
    }

    pub fn delete_thread(&self, id: &str) -> Result<(), String> {
        self.with(|c| c.execute(DELETE_THREAD, [id]).map(|_| ()))
    }

    pub fn get_detail(&self, id: &str) -> Option<ThreadDetail> {
        let (json, labels) = self
            .with(|c| c.query_row(GET_DETAIL, [id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).optional())
            .ok()
            .flatten()?;
        let mut d: ThreadDetail = serde_json::from_str(&json).ok()?;
        // Flagi zawsze z aktualnej kolumny etykiet (mogły się zmienić po zapisie szczegółów).
        d.unread = offline::has_label(&labels, "UNREAD");
        d.starred = offline::has_label(&labels, "STARRED");
        d.in_inbox = offline::has_label(&labels, "INBOX");
        d.in_spam = offline::has_label(&labels, "SPAM");
        d.in_trash = offline::has_label(&labels, "TRASH");
        d.is_draft = offline::has_label(&labels, "DRAFT");
        Some(d)
    }

    fn query_summaries(&self, sql: &str, args: Vec<String>) -> Result<Vec<ThreadSummary>, String> {
        self.with(|c| {
            let mut st = c.prepare(sql)?;
            let rows = st.query_map(params_from_iter(args.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut out = Vec::new();
            for row in rows {
                let (json, labels) = row?;
                if let Ok(mut s) = serde_json::from_str::<ThreadSummary>(&json) {
                    s.unread = offline::has_label(&labels, "UNREAD");
                    s.starred = offline::has_label(&labels, "STARRED");
                    out.push(s);
                }
            }
            Ok(out)
        })
    }

    /// Lista folderu z pamięci podręcznej (od najnowszych). Folder „suspicious” filtruje wg zapisanej oceny.
    pub fn list_folder(&self, folder: &str, offset: usize, limit: usize) -> Result<Vec<ThreadSummary>, String> {
        let f = offline::folder_filter(folder).ok_or_else(|| "Nieznany folder.".to_string())?;
        let mut clauses: Vec<String> = Vec::new();
        let mut args: Vec<String> = Vec::new();
        for l in f.must {
            clauses.push("labels LIKE ? ESCAPE '\\'".to_string());
            args.push(offline::label_pattern(l));
        }
        for l in f.must_not {
            clauses.push("labels NOT LIKE ? ESCAPE '\\'".to_string());
            args.push(offline::label_pattern(l));
        }
        let where_sql = if clauses.is_empty() { "1".to_string() } else { clauses.join(" AND ") };
        let suspicious = folder == "suspicious";
        let sql = if suspicious {
            format!("SELECT summary_json, labels FROM threads WHERE {where_sql} ORDER BY last_date_ms DESC LIMIT 1000")
        } else {
            format!(
                "SELECT summary_json, labels FROM threads WHERE {where_sql} ORDER BY last_date_ms DESC LIMIT {} OFFSET {}",
                limit.min(200),
                offset
            )
        };
        let rows = self.query_summaries(&sql, args)?;
        if suspicious {
            return Ok(rows.into_iter().filter(|t| t.risk.level != "none").skip(offset).take(limit).collect());
        }
        Ok(rows)
    }

    /// Wyszukiwanie lokalne: wszystkie słowa muszą wystąpić (temat, uczestnicy, treść zapisanych wątków).
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<ThreadSummary>, String> {
        let tokens = offline::search_tokens(query);
        if tokens.is_empty() {
            return Ok(vec![]);
        }
        let mut clauses: Vec<String> = vec![
            "labels NOT LIKE ? ESCAPE '\\'".to_string(),
            "labels NOT LIKE ? ESCAPE '\\'".to_string(),
        ];
        let mut args: Vec<String> = vec![offline::label_pattern("SPAM"), offline::label_pattern("TRASH")];
        for t in &tokens {
            clauses.push("search_text LIKE ? ESCAPE '\\'".to_string());
            args.push(offline::like_pattern(t));
        }
        let sql = format!(
            "SELECT summary_json, labels FROM threads WHERE {} ORDER BY last_date_ms DESC LIMIT {}",
            clauses.join(" AND "),
            limit.min(100)
        );
        self.query_summaries(&sql, args)
    }

    // ---------- Kolejka wysyłania (działa niezależnie od trybu offline) ----------

    /// Zapisuje przygotowaną wiadomość (gotowy MIME) do późniejszego wysłania. Zwraca jej numer w kolejce.
    pub fn enqueue(
        &self,
        account: &str,
        to: &str,
        subject: &str,
        thread_id: Option<&str>,
        draft_id: Option<&str>,
        raw: &[u8],
    ) -> Result<i64, String> {
        if raw.len() > MAX_OUTBOX_BYTES {
            return Err("Wiadomość jest za duża, aby czekać w kolejce.".to_string());
        }
        let account = account.to_string();
        self.with(|c| {
            let n: i64 = c.query_row(COUNT_OUTBOX, [], |r| r.get(0))?;
            if n >= MAX_OUTBOX {
                return Err(rusqlite::Error::InvalidParameterName("kolejka pełna".to_string()));
            }
            c.execute(ENQUEUE, params![now_ms(), account, to, subject, thread_id, draft_id, raw])?;
            Ok(c.last_insert_rowid())
        })
        .map_err(|e| if e.contains("kolejka pełna") { "Kolejka wysyłania jest pełna (50 wiadomości).".to_string() } else { e })
    }

    pub fn list_outbox(&self, account: &str) -> Result<Vec<OutboxItem>, String> {
        self.with(|c| {
            let mut st = c.prepare(LIST_OUTBOX)?;
            let rows = st.query_map([account], |r| {
                Ok(OutboxItem {
                    id: r.get(0)?,
                    created_ms: r.get(1)?,
                    to: r.get(2)?,
                    subject: r.get(3)?,
                    state: r.get(4)?,
                    error: r.get(5)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    /// Bierze najstarszą wiadomość czekającą na wysłanie i od razu oznacza ją jako „wysyłana” (jedno wywołanie = jeden odbiorca).
    pub fn claim_next(&self, account: &str) -> Result<Option<QueuedMail>, String> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let item = tx
                .query_row(NEXT_OUTBOX, [account], |r| {
                    Ok(QueuedMail { id: r.get(0)?, raw: r.get(1)?, thread_id: r.get(2)?, draft_id: r.get(3)? })
                })
                .optional()?;
            if let Some(m) = &item {
                tx.execute(SET_OUTBOX_STATE, params![m.id, "sending", Option::<String>::None, account])?;
            }
            tx.commit()?;
            Ok(item)
        })
    }

    pub fn set_outbox_state(&self, account: &str, id: i64, state: &str, error: Option<&str>) -> Result<(), String> {
        self.with(|c| c.execute(SET_OUTBOX_STATE, params![id, state, error, account]).map(|_| ()))
    }

    pub fn retry_outbox(&self, account: &str, id: i64) -> Result<(), String> {
        self.with(|c| c.execute(RETRY_OUTBOX, params![id, account]).map(|_| ()))
    }

    pub fn delete_outbox(&self, account: &str, id: i64) -> Result<(), String> {
        self.with(|c| c.execute(DELETE_OUTBOX, params![id, account]).map(|_| ()))
    }
}

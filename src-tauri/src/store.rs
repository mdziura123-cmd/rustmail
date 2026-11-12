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
CREATE TABLE IF NOT EXISTS prio (key TEXT PRIMARY KEY, level INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS contacts (email TEXT PRIMARY KEY, name TEXT NOT NULL, uses INTEGER NOT NULL, last_ms INTEGER NOT NULL);
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
CREATE TABLE IF NOT EXISTS scheduled (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    send_at INTEGER NOT NULL,
    account TEXT NOT NULL,
    to_summary TEXT NOT NULL,
    subject TEXT NOT NULL,
    thread_id TEXT,
    draft_id TEXT,
    raw BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    notes TEXT NOT NULL DEFAULT '',
    start_ms INTEGER NOT NULL,
    end_ms INTEGER NOT NULL,
    all_day INTEGER NOT NULL DEFAULT 0,
    remind_min INTEGER NOT NULL DEFAULT -1,
    reminded INTEGER NOT NULL DEFAULT 0,
    repeat TEXT NOT NULL DEFAULT 'none',
    until_ms INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS events_start ON events (start_ms);
CREATE TABLE IF NOT EXISTS rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    field TEXT NOT NULL,
    needle TEXT NOT NULL,
    action TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS imap_snoozed (
    account TEXT NOT NULL,
    folder TEXT NOT NULL,
    uid INTEGER NOT NULL,
    subject TEXT NOT NULL,
    wake_ms INTEGER NOT NULL,
    PRIMARY KEY (account, folder, uid)
);
CREATE TABLE IF NOT EXISTS snoozed (
    account TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    subject TEXT NOT NULL,
    wake_ms INTEGER NOT NULL,
    PRIMARY KEY (account, thread_id)
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedItem {
    pub id: i64,
    pub send_at_ms: i64,
    pub to: String,
    pub subject: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnoozeItem {
    pub thread_id: String,
    pub subject: String,
    pub wake_ms: i64,
}

const MAX_SCHEDULED: i64 = 100;
const MAX_SNOOZED: i64 = 500;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImapSnoozeItem {
    pub folder: String,
    pub uid: u32,
    pub subject: String,
    pub wake_ms: i64,
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
        // Dodane później kolumny kalendarza (powtarzanie). Dla istniejących baz; błąd „duplicate column” jest oczekiwany.
        for stmt in [
            "ALTER TABLE events ADD COLUMN repeat TEXT NOT NULL DEFAULT 'none'",
            "ALTER TABLE events ADD COLUMN until_ms INTEGER NOT NULL DEFAULT 0",
        ] {
            if let Err(e) = conn.execute(stmt, []) {
                if !e.to_string().contains("duplicate column") {
                    return Err(e.to_string());
                }
            }
        }
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
        // Kopia pliku bazy przy pierwszym starcie nowej wersji (przed jej otwarciem); błąd nie blokuje startu.
        let _ = crate::dbsafe::snapshot(dir, DB_FILE, env!("CARGO_PKG_VERSION"));
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

    fn blob_query(&self, key: &str) -> Result<Option<String>, String> {
        self.with(|c| c.query_row(GET_META, [key], |r| r.get::<_, String>(0)).optional())
    }

    /// Kopia robocza pisanej wiadomości – w zaszyfrowanej bazie (nie w localStorage).
    pub fn backup_set(&self, value: &str) -> Result<(), String> {
        self.set_meta("compose_backup", value)
    }

    pub fn backup_get(&self) -> Option<String> {
        self.meta("compose_backup")
    }

    pub fn backup_clear(&self) -> Result<(), String> {
        self.with(|c| c.execute(DELETE_META, ["compose_backup"]).map(|_| ()))
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

    // ---------- zaplanowana wysyłka ----------

    #[allow(clippy::too_many_arguments)]
    pub fn sched_add(
        &self,
        account: &str,
        send_at_ms: i64,
        to: &str,
        subject: &str,
        thread_id: Option<&str>,
        draft_id: Option<&str>,
        raw: &[u8],
    ) -> Result<i64, String> {
        if raw.len() > MAX_OUTBOX_BYTES {
            return Err("Wiadomość jest za duża, aby czekać na wysłanie.".to_string());
        }
        self.with(|c| {
            let n: i64 = c.query_row("SELECT COUNT(*) FROM scheduled", [], |r| r.get(0))?;
            if n >= MAX_SCHEDULED {
                return Err(rusqlite::Error::InvalidParameterName("za dużo zaplanowanych".to_string()));
            }
            c.execute(
                "INSERT INTO scheduled (send_at, account, to_summary, subject, thread_id, draft_id, raw) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![send_at_ms, account, to, subject, thread_id, draft_id, raw],
            )?;
            Ok(c.last_insert_rowid())
        })
        .map_err(|e| if e.contains("za dużo zaplanowanych") { "Zaplanowano już 100 wiadomości.".to_string() } else { e })
    }

    pub fn sched_list(&self, account: &str) -> Result<Vec<SchedItem>, String> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id, send_at, to_summary, subject FROM scheduled WHERE account = ?1 ORDER BY send_at, id")?;
            let rows = st.query_map([account], |r| Ok(SchedItem { id: r.get(0)?, send_at_ms: r.get(1)?, to: r.get(2)?, subject: r.get(3)? }))?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    pub fn sched_delete(&self, account: &str, id: i64) -> Result<(), String> {
        self.with(|c| c.execute("DELETE FROM scheduled WHERE id = ?1 AND account = ?2", params![id, account]).map(|_| ()))
    }

    /// Przenosi zaplanowane wiadomości, którym minął czas, do zwykłej kolejki wysyłania (jedna transakcja).
    /// Zwraca klucze kont, do których coś trafiło.
    pub fn sched_release(&self, now: i64) -> Result<Vec<String>, String> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let mut keys: Vec<String> = Vec::new();
            {
                let mut st = tx.prepare("SELECT id, account, to_summary, subject, thread_id, draft_id, raw FROM scheduled WHERE send_at <= ?1 ORDER BY send_at, id")?;
                let due = st
                    .query_map([now], |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, Option<String>>(4)?,
                            r.get::<_, Option<String>>(5)?,
                            r.get::<_, Vec<u8>>(6)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                for (id, account, to, subject, thread, draft, raw) in due {
                    tx.execute(ENQUEUE, params![now, account, to, subject, thread, draft, raw])?;
                    tx.execute("DELETE FROM scheduled WHERE id = ?1", [id])?;
                    if !keys.contains(&account) {
                        keys.push(account);
                    }
                }
            }
            tx.commit()?;
            Ok(keys)
        })
    }

    // ---------- reguły automatyczne ----------

    pub fn rules_list(&self) -> Result<Vec<crate::rules::Rule>, String> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id, field, needle, action FROM rules ORDER BY id")?;
            let rows = st.query_map([], |r| Ok(crate::rules::Rule { id: r.get(0)?, field: r.get(1)?, needle: r.get(2)?, action: r.get(3)? }))?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    pub fn rule_add(&self, field: &str, needle: &str, action: &str) -> Result<i64, String> {
        self.with(|c| {
            let n: i64 = c.query_row("SELECT COUNT(*) FROM rules", [], |r| r.get(0))?;
            if n >= crate::rules::MAX_RULES as i64 {
                return Err(rusqlite::Error::InvalidParameterName("za dużo reguł".to_string()));
            }
            c.execute("INSERT INTO rules (field, needle, action) VALUES (?1, ?2, ?3)", params![field, needle, action])?;
            Ok(c.last_insert_rowid())
        })
        .map_err(|e| if e.contains("za dużo reguł") { "Można mieć najwyżej 100 reguł.".to_string() } else { e })
    }

    // ---------- kalendarz lokalny ----------

    pub fn event_save(&self, e: &crate::calendar::Event) -> Result<i64, String> {
        self.with(|c| match e.id {
            Some(id) => {
                let n = c.execute(
                    "UPDATE events SET title=?1, notes=?2, start_ms=?3, end_ms=?4, all_day=?5, remind_min=?6, repeat=?7, until_ms=?8, reminded=0 WHERE id=?9",
                    params![e.title, e.notes, e.start_ms, e.end_ms, e.all_day as i64, e.remind_min, e.repeat, e.until_ms, id],
                )?;
                if n == 0 {
                    return Err(rusqlite::Error::QueryReturnedNoRows);
                }
                Ok(id)
            }
            None => {
                let n: i64 = c.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
                if n >= crate::calendar::MAX_EVENTS {
                    return Err(rusqlite::Error::InvalidParameterName("za dużo wydarzeń".to_string()));
                }
                c.execute(
                    "INSERT INTO events (title, notes, start_ms, end_ms, all_day, remind_min, repeat, until_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![e.title, e.notes, e.start_ms, e.end_ms, e.all_day as i64, e.remind_min, e.repeat, e.until_ms],
                )?;
                Ok(c.last_insert_rowid())
            }
        })
        .map_err(|m| if m.contains("za dużo wydarzeń") { format!("Można mieć najwyżej {} wydarzeń.", crate::calendar::MAX_EVENTS) } else { m })
    }

    pub fn event_delete(&self, id: i64) -> Result<(), String> {
        self.with(|c| c.execute("DELETE FROM events WHERE id = ?1", [id]).map(|_| ()))
    }

    fn event_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<crate::calendar::Event> {
        Ok(crate::calendar::Event {
            id: Some(r.get(0)?),
            title: r.get(1)?,
            notes: r.get(2)?,
            start_ms: r.get(3)?,
            end_ms: r.get(4)?,
            all_day: r.get::<_, i64>(5)? != 0,
            remind_min: r.get(6)?,
            repeat: r.get(7)?,
            until_ms: r.get(8)?,
            reminded: r.get(9)?,
        })
    }

    /// Wydarzenia, które zaczynają się lub trwają w podanym przedziale czasu.
    pub fn event_range(&self, from_ms: i64, to_ms: i64) -> Result<Vec<crate::calendar::Event>, String> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, title, notes, start_ms, end_ms, all_day, remind_min, repeat, until_ms, reminded FROM events \
                 WHERE start_ms < ?2 AND ((repeat = 'none' AND end_ms >= ?1) OR (repeat != 'none' AND (until_ms = 0 OR until_ms >= ?1 - (end_ms - start_ms)))) \
                 ORDER BY start_ms, id LIMIT 2000",
            )?;
            let rows = st.query_map(params![from_ms, to_ms], Self::event_from_row)?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    pub fn event_all(&self) -> Result<Vec<crate::calendar::Event>, String> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id, title, notes, start_ms, end_ms, all_day, remind_min, repeat, until_ms, reminded FROM events ORDER BY start_ms, id")?;
            let rows = st.query_map([], Self::event_from_row)?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    pub fn event_recurring(&self) -> Result<Vec<crate::calendar::Event>, String> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, title, notes, start_ms, end_ms, all_day, remind_min, repeat, until_ms, reminded FROM events \
                 WHERE repeat != 'none' AND remind_min >= 0 ORDER BY start_ms LIMIT 1000",
            )?;
            let rows = st.query_map([], Self::event_from_row)?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    pub fn event_mark(&self, id: i64, occ_ms: i64) -> Result<(), String> {
        self.with(|c| c.execute("UPDATE events SET reminded = ?2 WHERE id = ?1", params![id, occ_ms]).map(|_| ()))
    }

    /// Wydarzenia z przypomnieniem, na które przyszedł czas; oznacza je jako przypomniane.
    /// Zwraca tylko te, które zaczynają się nie dawniej niż dobę temu (starych nie ogłaszamy po fakcie).
    pub fn event_due(&self, now_ms: i64) -> Result<Vec<crate::calendar::Event>, String> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let due: Vec<crate::calendar::Event> = {
                let mut st = tx.prepare(
                    "SELECT id, title, notes, start_ms, end_ms, all_day, remind_min, repeat, until_ms, reminded FROM events \
                     WHERE repeat = 'none' AND remind_min >= 0 AND reminded = 0 AND start_ms - remind_min * 60000 <= ?1 ORDER BY start_ms LIMIT 200",
                )?;
                let rows = st.query_map([now_ms], Self::event_from_row)?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            for e in &due {
                tx.execute("UPDATE events SET reminded = 1 WHERE id = ?1", [e.id])?;
            }
            tx.commit()?;
            Ok(due.into_iter().filter(|e| e.start_ms >= now_ms - 86_400_000).collect())
        })
    }

    pub fn rule_delete(&self, id: i64) -> Result<(), String> {
        self.with(|c| c.execute("DELETE FROM rules WHERE id = ?1", [id]).map(|_| ()))
    }

    // ---------- odłożone wiadomości (Gmail) ----------

    pub fn snooze_add(&self, account: &str, thread_id: &str, subject: &str, wake_ms: i64) -> Result<(), String> {
        self.with(|c| {
            let n: i64 = c.query_row("SELECT COUNT(*) FROM snoozed", [], |r| r.get(0))?;
            if n >= MAX_SNOOZED {
                return Err(rusqlite::Error::InvalidParameterName("za dużo odłożonych".to_string()));
            }
            c.execute(
                "INSERT INTO snoozed (account, thread_id, subject, wake_ms) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(account, thread_id) DO UPDATE SET wake_ms = excluded.wake_ms, subject = excluded.subject",
                params![account, thread_id, subject, wake_ms],
            )
            .map(|_| ())
        })
        .map_err(|e| if e.contains("za dużo odłożonych") { "Odłożono już 500 wiadomości.".to_string() } else { e })
    }

    pub fn snooze_list(&self, account: &str, due_before: Option<i64>) -> Result<Vec<SnoozeItem>, String> {
        self.with(|c| {
            let mut st = c.prepare("SELECT thread_id, subject, wake_ms FROM snoozed WHERE account = ?1 AND wake_ms <= ?2 ORDER BY wake_ms")?;
            let rows = st.query_map(params![account, due_before.unwrap_or(i64::MAX)], |r| {
                Ok(SnoozeItem { thread_id: r.get(0)?, subject: r.get(1)?, wake_ms: r.get(2)? })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    // ---------- odłożone wiadomości IMAP (lokalnie ukrywane do wskazanej pory) ----------

    pub fn imap_snooze_add(&self, account: &str, folder: &str, uid: u32, subject: &str, wake_ms: i64) -> Result<(), String> {
        self.with(|c| {
            let n: i64 = c.query_row("SELECT COUNT(*) FROM imap_snoozed", [], |r| r.get(0))?;
            if n >= MAX_SNOOZED {
                return Err(rusqlite::Error::InvalidParameterName("za dużo odłożonych".to_string()));
            }
            c.execute(
                "INSERT INTO imap_snoozed (account, folder, uid, subject, wake_ms) VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT(account, folder, uid) DO UPDATE SET wake_ms = excluded.wake_ms, subject = excluded.subject",
                params![account, folder, uid as i64, subject, wake_ms],
            )
            .map(|_| ())
        })
        .map_err(|e| if e.contains("za dużo odłożonych") { "Odłożono już 500 wiadomości.".to_string() } else { e })
    }

    /// Numery wiadomości ukrytych w danym folderze (do odfiltrowania z listy).
    pub fn imap_snooze_hidden(&self, account: &str, folder: &str) -> Vec<u32> {
        self.with(|c| {
            let mut st = c.prepare("SELECT uid FROM imap_snoozed WHERE account = ?1 AND folder = ?2")?;
            let rows = st.query_map(params![account, folder], |r| r.get::<_, i64>(0))?;
            rows.map(|r| r.map(|n| n as u32)).collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default()
    }

    pub fn imap_snooze_list(&self, account: &str, due_before: Option<i64>) -> Result<Vec<ImapSnoozeItem>, String> {
        self.with(|c| {
            let mut st = c.prepare("SELECT folder, uid, subject, wake_ms FROM imap_snoozed WHERE account = ?1 AND wake_ms <= ?2 ORDER BY wake_ms")?;
            let rows = st.query_map(params![account, due_before.unwrap_or(i64::MAX)], |r| {
                Ok(ImapSnoozeItem { folder: r.get(0)?, uid: r.get::<_, i64>(1)? as u32, subject: r.get(2)?, wake_ms: r.get(3)? })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    pub fn imap_snooze_delete(&self, account: &str, folder: &str, uid: u32) -> Result<(), String> {
        self.with(|c| c.execute("DELETE FROM imap_snoozed WHERE account = ?1 AND folder = ?2 AND uid = ?3", params![account, folder, uid as i64]).map(|_| ()))
    }

    pub fn snooze_delete(&self, account: &str, thread_id: &str) -> Result<(), String> {
        self.with(|c| c.execute("DELETE FROM snoozed WHERE account = ?1 AND thread_id = ?2", params![account, thread_id]).map(|_| ()))
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

    // ---------- Flagi ważności (lokalne, w zaszyfrowanej bazie) ----------

    /// Zapamiętuje adresy, do których wysłano wiadomość (licznik użyć rośnie, nazwa uzupełnia się, gdy jej brakowało).
    pub fn contacts_remember(&self, addrs: &[crate::contacts::Addr], now_ms: i64) -> Result<(), String> {
        if addrs.is_empty() {
            return Ok(());
        }
        self.with(|c| {
            for a in addrs {
                c.execute(
                    "INSERT INTO contacts (email, name, uses, last_ms) VALUES (?1, ?2, 1, ?3) \
                     ON CONFLICT(email) DO UPDATE SET uses = uses + 1, last_ms = excluded.last_ms, \
                     name = CASE WHEN excluded.name <> '' THEN excluded.name ELSE contacts.name END",
                    params![a.email, a.name, now_ms],
                )?;
            }
            c.execute(
                "DELETE FROM contacts WHERE email IN (SELECT email FROM contacts ORDER BY uses ASC, last_ms ASC \
                 LIMIT MAX(0, (SELECT COUNT(*) FROM contacts) - ?1))",
                [crate::contacts::MAX_CONTACTS],
            )?;
            Ok(())
        })
    }

    /// Uzupełnia książkę adresów adresami z poczty. Liczba użyć ustawiana jest na większą z wartości (powtórne
    /// uruchomienie niczego nie zawyża), a nazwa uzupełnia się tylko gdy jej brakowało.
    pub fn contacts_learn(&self, items: &[(crate::contacts::Addr, i64)]) -> Result<usize, String> {
        if items.is_empty() {
            return Ok(0);
        }
        self.with(|c| {
            c.execute_batch("BEGIN")?;
            let mut n = 0usize;
            for (a, uses) in items {
                let r = c.execute(
                    "INSERT INTO contacts (email, name, uses, last_ms) VALUES (?1, ?2, ?3, 0) \
                     ON CONFLICT(email) DO UPDATE SET uses = MAX(uses, excluded.uses), \
                     name = CASE WHEN contacts.name = '' THEN excluded.name ELSE contacts.name END",
                    params![a.email, a.name, uses],
                );
                if let Err(e) = r {
                    let _ = c.execute_batch("ROLLBACK");
                    return Err(e);
                }
                n += 1;
            }
            c.execute(
                "DELETE FROM contacts WHERE email IN (SELECT email FROM contacts ORDER BY uses ASC, last_ms ASC \
                 LIMIT MAX(0, (SELECT COUNT(*) FROM contacts) - ?1))",
                [crate::contacts::MAX_CONTACTS],
            )?;
            c.execute_batch("COMMIT")?;
            Ok(n)
        })
    }

    /// Podpowiedzi adresów: najpierw te zaczynające się od wpisanego tekstu, potem najczęściej i ostatnio używane.
    pub fn contacts_suggest(&self, q: &str, limit: usize) -> Result<Vec<(String, String)>, String> {
        let q = q.trim().to_lowercase();
        if q.is_empty() || q.chars().count() > 100 {
            return Ok(Vec::new());
        }
        let esc = crate::contacts::like_escape(&q);
        let (any, prefix) = (format!("%{esc}%"), format!("{esc}%"));
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT email, name FROM contacts \
                 WHERE email LIKE ?1 ESCAPE '\\' OR lower(name) LIKE ?1 ESCAPE '\\' \
                 ORDER BY (CASE WHEN email LIKE ?2 ESCAPE '\\' OR lower(name) LIKE ?2 ESCAPE '\\' OR lower(name) LIKE ?3 ESCAPE '\\' THEN 0 ELSE 1 END), \
                          uses DESC, last_ms DESC LIMIT ?4",
            )?;
            let rows = st.query_map(params![any, prefix, format!("% {esc}%"), limit.clamp(1, 20) as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            rows.collect()
        })
    }

    pub fn contacts_list(&self) -> Result<Vec<(String, String)>, String> {
        self.with(|c| {
            let mut st = c.prepare("SELECT email, name FROM contacts ORDER BY lower(CASE WHEN name <> '' THEN name ELSE email END) LIMIT 3000")?;
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            rows.collect()
        })
    }

    pub fn contacts_delete(&self, email: &str) -> Result<(), String> {
        self.with(|c| c.execute("DELETE FROM contacts WHERE email = ?1", [email.trim().to_lowercase()]).map(|_| ()))
    }

    pub fn contacts_clear(&self) -> Result<(), String> {
        self.with(|c| c.execute("DELETE FROM contacts", []).map(|_| ()))
    }

    /// Ustawia flagę (1 = bardzo ważne, 2 = ważne, 3 = mniej ważne); 0 usuwa flagę.
    pub fn prio_set(&self, key: &str, level: u8) -> Result<(), String> {
        if !crate::prio::valid_key(key) || level > 3 {
            return Err("Niepoprawna flaga.".to_string());
        }
        self.with(|c| {
            if level == 0 {
                c.execute("DELETE FROM prio WHERE key = ?1", [key]).map(|_| ())
            } else {
                c.execute(
                    "INSERT INTO prio (key, level) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET level = excluded.level",
                    params![key, level as i64],
                )
                .map(|_| ())
            }
        })
    }

    /// Wszystkie flagi (do kopii zapasowej); najwyżej `limit` wpisów.
    pub fn prio_list(&self, limit: usize) -> Result<Vec<(String, u8)>, String> {
        self.with(|c| {
            let mut st = c.prepare("SELECT key, level FROM prio ORDER BY key LIMIT ?1")?;
            let rows = st.query_map([limit as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.clamp(0, 3) as u8)))?;
            rows.collect()
        })
    }

    /// Flagi dla podanych kluczy (brakujące klucze oznaczają brak flagi).
    pub fn prio_get(&self, keys: &[String]) -> Result<Vec<(String, u8)>, String> {
        self.with(|c| {
            let mut out = Vec::new();
            let mut st = c.prepare("SELECT level FROM prio WHERE key = ?1")?;
            for k in keys.iter().filter(|k| crate::prio::valid_key(k)).take(crate::prio::MAX_KEYS) {
                if let Some(l) = st.query_row([k], |r| r.get::<_, i64>(0)).optional()? {
                    out.push((k.clone(), l.clamp(0, 3) as u8));
                }
            }
            Ok(out)
        })
    }

    /// Usuwa wszystkie flagi o kluczach zaczynających się od prefiksu (np. po wylogowaniu z konta).
    pub fn prio_purge(&self, prefix: &str) -> Result<(), String> {
        if !crate::prio::valid_key(prefix) {
            return Ok(());
        }
        let pat = format!("{}%", prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        self.with(|c| c.execute("DELETE FROM prio WHERE key LIKE ?1 ESCAPE '\\'", [pat]).map(|_| ()))
    }

    /// Usuwa z kolejki wszystkie wiadomości danego konta (np. po wylogowaniu).
    pub fn purge_outbox(&self, account: &str) -> Result<(), String> {
        self.with(|c| c.execute("DELETE FROM outbox WHERE account = ?1", [account]).map(|_| ()))
    }

    pub fn retry_outbox(&self, account: &str, id: i64) -> Result<(), String> {
        self.with(|c| c.execute(RETRY_OUTBOX, params![id, account]).map(|_| ()))
    }

    pub fn delete_outbox(&self, account: &str, id: i64) -> Result<(), String> {
        self.with(|c| c.execute(DELETE_OUTBOX, params![id, account]).map(|_| ()))
    }
}

/// Małe dane tekstowe (dziennik i model filtra spamu) trzymamy w tabeli `meta` zaszyfrowanej bazy.
impl crate::blobmig::BlobDb for Store {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        self.blob_query(key)
    }
    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.set_meta(key, value)
    }
    fn delete(&self, key: &str) -> Result<(), String> {
        self.with(|c| c.execute(DELETE_META, [key]).map(|_| ()))
    }
}

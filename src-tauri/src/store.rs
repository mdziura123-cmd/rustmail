//! Lokalna baza SQLite (tryb offline). Włączana wyłącznie na życzenie użytkownika.
//! UWAGA: baza NIE jest szyfrowana. Chroni ją tylko uprawnienia plików (katalog 0700, plik 0600)
//! i ewentualne szyfrowanie dysku. Tokeny i hasła nigdy do niej nie trafiają (są w pęku kluczy).

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

use crate::gmail::{ThreadDetail, ThreadSummary};
use crate::offline;

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
"#;

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

const MAX_THREADS: i64 = 2000;
const MAX_DETAILS: i64 = 200;
const KEY_ENABLED: &str = "offline_enabled";
const KEY_HISTORY: &str = "history_id";

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

    /// Otwiera (lub tworzy) bazę w podanym katalogu. Przy błędzie zwraca wyłączony Store.
    pub fn open(dir: &Path) -> Store {
        match Self::try_open(dir) {
            Ok(c) => Store { conn: Arc::new(Mutex::new(Some(c))) },
            Err(_) => Store::disabled(),
        }
    }

    fn try_open(dir: &Path) -> Result<Connection, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
        let path = dir.join("rustmail.db");
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;").map_err(|e| e.to_string())?;
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        Ok(conn)
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
}

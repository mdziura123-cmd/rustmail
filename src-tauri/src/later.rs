//! „Na później”: zaplanowana wysyłka (wiadomość czeka zaszyfrowana w lokalnej bazie do wskazanej godziny,
//! potem trafia do zwykłej kolejki wysyłania) i odkładanie wiadomości Gmaila (wracają do skrzynki o wskazanej godzinie).

use tauri::State;

use crate::{auth, commands::OutgoingDto, secrets, store::{SchedItem, SnoozeItem, Store}};

const MAX_AHEAD_MS: i64 = 366 * 24 * 3600 * 1000;

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Czas musi być w przyszłości (z minutą tolerancji na różnice zegarów) i nie dalej niż rok.
pub fn check_time(at_ms: i64, now: i64) -> Result<(), String> {
    if at_ms < now - 60_000 {
        return Err("Wybrany czas już minął.".to_string());
    }
    if at_ms > now + MAX_AHEAD_MS {
        return Err("Można zaplanować najwyżej rok naprzód.".to_string());
    }
    Ok(())
}

/// Klucz kolejki: Gmail = adres konta, IMAP = „imap:” + adres (jak w kolejce wysyłania).
fn key_for(account: Option<&str>) -> Result<String, String> {
    match account {
        Some(a) => Ok(format!("imap:{}", a.to_ascii_lowercase())),
        None => Ok(secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?),
    }
}

#[tauri::command]
pub fn schedule_message(store: State<'_, Store>, msg: OutgoingDto, send_at_ms: i64) -> Result<i64, String> {
    check_time(send_at_ms, now_ms())?;
    let from = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    let raw = crate::commands::prepare(&msg, &from, true)?;
    let subject: String = crate::compose::clean_header_value(&msg.subject).chars().take(200).collect();
    store.sched_add(
        &from,
        send_at_ms,
        &crate::commands::recipients_summary(&msg),
        &subject,
        msg.thread_id.as_deref().filter(|t| !t.is_empty()),
        msg.draft_id.as_deref().filter(|d| !d.is_empty()),
        &raw,
    )
}

#[tauri::command]
pub fn schedule_list(store: State<'_, Store>, account: Option<String>) -> Result<Vec<SchedItem>, String> {
    store.sched_list(&key_for(account.as_deref())?)
}

#[tauri::command]
pub fn schedule_delete(store: State<'_, Store>, account: Option<String>, id: i64) -> Result<(), String> {
    store.sched_delete(&key_for(account.as_deref())?, id)
}

/// Przenosi wiadomości, którym minął czas, do kolejki wysyłania. Zwraca konta (None = Gmail), które mają co wysłać.
#[tauri::command]
pub fn schedule_release(store: State<'_, Store>) -> Result<Vec<Option<String>>, String> {
    let keys = store.sched_release(now_ms())?;
    Ok(keys.into_iter().map(|k| k.strip_prefix("imap:").map(str::to_string)).collect())
}

#[tauri::command]
pub fn snooze_add(store: State<'_, Store>, thread_id: String, subject: String, wake_ms: i64) -> Result<(), String> {
    check_time(wake_ms, now_ms())?;
    if thread_id.is_empty() || thread_id.len() > 200 {
        return Err("Niepoprawna wiadomość.".to_string());
    }
    let account = key_for(None)?;
    let subject: String = crate::compose::clean_header_value(&subject).chars().take(200).collect();
    store.snooze_add(&account, &thread_id, &subject, wake_ms)
}

#[tauri::command]
pub fn snooze_list(store: State<'_, Store>) -> Result<Vec<SnoozeItem>, String> {
    store.snooze_list(&key_for(None)?, None)
}

/// Odłożone wiadomości, którym minął czas (do przywrócenia do skrzynki).
#[tauri::command]
pub fn snooze_due(store: State<'_, Store>) -> Result<Vec<SnoozeItem>, String> {
    store.snooze_list(&key_for(None)?, Some(now_ms()))
}

#[tauri::command]
pub fn snooze_done(store: State<'_, Store>, thread_id: String) -> Result<(), String> {
    store.snooze_delete(&key_for(None)?, &thread_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_must_be_in_range() {
        let now = 1_000_000_000_000;
        assert!(check_time(now + 3_600_000, now).is_ok());
        assert!(check_time(now - 30_000, now).is_ok());
        assert!(check_time(now - 120_000, now).is_err());
        assert!(check_time(now + MAX_AHEAD_MS + 1, now).is_err());
    }
}

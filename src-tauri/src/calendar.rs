//! Kalendarz lokalny: wydarzenia w zaszyfrowanej bazie, niepołączone z pocztą ani z żadnym kontem.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::store::Store;

pub const MAX_EVENTS: i64 = 5000;
const MAX_TITLE: usize = 120;
const MAX_NOTES: usize = 2000;
/// Najdalsze przypomnienie: tydzień wcześniej.
const MAX_REMIND_MIN: i64 = 7 * 24 * 60;
/// Najdłuższe wydarzenie: rok.
const MAX_SPAN_MS: i64 = 366 * 86_400_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    #[serde(default)]
    pub id: Option<i64>,
    pub title: String,
    #[serde(default)]
    pub notes: String,
    pub start_ms: i64,
    pub end_ms: i64,
    #[serde(default)]
    pub all_day: bool,
    /// Ile minut przed początkiem przypomnieć; -1 = bez przypomnienia.
    #[serde(default = "no_remind")]
    pub remind_min: i64,
}

fn no_remind() -> i64 {
    -1
}

/// Sprawdza i porządkuje wydarzenie (usuwa znaki sterujące, przycina długość, pilnuje kolejności czasu).
pub fn validate(e: &Event) -> Result<Event, String> {
    let clean = |s: &str, max: usize, keep_nl: bool| -> String {
        s.chars().filter(|c| !c.is_control() || (keep_nl && *c == '\n')).take(max).collect::<String>().trim().to_string()
    };
    let title = clean(&e.title, MAX_TITLE, false);
    if title.is_empty() {
        return Err("Podaj tytuł wydarzenia.".to_string());
    }
    if e.start_ms <= 0 || e.end_ms <= 0 {
        return Err("Niepoprawna data wydarzenia.".to_string());
    }
    if e.end_ms < e.start_ms {
        return Err("Koniec wydarzenia nie może być przed jego początkiem.".to_string());
    }
    if e.end_ms - e.start_ms > MAX_SPAN_MS {
        return Err("Wydarzenie może trwać najwyżej rok.".to_string());
    }
    if e.remind_min < -1 || e.remind_min > MAX_REMIND_MIN {
        return Err("Niepoprawny czas przypomnienia.".to_string());
    }
    Ok(Event {
        id: e.id,
        title,
        notes: clean(&e.notes, MAX_NOTES, true),
        start_ms: e.start_ms,
        end_ms: e.end_ms,
        all_day: e.all_day,
        remind_min: e.remind_min,
    })
}

#[tauri::command]
pub fn cal_range(store: State<'_, Store>, from_ms: i64, to_ms: i64) -> Result<Vec<Event>, String> {
    store.event_range(from_ms, to_ms)
}

#[tauri::command]
pub fn cal_save(store: State<'_, Store>, event: Event) -> Result<i64, String> {
    store.event_save(&validate(&event)?)
}

#[tauri::command]
pub fn cal_delete(store: State<'_, Store>, id: i64) -> Result<(), String> {
    store.event_delete(id)
}

/// Przypomnienia, na które przyszedł czas (każde zwracane tylko raz).
#[tauri::command]
pub fn cal_due(store: State<'_, Store>, now_ms: i64) -> Result<Vec<Event>, String> {
    store.event_due(now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev() -> Event {
        Event { id: None, title: "  Wizyta ".into(), notes: "a\nb\u{7}".into(), start_ms: 1000, end_ms: 2000, all_day: false, remind_min: 15 }
    }

    #[test]
    fn cleans_and_accepts() {
        let v = validate(&ev()).unwrap();
        assert_eq!(v.title, "Wizyta");
        assert_eq!(v.notes, "a\nb");
    }

    #[test]
    fn rejects_bad_input() {
        let mut e = ev();
        e.title = "   ".into();
        assert!(validate(&e).is_err());
        let mut e = ev();
        e.end_ms = 500;
        assert!(validate(&e).is_err());
        let mut e = ev();
        e.remind_min = -5;
        assert!(validate(&e).is_err());
        let mut e = ev();
        e.remind_min = MAX_REMIND_MIN + 1;
        assert!(validate(&e).is_err());
        let mut e = ev();
        e.end_ms = e.start_ms + MAX_SPAN_MS + 1;
        assert!(validate(&e).is_err());
    }
}

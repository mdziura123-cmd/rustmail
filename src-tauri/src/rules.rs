//! Reguły automatyczne dla nowych wiadomości: jeśli nadawca albo temat zawiera podany tekst, wiadomość jest
//! oznaczana jako przeczytana, archiwizowana albo dostaje flagę ważności. Reguły działają tylko na wiadomościach,
//! które przyszły po uruchomieniu aplikacji (stare nie są ruszane) i są zapisane lokalnie w zaszyfrowanej bazie.

use serde::Serialize;
use tauri::State;

use crate::store::Store;

pub const FIELDS: [&str; 2] = ["from", "subject"];
pub const ACTIONS: [&str; 5] = ["read", "archive", "flag1", "flag2", "flag3"];
pub const MAX_RULES: usize = 100;
const MAX_NEEDLE: usize = 120;

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: i64,
    pub field: String,
    pub needle: String,
    pub action: String,
}

/// Sprawdza i czyści dane reguły; zwraca przycięty tekst do szukania.
pub fn validate(field: &str, needle: &str, action: &str) -> Result<String, String> {
    if !FIELDS.contains(&field) {
        return Err("Niepoprawne pole reguły.".to_string());
    }
    if !ACTIONS.contains(&action) {
        return Err("Niepoprawna akcja reguły.".to_string());
    }
    let n: String = needle.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string();
    if n.chars().count() < 2 {
        return Err("Wpisz co najmniej 2 znaki do dopasowania.".to_string());
    }
    if n.chars().count() > MAX_NEEDLE {
        return Err("Tekst do dopasowania jest za długi.".to_string());
    }
    Ok(n)
}

pub fn matches(r: &Rule, from: &str, subject: &str) -> bool {
    let hay = if r.field == "from" { from } else { subject };
    !r.needle.is_empty() && hay.to_lowercase().contains(&r.needle.to_lowercase())
}

/// Akcje wszystkich pasujących reguł, bez powtórzeń, w kolejności dodania.
pub fn actions_for<'a>(rules: &'a [Rule], from: &str, subject: &str) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    for r in rules.iter().filter(|r| matches(r, from, subject)) {
        if !out.contains(&r.action.as_str()) {
            out.push(r.action.as_str());
        }
    }
    out
}

/// Poziom flagi z nazwy akcji (`flag2` → 2).
pub fn flag_level(action: &str) -> Option<u8> {
    action.strip_prefix("flag").and_then(|n| n.parse::<u8>().ok()).filter(|n| (1..=3).contains(n))
}

#[tauri::command]
pub fn rules_list(store: State<'_, Store>) -> Result<Vec<Rule>, String> {
    store.rules_list()
}

#[tauri::command]
pub fn rules_add(store: State<'_, Store>, field: String, needle: String, action: String) -> Result<i64, String> {
    let needle = validate(&field, &needle, &action)?;
    store.rule_add(&field, &needle, &action)
}

#[tauri::command]
pub fn rules_delete(store: State<'_, Store>, id: i64) -> Result<(), String> {
    store.rule_delete(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(id: i64, field: &str, needle: &str, action: &str) -> Rule {
        Rule { id, field: field.into(), needle: needle.into(), action: action.into() }
    }

    #[test]
    fn validates_input() {
        assert_eq!(validate("from", "  @news.pl ", "archive").unwrap(), "@news.pl");
        assert!(validate("body", "abc", "read").is_err());
        assert!(validate("from", "abc", "delete").is_err());
        assert!(validate("subject", "a", "read").is_err());
        assert!(validate("subject", &"x".repeat(121), "read").is_err());
    }

    #[test]
    fn matches_case_insensitively_and_collects_actions() {
        let rules = vec![r(1, "from", "NEWS", "archive"), r(2, "subject", "faktura", "flag1"), r(3, "from", "news", "archive")];
        assert_eq!(actions_for(&rules, "Daily News <a@b.pl>", "Cześć"), vec!["archive"]);
        assert_eq!(actions_for(&rules, "ktoś", "Twoja FAKTURA"), vec!["flag1"]);
        assert!(actions_for(&rules, "ktoś", "nic").is_empty());
    }

    #[test]
    fn flag_levels() {
        assert_eq!(flag_level("flag3"), Some(3));
        assert_eq!(flag_level("flag4"), None);
        assert_eq!(flag_level("read"), None);
    }
}

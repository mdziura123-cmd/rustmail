//! Kopia zapasowa danych lokalnych (nie ma ich na serwerze poczty): książka adresów, flagi ważności
//! i wybrane ustawienia wyglądu. Plik JSON nie zawiera haseł, tokenów ani treści wiadomości.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

use crate::{contacts, remote, store::Store};

const FORMAT: &str = "rustmail-backup";
const VERSION: u64 = 1;
const MAX_ITEMS: usize = 5000;
const MAX_BYTES: usize = 5_000_000;
const MAX_SETTING_LEN: usize = 80_000;

/// Ustawienia, które wolno zapisać w kopii i przywrócić (tylko wygląd i zachowanie, nic wrażliwego).
pub const SETTING_KEYS: [&str; 16] = [
    "rm.theme", "rm.notify", "rm.notifycontent", "rm.sound", "rm.activeonly", "rm.background", "rm.flagsfirst",
    "rm.keys", "rm.scale", "rm.labels", "rm.autoupdate", "rm.left", "rm.mid", "rm.spell", "rm.undo", "rm.templates",
];

/// Zostawia tylko dozwolone klucze z rozsądnie krótkimi wartościami.
pub fn clean_settings(input: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    input
        .iter()
        .filter(|(k, v)| SETTING_KEYS.contains(&k.as_str()) && v.len() <= MAX_SETTING_LEN && !v.contains('\0'))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[derive(Serialize)]
pub struct ImportResult {
    contacts: usize,
    flags: usize,
    rules: usize,
    events: usize,
    settings: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct ContactItem {
    email: String,
    #[serde(default)]
    name: String,
}

/// Zapisuje kopię w katalogu Pobrane i zwraca ścieżkę pliku.
#[tauri::command]
pub fn backup_export(store: State<'_, Store>, settings: BTreeMap<String, String>) -> Result<String, String> {
    let contacts: Vec<Value> = store
        .contacts_list()?
        .into_iter()
        .take(MAX_ITEMS)
        .map(|(email, name)| json!({ "email": email, "name": name }))
        .collect();
    let flags: Vec<Value> = store.prio_list(MAX_ITEMS)?.into_iter().map(|(k, l)| json!([k, l])).collect();
    let rules: Vec<Value> = store
        .rules_list()?
        .into_iter()
        .map(|r| json!({ "field": r.field, "needle": r.needle, "action": r.action }))
        .collect();
    let events: Vec<Value> = store
        .event_all()?
        .into_iter()
        .take(crate::calendar::MAX_EVENTS as usize)
        .map(|e| json!({ "title": e.title, "notes": e.notes, "startMs": e.start_ms, "endMs": e.end_ms, "allDay": e.all_day, "remindMin": e.remind_min }))
        .collect();
    let doc = json!({
        "format": FORMAT,
        "version": VERSION,
        "settings": clean_settings(&settings),
        "contacts": contacts,
        "flags": flags,
        "rules": rules,
        "events": events,
    });
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
    let path = remote::save_download("rustmail-kopia.json", &bytes)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Wczytuje kopię: dopisuje adresy i flagi (niczego nie usuwa) i zwraca ustawienia do zastosowania w interfejsie.
#[tauri::command]
pub fn backup_import(store: State<'_, Store>, json: String) -> Result<ImportResult, String> {
    if json.len() > MAX_BYTES {
        return Err("Plik kopii jest zbyt duży.".to_string());
    }
    let v: Value = serde_json::from_str(&json).map_err(|_| "To nie jest plik kopii RustMail.".to_string())?;
    if v.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err("To nie jest plik kopii RustMail.".to_string());
    }
    if v.get("version").and_then(Value::as_u64).map(|n| n > VERSION).unwrap_or(true) {
        return Err("Kopia pochodzi z nowszej wersji programu.".to_string());
    }

    let mut addrs: Vec<(contacts::Addr, i64)> = Vec::new();
    if let Some(list) = v.get("contacts").and_then(Value::as_array) {
        for item in list.iter().take(MAX_ITEMS) {
            if let Ok(c) = serde_json::from_value::<ContactItem>(item.clone()) {
                let email = c.email.trim().to_lowercase();
                if contacts::valid_email(&email) {
                    addrs.push((contacts::Addr { email, name: c.name.trim().chars().take(120).collect() }, 2));
                }
            }
        }
    }
    let n_contacts = addrs.len();
    store.contacts_learn(&addrs)?;

    let mut n_flags = 0usize;
    if let Some(list) = v.get("flags").and_then(Value::as_array) {
        for item in list.iter().take(MAX_ITEMS) {
            if let (Some(k), Some(l)) = (item.get(0).and_then(Value::as_str), item.get(1).and_then(Value::as_u64)) {
                if (1..=3).contains(&l) && store.prio_set(k, l as u8).is_ok() {
                    n_flags += 1;
                }
            }
        }
    }

    // Reguły: dopisujemy tylko te, których jeszcze nie ma (bez duplikatów).
    let mut n_rules = 0usize;
    if let Some(list) = v.get("rules").and_then(Value::as_array) {
        let mut existing = store.rules_list()?;
        for item in list.iter().take(crate::rules::MAX_RULES) {
            let (Some(f), Some(n), Some(a)) = (
                item.get("field").and_then(Value::as_str),
                item.get("needle").and_then(Value::as_str),
                item.get("action").and_then(Value::as_str),
            ) else { continue };
            if let Ok(needle) = crate::rules::validate(f, n, a) {
                if !existing.iter().any(|r| r.field == f && r.needle == needle && r.action == a) && store.rule_add(f, &needle, a).is_ok() {
                    n_rules += 1;
                    existing.push(crate::rules::Rule { id: 0, field: f.to_string(), needle, action: a.to_string() });
                }
            }
        }
    }

    // Wydarzenia: dopisujemy tylko te, których jeszcze nie ma (ten sam tytuł i czas).
    let mut n_events = 0usize;
    if let Some(list) = v.get("events").and_then(Value::as_array) {
        let mut existing = store.event_all()?;
        for item in list.iter().take(crate::calendar::MAX_EVENTS as usize) {
            let Ok(mut e) = serde_json::from_value::<crate::calendar::Event>(item.clone()) else { continue };
            e.id = None;
            let Ok(e) = crate::calendar::validate(&e) else { continue };
            if existing.iter().any(|x| x.title == e.title && x.start_ms == e.start_ms && x.end_ms == e.end_ms) {
                continue;
            }
            if store.event_save(&e).is_ok() {
                n_events += 1;
                existing.push(e);
            }
        }
    }

    let mut raw: BTreeMap<String, String> = BTreeMap::new();
    if let Some(obj) = v.get("settings").and_then(Value::as_object) {
        for (k, val) in obj {
            if let Some(s) = val.as_str() {
                raw.insert(k.clone(), s.to_string());
            }
        }
    }
    Ok(ImportResult { contacts: n_contacts, flags: n_flags, rules: n_rules, events: n_events, settings: clean_settings(&raw) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_whitelisted_and_bounded() {
        let mut m = BTreeMap::new();
        m.insert("rm.theme".to_string(), "dark".to_string());
        m.insert("rm.trusted".to_string(), "x".to_string());
        m.insert("token".to_string(), "secret".to_string());
        m.insert("rm.keys".to_string(), "x".repeat(MAX_SETTING_LEN + 1));
        let c = clean_settings(&m);
        assert_eq!(c.len(), 1);
        assert_eq!(c.get("rm.theme").map(String::as_str), Some("dark"));
    }
}

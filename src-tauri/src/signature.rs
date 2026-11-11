//! Podpis wiadomości przechowywany lokalnie jako zwykły tekst w katalogu konfiguracji aplikacji.
//! Podpis nie jest sekretem, dlatego nie trafia do magazynu haseł.

use std::fs;
use std::path::Path;

pub const MAX_CHARS: usize = 2000;

pub fn normalize(text: &str) -> String {
    let t: String = text.replace("\r\n", "\n").chars().filter(|c| *c == '\n' || !c.is_control()).collect();
    t.trim_end().to_string()
}

pub const MAX_ACCOUNTS: usize = 30;

/// Klucz konta: „gmail” albo „imap:adres”.
pub fn valid_account(k: &str) -> bool {
    (k == "gmail" || k.starts_with("imap:")) && k.len() <= 200 && !k.chars().any(|c| c.is_control())
}

/// Podpisy poszczególnych kont (plik JSON); uszkodzony lub brakujący plik to brak podpisów własnych.
pub fn read_map(path: &Path) -> std::collections::BTreeMap<String, String> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<std::collections::BTreeMap<String, String>>(&s).ok())
        .map(|m| m.into_iter().filter(|(k, _)| valid_account(k)).map(|(k, v)| (k, normalize(&v))).collect())
        .unwrap_or_default()
}

/// Zapisuje podpis konta; `None` usuwa podpis własny (konto wraca do podpisu domyślnego).
pub fn write_entry(path: &Path, account: &str, text: Option<&str>) -> Result<(), String> {
    if !valid_account(account) {
        return Err("Niepoprawne konto.".to_string());
    }
    let mut map = read_map(path);
    match text {
        None => {
            map.remove(account);
        }
        Some(t) => {
            let t = normalize(t);
            if t.chars().count() > MAX_CHARS {
                return Err(format!("Podpis jest za długi (maksymalnie {MAX_CHARS} znaków)."));
            }
            if !map.contains_key(account) && map.len() >= MAX_ACCOUNTS {
                return Err("Za dużo podpisów.".to_string());
            }
            map.insert(account.to_string(), t);
        }
    }
    let json = serde_json::to_string(&map).map_err(|e| e.to_string())?;
    crate::privfs::write_private(path, json.as_bytes()).map_err(|e| format!("Nie można zapisać podpisu: {e}"))
}

pub fn read_from(path: &Path) -> String {
    fs::read_to_string(path).map(|s| normalize(&s)).unwrap_or_default()
}

pub fn write_to(path: &Path, text: &str) -> Result<(), String> {
    let t = normalize(text);
    if t.chars().count() > MAX_CHARS {
        return Err(format!("Podpis jest za długi (maksymalnie {MAX_CHARS} znaków)."));
    }
    crate::privfs::write_private(path, t.as_bytes()).map_err(|e| format!("Nie można zapisać podpisu: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_normalize() {
        let dir = std::env::temp_dir().join(format!("rustmail-sig-test-{}", std::process::id()));
        let file = dir.join("sub").join("signature.txt");
        assert_eq!(read_from(&file), "");
        write_to(&file, "Pozdrawiam\r\nJan\u{0007}\n\n").unwrap();
        assert_eq!(read_from(&file), "Pozdrawiam\nJan");
        assert!(write_to(&file, &"x".repeat(MAX_CHARS + 1)).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn per_account_signatures() {
        let dir = std::env::temp_dir().join(format!("rustmail-sigmap-test-{}", std::process::id()));
        let file = dir.join("signatures.json");
        assert!(read_map(&file).is_empty());
        write_entry(&file, "gmail", Some("Jan\r\nGmail")).unwrap();
        write_entry(&file, "imap:a@wp.pl", Some("")).unwrap();
        let m = read_map(&file);
        assert_eq!(m.get("gmail").map(String::as_str), Some("Jan\nGmail"));
        assert_eq!(m.get("imap:a@wp.pl").map(String::as_str), Some(""));
        write_entry(&file, "gmail", None).unwrap();
        assert!(!read_map(&file).contains_key("gmail"));
        assert!(write_entry(&file, "ktoś", Some("x")).is_err());
        assert!(write_entry(&file, "gmail", Some(&"x".repeat(MAX_CHARS + 1))).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}

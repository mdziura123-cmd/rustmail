//! Lista wątków, które użytkownik uznał za bezpieczne mimo ostrzeżenia (fałszywy alarm).
//! Zapisywana lokalnie jako zwykły plik tekstowy (jeden identyfikator wątku w linii).
//! Identyfikatory wątków nie są sekretem; plik nie zawiera treści poczty.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

const MAX_IDS: usize = 5000;

fn valid(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

pub fn load(path: &Path) -> HashSet<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| valid(l))
        .take(MAX_IDS)
        .collect()
}

fn save(path: &Path, set: &HashSet<String>) -> Result<(), String> {
    let mut ids: Vec<&String> = set.iter().collect();
    ids.sort();
    let text = ids.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n");
    crate::privfs::write_private(path, text.as_bytes()).map_err(|e| format!("Nie można zapisać listy: {e}"))
}

pub fn add(path: &Path, id: &str) -> Result<(), String> {
    if !valid(id) {
        return Err("Niepoprawny identyfikator wątku".to_string());
    }
    let mut set = load(path);
    if set.len() >= MAX_IDS {
        return Err("Lista bezpiecznych wątków jest pełna.".to_string());
    }
    set.insert(id.to_string());
    save(path, &set)
}

pub fn remove(path: &Path, id: &str) -> Result<(), String> {
    let mut set = load(path);
    set.remove(id);
    save(path, &set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_remove_roundtrip_and_validation() {
        let dir = std::env::temp_dir().join(format!("rustmail-dis-test-{}", std::process::id()));
        let f = dir.join("sub").join("dismissed.txt");
        assert!(load(&f).is_empty());
        add(&f, "abc123").unwrap();
        add(&f, "def456").unwrap();
        add(&f, "abc123").unwrap();
        assert_eq!(load(&f).len(), 2);
        assert!(add(&f, "../etc").is_err());
        assert!(add(&f, "").is_err());
        remove(&f, "abc123").unwrap();
        let s = load(&f);
        assert!(s.contains("def456") && !s.contains("abc123"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn garbage_lines_are_ignored() {
        let dir = std::env::temp_dir().join(format!("rustmail-dis-test2-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("d.txt");
        fs::write(&f, "ok1\n../../x\n\n  ok2  \nzły id\n").unwrap();
        let s = load(&f);
        assert_eq!(s.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }
}

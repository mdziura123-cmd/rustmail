//! Lokalne listy nadawców: zablokowani i zaufani. Zapis jako zwykłe pliki tekstowe (jeden wpis w linii).
//! Wpis to adres e-mail („oszust@zla.pl”) albo domena („zla.pl” – obejmuje też poddomeny).
//! Gmail API nie pozwala blokować nadawców bez dodatkowego, szerszego uprawnienia, dlatego lista działa w RustMail:
//! oznacza wiadomości i podnosi ocenę ryzyka. Niczego nie usuwa automatycznie.

use std::fs;
use std::path::Path;

const MAX_ENTRIES: usize = 2000;

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    pub blocked: Vec<String>,
    pub allowed: Vec<String>,
}

fn matches(entry: &str, email: &str) -> bool {
    let email = email.trim().to_ascii_lowercase();
    if email.is_empty() {
        return false;
    }
    if entry.contains('@') {
        return entry == email;
    }
    let domain = email.rsplit('@').next().unwrap_or("");
    domain == entry || domain.ends_with(&format!(".{entry}"))
}

impl Rules {
    pub fn is_blocked(&self, email: &str) -> bool {
        self.blocked.iter().any(|e| matches(e, email))
    }
    pub fn is_allowed(&self, email: &str) -> bool {
        self.allowed.iter().any(|e| matches(e, email))
    }
}

/// Sprawdza i normalizuje wpis: poprawny adres e-mail albo domena.
pub fn normalize_entry(s: &str) -> Result<String, String> {
    let e = s.trim().to_ascii_lowercase();
    let e = e.trim_start_matches("*@").trim_start_matches('@').to_string();
    let label_ok = |l: &str| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let domain_ok = |d: &str| d.len() <= 253 && d.contains('.') && d.split('.').all(label_ok);
    if let Some((local, domain)) = e.split_once('@') {
        let local_ok = !local.is_empty() && local.len() <= 64 && local.chars().all(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c));
        if local_ok && domain_ok(domain) {
            return Ok(e);
        }
    } else if domain_ok(&e) {
        return Ok(e);
    }
    Err("Podaj adres e-mail (np. ktos@firma.pl) albo domenę (np. firma.pl).".to_string())
}

pub fn load_list(path: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| normalize_entry(l).ok())
        .take(MAX_ENTRIES)
        .collect();
    v.sort();
    v.dedup();
    v
}

fn save_list(path: &Path, list: &[String]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Nie można utworzyć katalogu konfiguracji: {e}"))?;
    }
    fs::write(path, list.join("\n")).map_err(|e| format!("Nie można zapisać listy: {e}"))
}

pub fn add(path: &Path, entry: &str) -> Result<String, String> {
    let e = normalize_entry(entry)?;
    let mut list = load_list(path);
    if list.len() >= MAX_ENTRIES {
        return Err("Lista jest pełna.".to_string());
    }
    if !list.contains(&e) {
        list.push(e.clone());
        list.sort();
    }
    save_list(path, &list)?;
    Ok(e)
}

pub fn remove(path: &Path, entry: &str) -> Result<(), String> {
    let e = entry.trim().to_ascii_lowercase();
    let mut list = load_list(path);
    list.retain(|x| *x != e);
    save_list(path, &list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_validated_and_normalized() {
        assert_eq!(normalize_entry(" Oszust@Zla.PL ").unwrap(), "oszust@zla.pl");
        assert_eq!(normalize_entry("zla.pl").unwrap(), "zla.pl");
        assert_eq!(normalize_entry("@zla.pl").unwrap(), "zla.pl");
        assert_eq!(normalize_entry("*@zla.pl").unwrap(), "zla.pl");
        for bad in ["", "zla", "a b@x.pl", "a@x", "a@@x.pl", "../x.pl", "a@-x.pl", "x.pl/../etc", "a\nb@x.pl"] {
            assert!(normalize_entry(bad).is_err(), "powinno być odrzucone: {bad:?}");
        }
    }

    #[test]
    fn matching_by_address_and_domain() {
        let r = Rules { blocked: vec!["zla.pl".into(), "jan@ok.pl".into()], allowed: vec!["bank.pl".into()] };
        assert!(r.is_blocked("x@zla.pl"));
        assert!(r.is_blocked("X@Mail.Zla.PL"));
        assert!(!r.is_blocked("x@nie-zla.pl"));
        assert!(!r.is_blocked("x@zla.pl.example.com"));
        assert!(r.is_blocked("jan@ok.pl"));
        assert!(!r.is_blocked("anna@ok.pl"));
        assert!(r.is_allowed("a@bank.pl") && r.is_allowed("a@info.bank.pl"));
        assert!(!r.is_allowed("a@fakebank.pl"));
        assert!(!r.is_blocked(""));
    }

    #[test]
    fn add_remove_persist() {
        let dir = std::env::temp_dir().join(format!("rustmail-bl-test-{}", std::process::id()));
        let f = dir.join("x").join("blocked.txt");
        assert!(load_list(&f).is_empty());
        assert_eq!(add(&f, "B@X.pl").unwrap(), "b@x.pl");
        add(&f, "a.pl").unwrap();
        add(&f, "a.pl").unwrap();
        assert_eq!(load_list(&f), vec!["a.pl".to_string(), "b@x.pl".to_string()]);
        assert!(add(&f, "zle").is_err());
        remove(&f, "A.pl").unwrap();
        assert_eq!(load_list(&f), vec!["b@x.pl".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }
}

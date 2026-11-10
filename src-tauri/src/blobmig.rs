//! Przechowywanie małych danych tekstowych (dziennik spamu, model Bayesa, lista „widzianych” wątków)
//! w zaszyfrowanej bazie z bezpieczną migracją ze starych, jawnych plików.
//!
//! Zasady (żeby niczego nie zgubić):
//! * stary plik jest kasowany dopiero po zapisie w bazie i odczycie zwrotnym zgodnym z oryginałem;
//! * jeśli stary plik istnieje, jest traktowany jako najświeższy (powstaje przy migracji albo gdy baza
//!   była chwilowo niedostępna przy zapisie) i trafia do bazy;
//! * gdy bazy nie ma lub zawodzi, dane trafiają do prywatnego pliku (0600), jak dotychczas.

use std::path::Path;

use crate::privfs;

pub trait BlobDb {
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    fn delete(&self, key: &str) -> Result<(), String>;
}

fn read_legacy(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Próbuje przenieść tekst do bazy. Zwraca true tylko, gdy odczyt zwrotny jest identyczny.
fn store_verified(db: &dyn BlobDb, key: &str, text: &str) -> bool {
    db.set(key, text).is_ok() && matches!(db.get(key), Ok(Some(ref v)) if v == text)
}

fn remove_legacy(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

/// Wczytuje dane; przy okazji migruje stary plik do bazy.
pub fn load(db: Option<&dyn BlobDb>, key: &str, legacy: &Path) -> Option<String> {
    if let Some(text) = read_legacy(legacy) {
        if let Some(db) = db {
            if store_verified(db, key, &text) {
                remove_legacy(legacy);
            }
        }
        return Some(text);
    }
    match db {
        Some(db) => db.get(key).ok().flatten(),
        None => None,
    }
}

/// Zapisuje dane: do bazy, a gdy to się nie uda – do prywatnego pliku.
pub fn save(db: Option<&dyn BlobDb>, key: &str, legacy: &Path, text: &str) -> Result<(), String> {
    if let Some(db) = db {
        if db.set(key, text).is_ok() {
            remove_legacy(legacy);
            return Ok(());
        }
    }
    privfs::write_private(legacy, text.as_bytes())
}

/// Usuwa dane z bazy i ze starego pliku. Błąd tylko wtedy, gdy któraś kopia nadal istnieje.
pub fn wipe(db: Option<&dyn BlobDb>, key: &str, legacy: &Path) -> Result<(), String> {
    let mut failed = false;
    if let Some(db) = db {
        if db.delete(key).is_err() {
            failed = true;
        }
    }
    match std::fs::remove_file(legacy) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => failed = true,
    }
    if failed {
        Err("Nie udało się usunąć wszystkich danych.".to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::HashMap, path::PathBuf};

    #[derive(Default)]
    struct Mem {
        map: RefCell<HashMap<String, String>>,
        fail_set: bool,
        corrupt_set: bool,
    }
    impl BlobDb for Mem {
        fn get(&self, k: &str) -> Result<Option<String>, String> {
            Ok(self.map.borrow().get(k).cloned())
        }
        fn set(&self, k: &str, v: &str) -> Result<(), String> {
            if self.fail_set {
                return Err("błąd".into());
            }
            let v = if self.corrupt_set { format!("{v}X") } else { v.to_string() };
            self.map.borrow_mut().insert(k.into(), v);
            Ok(())
        }
        fn delete(&self, k: &str) -> Result<(), String> {
            self.map.borrow_mut().remove(k);
            Ok(())
        }
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rm-blob-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn migrates_legacy_file_and_removes_it_after_verification() {
        let d = dir("m1");
        let f = d.join("spam_log.json");
        std::fs::write(&f, "[{\"subject\":\"tajne\"}]").unwrap();
        let db = Mem::default();
        let got = load(Some(&db), "spam:log", &f);
        assert_eq!(got.as_deref(), Some("[{\"subject\":\"tajne\"}]"));
        assert!(!f.exists(), "jawna kopia musi zniknąć");
        assert_eq!(db.get("spam:log").unwrap().as_deref(), Some("[{\"subject\":\"tajne\"}]"));
        // kolejne wczytanie czyta już z bazy
        assert_eq!(load(Some(&db), "spam:log", &f).as_deref(), Some("[{\"subject\":\"tajne\"}]"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn failed_migration_keeps_legacy_file_and_data() {
        let d = dir("m2");
        let f = d.join("model.txt");
        std::fs::write(&f, "dane").unwrap();
        let db = Mem { fail_set: true, ..Default::default() };
        assert_eq!(load(Some(&db), "k", &f).as_deref(), Some("dane"));
        assert!(f.exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn mismatching_readback_keeps_legacy_file() {
        let d = dir("m3");
        let f = d.join("model.txt");
        std::fs::write(&f, "dane").unwrap();
        let db = Mem { corrupt_set: true, ..Default::default() };
        assert_eq!(load(Some(&db), "k", &f).as_deref(), Some("dane"));
        assert!(f.exists(), "bez zgodnego odczytu nie wolno kasować starego pliku");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn works_without_database_using_private_file() {
        use std::os::unix::fs::PermissionsExt;
        let d = dir("m4");
        let f = d.join("seen.txt");
        assert_eq!(load(None, "k", &f), None);
        save(None, "k", &f, "a\nb").unwrap();
        assert_eq!(load(None, "k", &f).as_deref(), Some("a\nb"));
        assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn save_goes_to_db_and_falls_back_to_file_on_error() {
        let d = dir("m5");
        let f = d.join("x.txt");
        let ok = Mem::default();
        save(Some(&ok), "k", &f, "v1").unwrap();
        assert!(!f.exists());
        assert_eq!(ok.get("k").unwrap().as_deref(), Some("v1"));
        let bad = Mem { fail_set: true, ..Default::default() };
        save(Some(&bad), "k", &f, "v2").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "v2");
        // nowszy plik wygrywa z bazą i po odzyskaniu bazy zostaje do niej przeniesiony
        assert_eq!(load(Some(&ok), "k", &f).as_deref(), Some("v2"));
        assert!(!f.exists());
        assert_eq!(ok.get("k").unwrap().as_deref(), Some("v2"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn wipe_removes_both_copies() {
        let d = dir("m6");
        let f = d.join("x.txt");
        std::fs::write(&f, "a").unwrap();
        let db = Mem::default();
        db.set("k", "b").unwrap();
        wipe(Some(&db), "k", &f).unwrap();
        assert!(!f.exists());
        assert_eq!(db.get("k").unwrap(), None);
        wipe(Some(&db), "k", &f).unwrap(); // idempotentnie
        std::fs::remove_dir_all(&d).unwrap();
    }
}

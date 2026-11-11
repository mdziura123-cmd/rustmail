//! Kopia bezpieczeństwa lokalnej bazy: przy pierwszym uruchomieniu każdej nowej wersji programu, zanim ta wersja
//! otworzy bazę, plik bazy (zaszyfrowany tak samo jak oryginał) jest kopiowany obok jako `*.bak`.
//! Dzięki temu po nieudanej aktualizacji dane sprzed niej da się odzyskać. Trzymana jest jedna kopia.

use std::fs;
use std::path::Path;

const MARKER: &str = "cache.bak.version";

pub fn needs_snapshot(marker: Option<&str>, version: &str) -> bool {
    marker.map(str::trim) != Some(version)
}

/// Zwraca `Ok(true)`, gdy zrobiono kopię. Brak pliku bazy lub kopia dla tej wersji to `Ok(false)`.
pub fn snapshot(dir: &Path, db_file: &str, version: &str) -> Result<bool, String> {
    let src = dir.join(db_file);
    if !fs::metadata(&src).map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(false);
    }
    let marker = dir.join(MARKER);
    let current = fs::read_to_string(&marker).ok();
    if !needs_snapshot(current.as_deref(), version) {
        return Ok(false);
    }
    for ext in ["", "-wal"] {
        let from = dir.join(format!("{db_file}{ext}"));
        let to = dir.join(format!("{db_file}.bak{ext}"));
        if from.exists() {
            fs::copy(&from, &to).map_err(|e| format!("Nie można zrobić kopii bazy: {e}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&to, fs::Permissions::from_mode(0o600));
            }
        } else {
            let _ = fs::remove_file(&to);
        }
    }
    fs::write(&marker, version).map_err(|e| format!("Nie można zapisać znacznika kopii: {e}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_once_per_version() {
        let dir = std::env::temp_dir().join(format!("rustmail-dbsafe-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // bez bazy nic nie robimy
        assert_eq!(snapshot(&dir, "cache.db", "1.0.0"), Ok(false));
        fs::write(dir.join("cache.db"), b"dane-v1").unwrap();
        fs::write(dir.join("cache.db-wal"), b"wal").unwrap();
        assert_eq!(snapshot(&dir, "cache.db", "1.0.0"), Ok(true));
        assert_eq!(fs::read(dir.join("cache.db.bak")).unwrap(), b"dane-v1");
        assert_eq!(fs::read(dir.join("cache.db.bak-wal")).unwrap(), b"wal");
        // ta sama wersja – bez nowej kopii
        fs::write(dir.join("cache.db"), b"dane-v1-zmienione").unwrap();
        assert_eq!(snapshot(&dir, "cache.db", "1.0.0"), Ok(false));
        assert_eq!(fs::read(dir.join("cache.db.bak")).unwrap(), b"dane-v1");
        // nowa wersja – nowa kopia
        assert_eq!(snapshot(&dir, "cache.db", "1.0.1"), Ok(true));
        assert_eq!(fs::read(dir.join("cache.db.bak")).unwrap(), b"dane-v1-zmienione");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn marker_logic() {
        assert!(needs_snapshot(None, "1"));
        assert!(needs_snapshot(Some("0"), "1"));
        assert!(!needs_snapshot(Some("1\n"), "1"));
    }
}

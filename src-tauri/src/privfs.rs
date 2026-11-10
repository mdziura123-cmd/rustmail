//! Prywatne zapisy plików: plik od pierwszej chwili ma prawa 0600 (nie ma okna, w którym byłby czytelny
//! dla innych użytkowników), a katalogi aplikacji mają 0700. Zapis idzie przez plik tymczasowy i `rename`.

use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

/// Tworzy katalog (razem z brakującymi nadrzędnymi) z prawami 0700 i wymusza 0700 na ostatnim.
/// Wywołuj tylko dla katalogów należących do aplikacji.
pub fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".tmp");
    PathBuf::from(s)
}

/// Zapisuje plik z prawami 0600, zastępując poprzednią wersję atomowo.
pub fn write_private(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(p) = path.parent() {
        if !p.as_os_str().is_empty() {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(p)
                .map_err(|e| format!("Nie można utworzyć katalogu: {e}"))?;
        }
    }
    let tmp = tmp_path(path);
    let _ = fs::remove_file(&tmp);
    let res = (|| -> std::io::Result<()> {
        let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()
    })();
    if let Err(e) = res {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Nie można zapisać pliku: {e}"));
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Nie można zapisać pliku: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rm-privfs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn mode(p: &Path) -> u32 {
        fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn files_are_private_from_creation_and_dirs_are_0700() {
        let d = tmpdir("a");
        let f = d.join("sub").join("data.json");
        write_private(&f, b"tajne").unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"tajne");
        assert_eq!(mode(&f), 0o600);
        assert_eq!(mode(&d.join("sub")), 0o700);
        assert!(!tmp_path(&f).exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn overwrite_keeps_private_mode_even_if_old_file_was_open_to_all() {
        let d = tmpdir("b");
        ensure_private_dir(&d).unwrap();
        let f = d.join("x.txt");
        fs::write(&f, "stare").unwrap();
        fs::set_permissions(&f, fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&f, b"nowe").unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"nowe");
        assert_eq!(mode(&f), 0o600);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn ensure_private_dir_tightens_existing_directory() {
        let d = tmpdir("c");
        fs::create_dir_all(&d).unwrap();
        fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
        ensure_private_dir(&d).unwrap();
        assert_eq!(mode(&d), 0o700);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn stale_tmp_file_does_not_block_writing() {
        let d = tmpdir("d");
        ensure_private_dir(&d).unwrap();
        let f = d.join("y.txt");
        fs::write(tmp_path(&f), "resztki").unwrap();
        write_private(&f, b"ok").unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"ok");
        fs::remove_dir_all(&d).unwrap();
    }
}

//! Skanowanie plików lokalnym ClamAV (jeśli jest zainstalowany). Aplikacja niczego nie wysyła do internetu.
//! Brak ClamAV nie blokuje pobierania, ale wynik jest wtedy jawnie oznaczony jako „nie sprawdzono”.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ScanStatus {
    Clean,
    Infected(String),
    Unavailable,
    Failed(String),
}

/// Szuka programu w katalogach z PATH oraz w typowych lokalizacjach. `exists` pozwala testować bez dysku.
pub fn find_in(path_var: &OsStr, name: &str, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(path_var).collect();
    for extra in ["/usr/bin", "/usr/local/bin", "/bin", "/usr/sbin"] {
        dirs.push(PathBuf::from(extra));
    }
    dirs.into_iter().map(|d| d.join(name)).find(|p| exists(p))
}

/// Interpretuje wynik clamscan/clamdscan: kod 0 = czysto, 1 = wykryto, inne = błąd.
pub fn parse_clam(code: Option<i32>, stdout: &str, stderr: &str) -> ScanStatus {
    match code {
        Some(0) => ScanStatus::Clean,
        Some(1) => {
            let sig = stdout
                .lines()
                .find_map(|l| {
                    let l = l.trim();
                    let body = l.strip_suffix(" FOUND")?;
                    Some(body.rsplit(": ").next().unwrap_or(body).trim().to_string())
                })
                .unwrap_or_else(|| "nieznane zagrożenie".to_string());
            ScanStatus::Infected(sig)
        }
        _ => {
            let msg = stderr.lines().chain(stdout.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("nieznany błąd skanera");
            ScanStatus::Failed(msg.chars().take(200).collect())
        }
    }
}

fn run_one(bin: &Path, daemon: bool, file: &Path, timeout: Duration) -> ScanStatus {
    let mut cmd = Command::new(bin);
    cmd.arg("--no-summary");
    if daemon {
        // demon clamd działa jako inny użytkownik – przekazujemy mu otwarty plik zamiast ścieżki
        cmd.arg("--fdpass");
    }
    cmd.arg(file).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // Zmienne środowiska AppImage mogłyby zepsuć uruchamianie programów systemowych.
    for k in ["LD_LIBRARY_PATH", "LD_PRELOAD", "APPDIR", "GDK_PIXBUF_MODULE_FILE"] {
        cmd.env_remove(k);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return ScanStatus::Failed(format!("nie można uruchomić skanera: {e}")),
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return ScanStatus::Failed("skanowanie przekroczyło limit czasu".to_string());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return ScanStatus::Failed(e.to_string()),
        }
    }
    match child.wait_with_output() {
        Ok(out) => parse_clam(
            out.status.code(),
            &String::from_utf8_lossy(&out.stdout),
            &String::from_utf8_lossy(&out.stderr),
        ),
        Err(e) => ScanStatus::Failed(e.to_string()),
    }
}

/// Zapisuje plik tymczasowy tylko do odczytu przez właściciela (0600); nigdy nie nadpisuje istniejącego.
pub fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    f.write_all(data)
}

/// Skanuje plik (wywołanie blokujące – uruchamiaj w `spawn_blocking`). Najpierw demon (szybki), potem clamscan.
pub fn scan_file(file: &Path) -> ScanStatus {
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let candidates = [("clamdscan", true), ("clamscan", false)];
    let mut found_any = false;
    let mut last = ScanStatus::Unavailable;
    for (name, daemon) in candidates {
        let Some(bin) = find_in(&path_var, name, |p| p.is_file()) else { continue };
        found_any = true;
        let st = run_one(&bin, daemon, file, Duration::from_secs(120));
        match st {
            ScanStatus::Failed(_) => last = st, // spróbuj kolejnego skanera
            other => return other,
        }
    }
    if found_any { last } else { ScanStatus::Unavailable }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_infected_and_errors() {
        assert_eq!(parse_clam(Some(0), "/tmp/x: OK\n", ""), ScanStatus::Clean);
        assert_eq!(
            parse_clam(Some(1), "/tmp/x.bin: Win.Test.EICAR_HDB-1 FOUND\n", ""),
            ScanStatus::Infected("Win.Test.EICAR_HDB-1".to_string())
        );
        assert_eq!(parse_clam(Some(1), "", ""), ScanStatus::Infected("nieznane zagrożenie".to_string()));
        assert_eq!(
            parse_clam(Some(2), "", "ERROR: Can't connect to clamd\n"),
            ScanStatus::Failed("ERROR: Can't connect to clamd".to_string())
        );
        assert!(matches!(parse_clam(None, "", ""), ScanStatus::Failed(_)));
        let long = "x".repeat(500);
        if let ScanStatus::Failed(m) = parse_clam(Some(2), "", &long) {
            assert_eq!(m.len(), 200);
        } else {
            panic!("oczekiwano Failed");
        }
    }

    #[test]
    fn finds_scanner_in_path_and_defaults() {
        let p = std::ffi::OsString::from("/opt/a:/opt/b");
        let found = find_in(&p, "clamscan", |x| x == Path::new("/opt/b/clamscan"));
        assert_eq!(found, Some(PathBuf::from("/opt/b/clamscan")));
        let usr = find_in(&p, "clamscan", |x| x == Path::new("/usr/bin/clamscan"));
        assert_eq!(usr, Some(PathBuf::from("/usr/bin/clamscan")));
        assert_eq!(find_in(&p, "clamscan", |_| false), None);
    }

    #[test]
    fn missing_scanner_is_reported_as_unavailable() {
        // w środowisku testowym zwykle nie ma ClamAV; jeśli jest, wynik nie może być „Unavailable”
        let dir = std::env::temp_dir().join(format!("rustmail-scan-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.bin");
        std::fs::write(&f, b"hello").unwrap();
        let r = scan_file(&f);
        let have = find_in(&std::env::var_os("PATH").unwrap_or_default(), "clamscan", |p| p.is_file()).is_some()
            || find_in(&std::env::var_os("PATH").unwrap_or_default(), "clamdscan", |p| p.is_file()).is_some();
        assert_eq!(r == ScanStatus::Unavailable, !have);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! Skrót aplikacji (plik .desktop) wskazujący na ten sam plik AppImage, z którego działa program.
//! Dzięki stałej nazwie skrótu (pl.rustmail.app.desktop) przypięcie do paska zadań zostaje po aktualizacji:
//! aktualizator podmienia plik w miejscu, a przy każdym starcie ścieżka w skrócie jest odświeżana.
//! Ta część jest czysta (bez zależności od Tauri), więc jest testowana osobno.

pub const DESKTOP_ID: &str = "pl.rustmail.app";

/// Ścieżka trafia do pola `Exec` w cudzysłowie. Nie próbujemy ucieczek – ścieżki ze znakami, które
/// wymagałyby podwójnego ucieczkowania (`"`, `` ` ``, `$`, `\`, `%`) lub ze znakami sterującymi, są odrzucane.
pub fn safe_exec_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.is_empty()
        && !path.chars().any(|c| c.is_control() || matches!(c, '"' | '`' | '$' | '\\' | '%'))
}

/// Treść pliku .desktop dla podanego pliku AppImage; `None`, gdy ścieżki nie da się bezpiecznie zapisać.
pub fn desktop_entry(appimage: &str) -> Option<String> {
    if !safe_exec_path(appimage) {
        return None;
    }
    Some(format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name=RustMail\n\
         GenericName=Klient poczty\n\
         Comment=Bezpieczny klient poczty Gmail i WP\n\
         Exec=\"{appimage}\"\n\
         Icon={DESKTOP_ID}\n\
         Terminal=false\n\
         Categories=Network;Email;\n\
         StartupWMClass=rustmail\n\
         X-AppImage-Integrate=false\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_entry_with_quoted_exec() {
        let e = desktop_entry("/home/ola/Aplikacje/RustMail x86_64.AppImage").unwrap();
        assert!(e.starts_with("[Desktop Entry]\n"));
        assert!(e.contains("Exec=\"/home/ola/Aplikacje/RustMail x86_64.AppImage\"\n"));
        assert!(e.contains("Icon=pl.rustmail.app\n"));
        assert!(e.ends_with('\n'));
        // każda linia to klucz=wartość albo nagłówek
        for l in e.lines() {
            assert!(l.starts_with('[') || l.contains('='), "zła linia: {l}");
        }
    }

    #[test]
    fn rejects_dangerous_paths() {
        for p in ["relative/RustMail.AppImage", "", "/a\nExec=/bin/sh", "/a\"b", "/a$b", "/a`b", "/a\\b", "/a%b", "/a\tb"] {
            assert!(desktop_entry(p).is_none(), "powinno odrzucić {p:?}");
        }
    }
}

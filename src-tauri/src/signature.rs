//! Podpis wiadomości przechowywany lokalnie jako zwykły tekst w katalogu konfiguracji aplikacji.
//! Podpis nie jest sekretem, dlatego nie trafia do magazynu haseł.

use std::fs;
use std::path::Path;

pub const MAX_CHARS: usize = 2000;

pub fn normalize(text: &str) -> String {
    let t: String = text.replace("\r\n", "\n").chars().filter(|c| *c == '\n' || !c.is_control()).collect();
    t.trim_end().to_string()
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
}

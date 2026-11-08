//! Czyste pomocnicze funkcje do szyfrowania lokalnej bazy (bez bazy i bez sieci), testowane osobno.

const PLAIN_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// Czy plik to zwykła (niezaszyfrowana) baza SQLite – po nagłówku. Zaszyfrowana baza ma losowy początek.
pub fn is_plain_sqlite(first_bytes: &[u8]) -> bool {
    first_bytes.len() >= 16 && &first_bytes[..16] == PLAIN_HEADER
}

/// Klucz bazy: tylko znaki base64url, 32–128 znaków. Dzięki temu można go bezpiecznie wstawić do polecenia PRAGMA.
pub fn valid_key(key: &str) -> bool {
    (32..=128).contains(&key.len()) && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Literał tekstowy SQL w apostrofach (apostrofy podwojone, bez znaków NUL).
pub fn sql_literal(s: &str) -> String {
    format!("'{}'", s.replace('\0', "").replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_plain_sqlite_header() {
        assert!(is_plain_sqlite(b"SQLite format 3\0rest of file"));
        assert!(!is_plain_sqlite(b"SQLite format 3"));
        assert!(!is_plain_sqlite(&[0x8a, 0x13, 0x77, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d]));
        assert!(!is_plain_sqlite(b""));
    }

    #[test]
    fn key_validation() {
        assert!(valid_key(&"a".repeat(43)));
        assert!(valid_key("abcDEF123-_abcDEF123-_abcDEF123-_ab"));
        assert!(!valid_key("short"));
        assert!(!valid_key(&format!("{}'; DROP TABLE x;--", "a".repeat(40))));
        assert!(!valid_key(&"a".repeat(200)));
    }

    #[test]
    fn literal_escaping() {
        assert_eq!(sql_literal("/home/o'brien/db"), "'/home/o''brien/db'");
        assert_eq!(sql_literal("a\0b"), "'ab'");
    }
}

//! Flagi ważności wiadomości (kolorowe: bardzo ważne, ważne, mniej ważne). Zapisywane lokalnie
//! w zaszyfrowanej bazie; serwer poczty ich nie widzi. Klucz identyfikuje wiadomość i konto, np.
//! `g:konto@gmail.com:18c3f…` (wątek Gmaila) albo `i:konto@wp.pl:INBOX:1234` (wiadomość IMAP).

pub const MAX_KEYS: usize = 300;

/// Klucz: drukowalny ASCII albo litery, bez znaków sterujących, rozsądnej długości.
pub fn valid_key(k: &str) -> bool {
    !k.is_empty() && k.len() <= 400 && !k.chars().any(|c| c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_validation() {
        assert!(valid_key("g:a@b.pl:abc123"));
        assert!(valid_key("i:a@wp.pl:Wysłane:12"));
        assert!(!valid_key(""));
        assert!(!valid_key("a\nb"));
        assert!(!valid_key(&"x".repeat(401)));
    }
}

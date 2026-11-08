//! Czysta walidacja własnych etykiet Gmaila (bez sieci), aby nie dało się przez nie dotknąć etykiet systemowych.

/// Identyfikator etykiety użytkownika. Nowe mają postać `Label_123`, ale starsze etykiety w Gmailu mogą mieć
/// dowolny identyfikator (np. równy nazwie). Wpuszczamy więc bezpieczne znaki, ale nigdy etykiet systemowych.
pub fn check_user_label_id(id: &str) -> Result<(), String> {
    const SYSTEM: &[&str] = &[
        "INBOX", "SENT", "DRAFT", "SPAM", "TRASH", "UNREAD", "STARRED", "IMPORTANT", "CHAT", "ALL", "OUTBOX", "SNOOZED", "SCHEDULED",
    ];
    let upper = id.to_uppercase();
    let ok = !id.is_empty()
        && id.chars().count() <= 128
        && !SYSTEM.contains(&upper.as_str())
        && !upper.starts_with("CATEGORY_")
        && id.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' ' | '/' | '.'));
    if ok {
        Ok(())
    } else {
        Err("Niepoprawny identyfikator etykiety.".to_string())
    }
}

/// Kodowanie procentowe identyfikatora do użycia w ścieżce URL.
pub fn path_encode(id: &str) -> String {
    let mut o = String::new();
    for b in id.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

/// Nazwa nowej etykiety: bez znaków sterujących, 1–100 znaków, bez nazw zarezerwowanych przez Gmail.
pub fn clean_label_name(name: &str) -> Result<String, String> {
    let s: String = name.chars().filter(|c| !c.is_control()).collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        return Err("Podaj nazwę etykiety.".to_string());
    }
    if s.chars().count() > 100 {
        return Err("Nazwa etykiety jest za długa (maks. 100 znaków).".to_string());
    }
    const RESERVED: &[&str] = &[
        "inbox", "sent", "draft", "drafts", "spam", "trash", "unread", "starred", "important", "chat", "all mail",
        "category_personal", "category_social", "category_promotions", "category_updates", "category_forums",
    ];
    if RESERVED.contains(&s.to_lowercase().as_str()) {
        return Err("Ta nazwa jest zarezerwowana przez Gmail.".to_string());
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_label_ids() {
        for ok in ["Label_12", "Label_1234567890", "faktury", "Praca/Klienci", "Łódź 1"] {
            assert!(check_user_label_id(ok).is_ok(), "{ok}");
        }
        for bad in ["INBOX", "trash", "CATEGORY_PROMOTIONS", "", "Label_1?x=2", "a%2Fb", "x\ny", "..\\x"] {
            assert!(check_user_label_id(bad).is_err(), "{bad}");
        }
        assert!(check_user_label_id(&"1".repeat(200)).is_err());
    }

    #[test]
    fn path_encoding() {
        assert_eq!(path_encode("Label_1"), "Label_1");
        assert_eq!(path_encode("a b/ł"), "a%20b%2F%C5%82");
    }

    #[test]
    fn label_names() {
        assert_eq!(clean_label_name("  Praca/Klienci \n").unwrap(), "Praca/Klienci");
        assert!(clean_label_name("   ").is_err());
        assert!(clean_label_name("Inbox").is_err());
        assert!(clean_label_name("SPAM").is_err());
        assert!(clean_label_name(&"x".repeat(101)).is_err());
        assert_eq!(clean_label_name("a\u{0}b").unwrap(), "ab");
    }
}

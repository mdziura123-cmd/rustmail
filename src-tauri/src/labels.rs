//! Czysta walidacja własnych etykiet Gmaila (bez sieci), aby nie dało się przez nie dotknąć etykiet systemowych.

/// Identyfikator etykiety użytkownika w Gmailu ma postać `Label_123`. Etykiet systemowych (INBOX, TRASH…) tu nie wpuszczamy.
pub fn check_user_label_id(id: &str) -> Result<(), String> {
    let ok = id.len() > 6
        && id.len() <= 64
        && id.starts_with("Label_")
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err("Niepoprawny identyfikator etykiety.".to_string())
    }
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
        assert!(check_user_label_id("Label_12").is_ok());
        assert!(check_user_label_id("Label_1234567890").is_ok());
        for bad in ["INBOX", "TRASH", "Label_", "Label_a/b", "label_1", "", "Label_1?x=2", "Label_ 1"] {
            assert!(check_user_label_id(bad).is_err(), "{bad}");
        }
        assert!(check_user_label_id(&format!("Label_{}", "1".repeat(80))).is_err());
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

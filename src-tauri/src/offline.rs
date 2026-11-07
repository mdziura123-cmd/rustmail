//! Czysta logika trybu offline (bez bazy i bez sieci): filtry folderów po etykietach,
//! zapis etykiet w kolumnie, tokeny wyszukiwania i zabezpieczone wzorce LIKE.
//! Dzięki temu da się ją testować bez SQLite; zapytania SQL składa `store.rs` z gotowych fragmentów.

const MAX_TOKENS: usize = 6;
const MAX_TOKEN_CHARS: usize = 50;
pub const MAX_SEARCH_TEXT: usize = 200_000;

/// Etykiety, które wątek musi mieć (`must`) i których mieć nie może (`must_not`).
pub struct FolderFilter {
    pub must: &'static [&'static str],
    pub must_not: &'static [&'static str],
}

/// Odpowiednik zapytań Gmaila z `folder_query`. `None` = nieznany folder.
/// „suspicious” zwraca filtr skrzynki odbiorczej; ocenę ryzyka sprawdza wywołujący.
pub fn folder_filter(folder: &str) -> Option<FolderFilter> {
    const ST: &[&str] = &["SPAM", "TRASH"];
    let f = match folder {
        "inbox" | "suspicious" => FolderFilter { must: &["INBOX"], must_not: &[] },
        "unread" => FolderFilter { must: &["INBOX", "UNREAD"], must_not: &[] },
        "sent" => FolderFilter { must: &["SENT"], must_not: ST },
        "drafts" => FolderFilter { must: &["DRAFT"], must_not: ST },
        "spam" => FolderFilter { must: &["SPAM"], must_not: &[] },
        "trash" => FolderFilter { must: &["TRASH"], must_not: &[] },
        "archive" => FolderFilter { must: &[], must_not: &["INBOX", "SENT", "DRAFT", "SPAM", "TRASH"] },
        "starred" => FolderFilter { must: &["STARRED"], must_not: ST },
        "important" => FolderFilter { must: &["IMPORTANT"], must_not: ST },
        _ => return None,
    };
    Some(f)
}

/// Kolumna etykiet w postaci `|INBOX|UNREAD|` (posortowana, bez duplikatów, tylko bezpieczne znaki),
/// aby można było szukać `LIKE '%|INBOX|%'` bez pomyłek na podobnych nazwach.
pub fn labels_column(labels: &[String]) -> String {
    let mut v: Vec<String> = labels
        .iter()
        .map(|l| l.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').take(64).collect::<String>())
        .filter(|l| !l.is_empty())
        .collect();
    v.sort();
    v.dedup();
    if v.is_empty() {
        return "|".to_string();
    }
    format!("|{}|", v.join("|"))
}

/// Wzorzec LIKE dla jednej etykiety (stałe z białej listy, ale i tak przechodzi przez escape).
pub fn label_pattern(label: &str) -> String {
    format!("%|{}|%", escape_like(label))
}

pub fn escape_like(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        if c == '%' || c == '_' || c == '\\' {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

/// Wzorzec `%token%` z zabezpieczonymi znakami specjalnymi.
pub fn like_pattern(token: &str) -> String {
    format!("%{}%", escape_like(token))
}

/// Dzieli zapytanie na małe litery: najwyżej 6 słów po najwyżej 50 znaków, bez znaków sterujących.
pub fn search_tokens(q: &str) -> Vec<String> {
    q.split_whitespace()
        .map(|t| t.chars().filter(|c| !c.is_control()).collect::<String>().to_lowercase())
        .filter(|t| !t.is_empty())
        .map(|t| t.chars().take(MAX_TOKEN_CHARS).collect::<String>())
        .take(MAX_TOKENS)
        .collect()
}

/// Zapytanie do Gmaila (parametr q): bez znaków sterujących, przycięte do 500 znaków. Puste = błąd.
pub fn clean_remote_query(q: &str) -> Result<String, String> {
    let s: String = q.chars().filter(|c| !c.is_control()).collect();
    let s = s.trim();
    if s.is_empty() {
        return Err("Wpisz, czego szukać.".to_string());
    }
    Ok(s.chars().take(500).collect())
}

/// Tekst do lokalnego wyszukiwania: złączone fragmenty małymi literami, przycięte do limitu.
pub fn build_search_text(parts: &[&str], cap: usize) -> String {
    let mut out = String::new();
    for p in parts {
        if out.chars().count() >= cap {
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&p.to_lowercase());
    }
    out.chars().take(cap).collect()
}

/// Lokalny „token strony” to po prostu przesunięcie w liczbach dziesiętnych.
pub fn parse_offset(token: Option<&str>) -> usize {
    token.and_then(|t| t.trim().parse::<usize>().ok()).unwrap_or(0).min(100_000)
}

/// Czy etykieta jest w kolumnie `|A|B|`.
pub fn has_label(column: &str, label: &str) -> bool {
    column.contains(&format!("|{label}|"))
}

/// Czy Gmail odrzucił historię jako zbyt starą (błąd 404 z `history.list`).
pub fn is_not_found(msg: &str) -> bool {
    msg.starts_with("Gmail zwrócił błąd 404")
}

/// Czy komunikat błędu oznacza brak sieci (wtedy wolno użyć pamięci podręcznej).
pub fn is_network_error(msg: &str) -> bool {
    msg.starts_with("Brak połączenia")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_map_to_labels() {
        let f = folder_filter("unread").unwrap();
        assert_eq!(f.must, &["INBOX", "UNREAD"]);
        let a = folder_filter("archive").unwrap();
        assert!(a.must.is_empty() && a.must_not.contains(&"INBOX") && a.must_not.contains(&"TRASH"));
        assert!(folder_filter("spam").unwrap().must_not.is_empty());
        assert!(folder_filter("starred").unwrap().must_not.contains(&"SPAM"));
        assert!(folder_filter("suspicious").is_some());
        assert!(folder_filter("x; DROP TABLE").is_none());
    }

    #[test]
    fn labels_are_sanitized_and_sorted() {
        let l = vec!["UNREAD".to_string(), "INBOX".to_string(), "INBOX".to_string(), "Label_1|x%".to_string()];
        assert_eq!(labels_column(&l), "|INBOX|Label_1x|UNREAD|");
        assert_eq!(labels_column(&[]), "|");
        assert_eq!(label_pattern("INBOX"), "%|INBOX|%");
    }

    #[test]
    fn like_special_chars_are_escaped() {
        assert_eq!(like_pattern("100%_a\\b"), "%100\\%\\_a\\\\b%");
    }

    #[test]
    fn tokens_are_limited() {
        assert_eq!(search_tokens("  Faktura   MARZEC "), vec!["faktura", "marzec"]);
        assert_eq!(search_tokens("a b c d e f g h").len(), 6);
        let t = search_tokens(&"x".repeat(200));
        assert_eq!(t[0].chars().count(), 50);
        assert!(search_tokens("   \n ").is_empty());
        assert_eq!(search_tokens("ŁÓDŹ"), vec!["łódź"]);
        assert_eq!(search_tokens("a\u{0}b"), vec!["ab"]);
    }

    #[test]
    fn remote_query_is_cleaned() {
        assert_eq!(clean_remote_query(" from:ala \n").unwrap(), "from:ala");
        assert!(clean_remote_query("  ").is_err());
        assert_eq!(clean_remote_query(&"y".repeat(900)).unwrap().chars().count(), 500);
    }

    #[test]
    fn search_text_is_capped_and_lowercase() {
        let t = build_search_text(&["Temat", "Treść Wiadomości"], 1000);
        assert_eq!(t, "temat\ntreść wiadomości");
        assert_eq!(build_search_text(&["abcdef", "ghij"], 4), "abcd");
    }

    #[test]
    fn offset_parsing_is_forgiving() {
        assert_eq!(parse_offset(None), 0);
        assert_eq!(parse_offset(Some("40")), 40);
        assert_eq!(parse_offset(Some("-1")), 0);
        assert_eq!(parse_offset(Some("abc")), 0);
        assert_eq!(parse_offset(Some("99999999999")), 100_000);
    }

    #[test]
    fn label_column_lookup() {
        assert!(has_label("|INBOX|UNREAD|", "UNREAD"));
        assert!(!has_label("|INBOX|UNREAD_X|", "UNREAD"));
        assert!(!has_label("|", "INBOX"));
        assert!(is_not_found("Gmail zwrócił błąd 404 Not Found: x"));
        assert!(!is_not_found("Gmail zwrócił błąd 403"));
    }

    #[test]
    fn network_errors_are_recognized() {
        assert!(is_network_error("Brak połączenia z Gmail: timeout"));
        assert!(!is_network_error("Gmail zwrócił błąd 403"));
    }
}

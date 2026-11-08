//! Czysta logika protokołu IMAP (bez sieci): cytowanie, nazwy folderów (zmodyfikowany UTF-7),
//! rozbiór odpowiedzi LIST i FETCH. Wszystko oparte na samym `std`, dzięki czemu da się to testować osobno.

/// Znacznik miejsca, w którym w odpowiedzi stał literał `{N}` (jego bajty trafiają do osobnej listy).
pub const LIT: char = '\u{1}';

/// Cytuje tekst do polecenia IMAP. Odrzuca znaki sterujące i spoza ASCII (chroni przed wstrzyknięciem poleceń).
pub fn quote(s: &str) -> Result<String, String> {
    if s.is_empty() || s.len() > 200 {
        return Err("Niepoprawna wartość (pusta albo za długa).".to_string());
    }
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        if !(' '..='~').contains(&c) {
            return Err("Dozwolone są tylko znaki ASCII bez znaków sterujących.".to_string());
        }
        if c == '"' || c == '\\' {
            o.push('\\');
        }
        o.push(c);
    }
    o.push('"');
    Ok(o)
}

/// Nazwa serwera: litery, cyfry, kropki i myślniki; musi zawierać kropkę.
pub fn valid_host(h: &str) -> bool {
    let h = h.trim();
    h.len() >= 4
        && h.len() <= 100
        && h.contains('.')
        && !h.starts_with(['.', '-'])
        && !h.ends_with(['.', '-'])
        && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// Domyślny serwer IMAP na podstawie domeny adresu e-mail (tylko te, których nazwy są pewne).
pub fn default_host(email: &str) -> Option<&'static str> {
    let d = email.rsplit('@').next()?.trim().to_ascii_lowercase();
    match d.as_str() {
        "wp.pl" => Some("imap.wp.pl"),
        "o2.pl" | "tlen.pl" | "go2.pl" | "prokonto.pl" | "tmcc.pl" => Some("poczta.o2.pl"),
        _ => None,
    }
}

fn b64_val(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some((c - b'A') as u32),
        b'a'..=b'z' => Some((c - b'a') as u32 + 26),
        b'0'..=b'9' => Some((c - b'0') as u32 + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Dekoduje nazwę folderu ze zmodyfikowanego UTF-7 (np. `&AUI-` → `ł`). Przy błędzie zwraca tekst bez zmian.
pub fn mutf7_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'&' {
            let ch_len = s[i..].chars().next().map_or(1, |c| c.len_utf8());
            out.push_str(&s[i..i + ch_len]);
            i += ch_len;
            continue;
        }
        let Some(end) = s[i + 1..].find('-') else { return s.to_string() };
        let seg = &b[i + 1..i + 1 + end];
        i += end + 2;
        if seg.is_empty() {
            out.push('&');
            continue;
        }
        let mut bits: u32 = 0;
        let mut nbits = 0;
        let mut bytes: Vec<u8> = Vec::new();
        for &c in seg {
            let c = if c == b',' { b'/' } else { c };
            let Some(v) = b64_val(c) else { return s.to_string() };
            bits = (bits << 6) | v;
            nbits += 6;
            if nbits >= 8 {
                nbits -= 8;
                bytes.push(((bits >> nbits) & 0xff) as u8);
            }
        }
        let units: Vec<u16> = bytes.chunks_exact(2).map(|p| u16::from_be_bytes([p[0], p[1]])).collect();
        out.push_str(&String::from_utf16_lossy(&units));
    }
    out
}

pub struct ListEntry {
    /// Surowa nazwa folderu (do poleceń).
    pub name: String,
    pub attrs: Vec<String>,
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        let mut o = String::new();
        let mut esc = false;
        for c in s[1..s.len() - 1].chars() {
            if esc {
                o.push(c);
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else {
                o.push(c);
            }
        }
        o
    } else {
        s.to_string()
    }
}

/// Rozbiera linię `* LIST (\Atrybuty) "/" "Nazwa"`.
pub fn parse_list(head: &str, lits: &[Vec<u8>]) -> Option<ListEntry> {
    let rest = head.strip_prefix("* LIST ").or_else(|| head.strip_prefix("* LSUB "))?;
    let open = rest.find('(')?;
    let close = rest.find(')')?;
    if close < open {
        return None;
    }
    let attrs: Vec<String> = rest[open + 1..close].split_whitespace().map(|s| s.to_string()).collect();
    let after = rest[close + 1..].trim_start();
    // separator: "/" albo NIL
    let after = if let Some(r) = after.strip_prefix("NIL") {
        r
    } else if after.starts_with('"') {
        let end = after[1..].find('"')? + 2;
        &after[end..]
    } else {
        return None;
    };
    let name_tok = after.trim();
    if name_tok.is_empty() {
        return None;
    }
    let name = if name_tok == LIT.to_string() {
        String::from_utf8_lossy(lits.first()?).to_string()
    } else {
        unquote(name_tok)
    };
    if name.is_empty() {
        return None;
    }
    Some(ListEntry { name, attrs })
}

/// Rola folderu: inbox / sent / trash / spam / drafts / other.
pub fn role_of(raw_name: &str, attrs: &[String]) -> &'static str {
    for a in attrs {
        match a.to_ascii_lowercase().as_str() {
            "\\sent" => return "sent",
            "\\trash" => return "trash",
            "\\junk" => return "spam",
            "\\drafts" => return "drafts",
            _ => {}
        }
    }
    let n = mutf7_decode(raw_name).to_lowercase();
    if n == "inbox" {
        return "inbox";
    }
    let last = n.rsplit(['/', '.']).next().unwrap_or(&n).trim().to_string();
    match last.as_str() {
        "sent" | "sent items" | "sent mail" | "wysłane" | "wyslane" | "elementy wysłane" => "sent",
        "trash" | "deleted" | "deleted items" | "kosz" | "usunięte" | "usuniete" | "elementy usunięte" => "trash",
        "spam" | "junk" | "junk e-mail" | "bulk mail" | "wiadomości-śmieci" => "spam",
        "drafts" | "draft" | "robocze" | "szkice" | "wersje robocze" => "drafts",
        _ => "other",
    }
}

#[derive(Debug, Default)]
pub struct FetchInfo {
    pub seq: u32,
    pub uid: Option<u32>,
    pub flags: Vec<String>,
    pub internaldate: Option<String>,
}

/// Rozbiera początek odpowiedzi FETCH: `* 5 FETCH (UID 12 FLAGS (\Seen) INTERNALDATE "…" BODY[…] \u{1})`.
pub fn parse_fetch(head: &str) -> Option<FetchInfo> {
    let rest = head.strip_prefix("* ")?;
    let (seq_s, rest) = rest.split_once(' ')?;
    let seq: u32 = seq_s.parse().ok()?;
    let rest = rest.strip_prefix("FETCH ")?;
    let mut info = FetchInfo { seq, ..Default::default() };
    // Przed pierwszym literałem są tylko proste pola; ograniczamy rozbiór do tej części.
    let upto = rest.find(LIT).unwrap_or(rest.len());
    let part = &rest[..upto];
    if let Some(p) = part.find("UID ") {
        let n: String = part[p + 4..].chars().take_while(|c| c.is_ascii_digit()).collect();
        info.uid = n.parse().ok();
    }
    if let Some(p) = part.find("FLAGS (") {
        let r = &part[p + 7..];
        if let Some(e) = r.find(')') {
            info.flags = r[..e].split_whitespace().map(|s| s.to_string()).collect();
        }
    }
    if let Some(p) = part.find("INTERNALDATE \"") {
        let r = &part[p + 14..];
        if let Some(e) = r.find('"') {
            info.internaldate = Some(r[..e].to_string());
        }
    }
    Some(info)
}

/// `* 42 EXISTS` → 42
pub fn parse_exists(head: &str) -> Option<u32> {
    let r = head.strip_prefix("* ")?;
    let (n, tail) = r.split_once(' ')?;
    if tail.trim().eq_ignore_ascii_case("EXISTS") {
        n.parse().ok()
    } else {
        None
    }
}

/// Czy w treści odpowiedzi serwera występuje dana zdolność (`UIDPLUS`, `MOVE`…).
pub fn has_capability(head: &str, cap: &str) -> bool {
    head.starts_with("* CAPABILITY") && head.split_whitespace().any(|w| w.eq_ignore_ascii_case(cap))
}

/// Jeśli linia kończy się literałem `{N}` (lub `{N+}`), zwraca (długość_przed_nawiasem, N).
pub fn trailing_literal(line: &str) -> Option<(usize, usize)> {
    let t = line.strip_suffix('}')?;
    let open = t.rfind('{')?;
    let inner = &t[open + 1..];
    let inner = inner.strip_suffix('+').unwrap_or(inner);
    if inner.is_empty() || !inner.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((open, inner.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(quote("a\"b\\c").unwrap(), "\"a\\\"b\\\\c\"");
        assert!(quote("a\r\nLOGOUT").is_err());
        assert!(quote("zażółć").is_err());
        assert!(quote("").is_err());
    }

    #[test]
    fn hosts() {
        assert!(valid_host("imap.wp.pl"));
        assert!(!valid_host("imap wp.pl"));
        assert!(!valid_host("localhost"));
        assert!(!valid_host("a.pl/../x"));
        assert_eq!(default_host("Jan@WP.pl"), Some("imap.wp.pl"));
        assert_eq!(default_host("x@gmail.com"), None);
    }

    #[test]
    fn utf7() {
        assert_eq!(mutf7_decode("INBOX"), "INBOX");
        assert_eq!(mutf7_decode("Wys&AUI-ane"), "Wysłane");
        assert_eq!(mutf7_decode("A &- B"), "A & B");
        assert_eq!(mutf7_decode("&AUQ-"), "ń");
        assert_eq!(mutf7_decode("Z&AWE-&AUQ-"), "Zšń");
        assert_eq!(mutf7_decode("zepsute&"), "zepsute&");
    }

    #[test]
    fn list_lines() {
        let e = parse_list("* LIST (\\HasNoChildren \\Sent) \"/\" \"Sent\"", &[]).unwrap();
        assert_eq!(e.name, "Sent");
        assert_eq!(e.attrs, vec!["\\HasNoChildren", "\\Sent"]);
        let e = parse_list("* LIST () \".\" INBOX", &[]).unwrap();
        assert_eq!(e.name, "INBOX");
        let e = parse_list("* LIST () NIL \"Za \\\"x\\\"\"", &[]).unwrap();
        assert_eq!(e.name, "Za \"x\"");
        let lit = LIT.to_string();
        let e = parse_list(&format!("* LIST () \"/\" {lit}"), &[b"Folder".to_vec()]).unwrap();
        assert_eq!(e.name, "Folder");
        assert!(parse_list("* OK coś", &[]).is_none());
    }

    #[test]
    fn roles() {
        assert_eq!(role_of("INBOX", &[]), "inbox");
        assert_eq!(role_of("Wys&AUI-ane", &[]), "sent");
        assert_eq!(role_of("Kosz", &[]), "trash");
        assert_eq!(role_of("INBOX.Spam", &[]), "spam");
        assert_eq!(role_of("Coś", &["\\Junk".to_string()]), "spam");
        assert_eq!(role_of("Prywatne", &[]), "other");
    }

    #[test]
    fn fetch_lines() {
        let lit = LIT.to_string();
        let h = format!("* 5 FETCH (UID 123 FLAGS (\\Seen \\Flagged) INTERNALDATE \"08-Nov-2026 10:00:00 +0100\" BODY[HEADER.FIELDS (FROM SUBJECT DATE)] {lit})");
        let f = parse_fetch(&h).unwrap();
        assert_eq!(f.seq, 5);
        assert_eq!(f.uid, Some(123));
        assert_eq!(f.flags, vec!["\\Seen", "\\Flagged"]);
        assert_eq!(f.internaldate.as_deref(), Some("08-Nov-2026 10:00:00 +0100"));
        let f = parse_fetch("* 7 FETCH (FLAGS ())").unwrap();
        assert!(f.flags.is_empty() && f.uid.is_none());
        assert!(parse_fetch("* 7 EXISTS").is_none());
        // "UID" wewnątrz literału nie jest rozbierany (część po LIT jest pomijana)
        let h = format!("* 1 FETCH (FLAGS () {lit} UID 999)");
        assert_eq!(parse_fetch(&h).unwrap().uid, None);
    }

    #[test]
    fn exists_and_caps() {
        assert_eq!(parse_exists("* 42 EXISTS"), Some(42));
        assert_eq!(parse_exists("* 42 RECENT"), None);
        assert!(has_capability("* CAPABILITY IMAP4rev1 UIDPLUS MOVE", "uidplus"));
        assert!(!has_capability("* CAPABILITY IMAP4rev1", "MOVE"));
    }

    #[test]
    fn literals() {
        assert_eq!(trailing_literal("* 1 FETCH (BODY[] {342}"), Some((18, 342)));
        assert_eq!(trailing_literal("x {12+}"), Some((2, 12)));
        assert_eq!(trailing_literal("abc {x}"), None);
        assert_eq!(trailing_literal("abc"), None);
    }
}

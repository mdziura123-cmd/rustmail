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

/// Element polecenia SEARCH: zwykły tekst albo literał (wartość z polskimi znakami wysyłana osobno, w UTF-8).
#[derive(Debug, Clone, PartialEq)]
pub enum SPart {
    Text(String),
    Lit(String),
}

fn split_terms(q: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut had = false;
    for c in q.chars() {
        match c {
            '"' => {
                in_q = !in_q;
                had = true;
            }
            c if c.is_whitespace() && !in_q => {
                if had || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    had = false;
                }
            }
            c => cur.push(c),
        }
    }
    if in_q {
        return Err("Zamknij cudzysłów w wyszukiwanej frazie.".to_string());
    }
    if had || !cur.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

fn value_parts(key: &str, v: &str) -> Result<Vec<SPart>, String> {
    if v.is_empty() || v.chars().count() > 100 || v.chars().any(|c| c.is_control()) {
        return Err("Fraza w wyszukiwaniu jest pusta, za długa albo zawiera znaki sterujące.".to_string());
    }
    if v.is_ascii() {
        Ok(vec![SPart::Text(format!("{key} {}", quote(v)?))])
    } else {
        Ok(vec![SPart::Text(format!("{key} ")), SPart::Lit(v.to_string())])
    }
}

/// Zamienia frazę wyszukiwania na kryteria IMAP SEARCH. Obsługiwane: słowa (cały tekst), `od:`, `do:`, `temat:`
/// (także `from:`, `to:`, `subject:`), `is:unread`, `is:read`, `is:starred`, fraza w cudzysłowie. Wszystkie warunki
/// łączą się spójnikiem „i”. Wartości spoza ASCII idą jako literały UTF-8 (patrz `search_needs_utf8`).
pub fn search_parts(q: &str) -> Result<Vec<SPart>, String> {
    let terms = split_terms(q)?;
    if terms.is_empty() {
        return Err("Wpisz frazę do wyszukania.".to_string());
    }
    if terms.len() > 8 {
        return Err("Za dużo warunków w wyszukiwaniu (najwyżej 8).".to_string());
    }
    let mut parts: Vec<SPart> = Vec::new();
    for (i, t) in terms.iter().enumerate() {
        if i > 0 {
            parts.push(SPart::Text(" ".to_string()));
        }
        let low = t.to_lowercase();
        let one = match low.split_once(':') {
            Some(("od" | "from", _)) => value_parts("FROM", &t[t.find(':').unwrap_or(0) + 1..])?,
            Some(("do" | "to", _)) => value_parts("TO", &t[t.find(':').unwrap_or(0) + 1..])?,
            Some(("temat" | "subject", _)) => value_parts("SUBJECT", &t[t.find(':').unwrap_or(0) + 1..])?,
            Some(("is", "unread")) => vec![SPart::Text("UNSEEN".to_string())],
            Some(("is", "read")) => vec![SPart::Text("SEEN".to_string())],
            Some(("is", "starred")) => vec![SPart::Text("FLAGGED".to_string())],
            Some(("is", _)) => return Err("Nieznany warunek „is:” (dozwolone: is:unread, is:read, is:starred).".to_string()),
            _ => value_parts("TEXT", t)?,
        };
        parts.extend(one);
    }
    // sklejamy sąsiednie fragmenty tekstu
    let mut out: Vec<SPart> = Vec::new();
    for p in parts {
        match (out.last_mut(), p) {
            (Some(SPart::Text(a)), SPart::Text(b)) => a.push_str(&b),
            (_, p) => out.push(p),
        }
    }
    Ok(out)
}

/// Czy wyszukiwanie zawiera literały spoza ASCII (wtedy polecenie musi mieć `CHARSET UTF-8`).
pub fn search_needs_utf8(parts: &[SPart]) -> bool {
    parts.iter().any(|p| matches!(p, SPart::Lit(_)))
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
            "\\archive" | "\\all" => return "archive",
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
        "archive" | "archiwum" | "all mail" | "wszystkie wiadomości" => "archive",
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

// ---------- Wysyłanie (SMTP) ----------

/// Serwer SMTP na podstawie serwera IMAP (`imap.wp.pl` → `smtp.wp.pl`; inne nazwy bez zmian).
pub fn smtp_host(imap_host: &str) -> String {
    match imap_host.strip_prefix("imap.") {
        Some(rest) => format!("smtp.{rest}"),
        None => imap_host.to_string(),
    }
}

/// Sam adres z zapisu `Imię <adres@x.pl>` albo `adres@x.pl`.
pub fn bare_addr(s: &str) -> String {
    let t = s.trim();
    match (t.rfind('<'), t.rfind('>')) {
        (Some(a), Some(b)) if a < b => t[a + 1..b].trim().to_string(),
        _ => t.to_string(),
    }
}

/// Nazwa wyświetlana z nagłówka Od (`"Jan K." <a@b.pl>` → `Jan K.`); pusta, gdy jest sam adres.
pub fn display_name(s: &str) -> String {
    let t = s.trim();
    match t.rfind('<') {
        Some(a) => t[..a].trim().trim_matches('"').trim().to_string(),
        None => String::new(),
    }
}

/// Numer UID z kodu odpowiedzi serwera: `[APPENDUID <v> <uid>]` albo `[COPYUID <v> <src> <dst>]` (ostatnia liczba).
pub fn resp_uid(text: &str, code: &str) -> Option<u32> {
    let up = text.to_ascii_uppercase();
    let start = up.find(&format!("[{} ", code.to_ascii_uppercase()))?;
    let rest = &text[start + code.len() + 2..];
    let end = rest.find(']')?;
    rest[..end].split_whitespace().last()?.parse().ok()
}

/// Data w formacie RFC 2822 (czas UTC), np. `Mon, 09 Nov 2026 18:30:00 +0000`.
pub fn rfc2822_date(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    const WD: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MO: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let wd = WD[((days % 7 + 11) % 7) as usize];
    format!("{wd}, {d:02} {} {y} {h:02}:{mi:02}:{s:02} +0000", MO[(m - 1) as usize])
}

/// Przygotowuje wiadomość do wysłania przez SMTP: usuwa nagłówek Bcc (ukryci odbiorcy nie mogą trafić do treści)
/// i dodaje Date oraz Message-ID, jeśli ich brakuje.
pub fn smtp_prepare(raw: &[u8], message_id: &str, date: &str) -> Vec<u8> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap_or(raw.len());
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let rest = &raw[split.min(raw.len())..];
    let mut out = String::new();
    let mut skipping = false;
    let (mut has_date, mut has_id) = (false, false);
    for line in head.split("\r\n") {
        if line.starts_with(' ') || line.starts_with('\t') {
            if !skipping {
                out.push_str(line);
                out.push_str("\r\n");
            }
            continue;
        }
        let low = line.to_ascii_lowercase();
        skipping = low.starts_with("bcc:");
        if skipping {
            continue;
        }
        if low.starts_with("date:") {
            has_date = true;
        }
        if low.starts_with("message-id:") {
            has_id = true;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    if !has_date {
        out.push_str(&format!("Date: {date}\r\n"));
    }
    if !has_id {
        out.push_str(&format!("Message-ID: <{message_id}>\r\n"));
    }
    let mut v = out.trim_end_matches("\r\n").as_bytes().to_vec();
    v.extend_from_slice(rest);
    v
}

/// Wartości nagłówków To, Cc i Bcc z gotowej wiadomości (złożone w jedną listę rozdzieloną przecinkami).
/// Służy do ustalenia odbiorców przy wysyłce wiadomości z kolejki. Czytany jest tylko blok nagłówków.
pub fn recipient_headers(raw: &[u8]) -> String {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap_or(raw.len());
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let mut vals: Vec<String> = Vec::new();
    let mut cur: Option<String> = None;
    for line in head.split("\r\n") {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(c) = cur.as_mut() {
                c.push(' ');
                c.push_str(line.trim());
            }
            continue;
        }
        if let Some(c) = cur.take() {
            vals.push(c);
        }
        let low = line.to_ascii_lowercase();
        for name in ["to:", "cc:", "bcc:"] {
            if low.starts_with(name) {
                cur = Some(line[name.len()..].trim().to_string());
            }
        }
    }
    if let Some(c) = cur.take() {
        vals.push(c);
    }
    vals.retain(|v| !v.is_empty());
    vals.join(", ")
}

/// Czy błąd powstał, zanim cokolwiek wysłano do serwera (brak połączenia z serwerem). Tylko wtedy wolno
/// bezpiecznie zapisać wiadomość w kolejce i ponowić – przy błędzie po rozpoczęciu wysyłki mogłaby wyjść dwa razy.
pub fn is_connect_error(e: &str) -> bool {
    e.starts_with("Nie można połączyć z serwerem")
        || e.starts_with("Błąd bezpiecznego połączenia")
        || (e.starts_with("Serwer ") && e.ends_with(" nie odpowiada."))
}

/// Przygotowuje treść do polecenia DATA: podwaja kropki na początku linii i dodaje końcowe `.`.
pub fn dot_stuff(data: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(data.len() + 16);
    let mut at_line_start = true;
    for &b in data {
        if at_line_start && b == b'.' {
            out.push(b'.');
        }
        out.push(b);
        at_line_start = b == b'\n';
    }
    if !out.ends_with(b"\r\n") {
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b".\r\n");
    out
}

/// Linia odpowiedzi SMTP: (kod, czy_będą_kolejne_linie, tekst).
pub fn parse_smtp_line(line: &str) -> Option<(u16, bool, String)> {
    let code: u16 = line.get(..3)?.parse().ok()?;
    let sep = line.as_bytes().get(3).copied().unwrap_or(b' ');
    Some((code, sep == b'-', line.get(4..).unwrap_or("").to_string()))
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
        assert_eq!(role_of("INBOX/Archive", &[]), "archive");
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

    #[test]
    fn smtp_helpers() {
        assert_eq!(smtp_host("imap.wp.pl"), "smtp.wp.pl");
        assert_eq!(smtp_host("poczta.o2.pl"), "poczta.o2.pl");
        assert_eq!(bare_addr("Jan <jan@x.pl>"), "jan@x.pl");
        assert_eq!(bare_addr(" a@b.pl "), "a@b.pl");
        assert_eq!(rfc2822_date(0), "Thu, 01 Jan 1970 00:00:00 +0000");
        assert_eq!(rfc2822_date(1794249000), "Mon, 09 Nov 2026 18:30:00 +0000");
        assert_eq!(rfc2822_date(951_782_400), "Tue, 29 Feb 2000 00:00:00 +0000");
        let raw = b"From: a@b.pl\r\nTo: c@d.pl\r\nBcc: ukryty@x.pl,\r\n inny@x.pl\r\nSubject: Hej\r\n\r\nTre\xc5\x9b\xc4\x87\r\n.kropka\r\n";
        let out = String::from_utf8_lossy(&smtp_prepare(raw, "id1@rustmail", "DATA")).to_string();
        assert!(!out.to_lowercase().contains("bcc"));
        assert!(!out.contains("ukryty") && !out.contains("inny@x.pl"));
        assert!(out.contains("Date: DATA\r\n") && out.contains("Message-ID: <id1@rustmail>\r\n"));
        assert!(out.contains("Subject: Hej\r\n") && out.contains("<id1@rustmail>\r\n\r\nTre"));
        let again = smtp_prepare(out.as_bytes(), "zz", "inna");
        assert_eq!(String::from_utf8_lossy(&again).matches("Date:").count(), 1);
        let st = dot_stuff(b".a\r\nb\r\n.\r\nc");
        assert_eq!(String::from_utf8(st).unwrap(), "..a\r\nb\r\n..\r\nc\r\n.\r\n");
        assert_eq!(parse_smtp_line("250-AUTH PLAIN LOGIN"), Some((250, true, "AUTH PLAIN LOGIN".to_string())));
        assert_eq!(parse_smtp_line("250 OK"), Some((250, false, "OK".to_string())));
        assert_eq!(parse_smtp_line("zz"), None);
    }

    #[test]
    fn reads_uid_from_response_codes() {
        assert_eq!(resp_uid("[APPENDUID 1699 42] APPEND completed", "APPENDUID"), Some(42));
        assert_eq!(resp_uid("[COPYUID 1699 7 31] Copy completed", "copyuid"), Some(31));
        assert_eq!(resp_uid("[COPYUID 1 5:7 9:11] ok", "COPYUID"), None);
        assert_eq!(resp_uid("Copy completed", "COPYUID"), None);
        assert_eq!(resp_uid("[APPENDUID 1", "APPENDUID"), None);
    }

    #[test]
    fn recipient_headers_collect_to_cc_bcc_only_from_header_block() {
        let raw = b"From: a@x.pl\r\nTo: b@y.pl,\r\n c@y.pl\r\nCc: d@y.pl\r\nBcc: e@y.pl\r\nSubject: To: zly@z.pl\r\n\r\nTo: tresc@z.pl\r\nBcc: tresc2@z.pl";
        let h = recipient_headers(raw);
        assert_eq!(h, "b@y.pl, c@y.pl, d@y.pl, e@y.pl");
        assert_eq!(recipient_headers(b"Subject: x\r\n\r\nTo: a@b.pl"), "");
        assert_eq!(recipient_headers(b"TO: A@B.PL\r\n\r\n"), "A@B.PL");
    }

    #[test]
    fn only_pre_send_connection_errors_allow_queueing() {
        assert!(is_connect_error("Nie można połączyć z serwerem smtp.wp.pl: timed out"));
        assert!(is_connect_error("Błąd bezpiecznego połączenia (TLS) z smtp.wp.pl: x"));
        assert!(is_connect_error("Serwer smtp.wp.pl nie odpowiada."));
        assert!(!is_connect_error("Serwer SMTP zamknął połączenie."));
        assert!(!is_connect_error("Serwer SMTP nie przyjął wiadomości (554): spam"));
        assert!(!is_connect_error("Logowanie do serwera SMTP nie powiodło się (x)."));
    }

    #[test]
    fn display_name_extraction() {
        assert_eq!(display_name("\"Jan K.\" <jan@x.pl>"), "Jan K.");
        assert_eq!(display_name("PayPal <a@b.pl>"), "PayPal");
        assert_eq!(display_name("jan@x.pl"), "");
        assert_eq!(display_name("<jan@x.pl>"), "");
    }

    fn t(s: &str) -> SPart {
        SPart::Text(s.to_string())
    }
    fn l(s: &str) -> SPart {
        SPart::Lit(s.to_string())
    }

    #[test]
    fn search_plain_words_and_operators() {
        assert_eq!(search_parts("faktura").unwrap(), vec![t("TEXT \"faktura\"")]);
        assert_eq!(
            search_parts("od:jan@x.pl temat:\"plan pracy\" is:unread").unwrap(),
            vec![t("FROM \"jan@x.pl\" SUBJECT \"plan pracy\" UNSEEN")]
        );
        assert_eq!(search_parts("Do:a@b.pl IS:Starred").unwrap(), vec![t("TO \"a@b.pl\" FLAGGED")]);
        assert_eq!(search_parts("\"dwa słowa\"").unwrap().len(), 2); // spoza ASCII → literał
    }

    #[test]
    fn search_non_ascii_values_become_literals_in_order() {
        let p = search_parts("od:Łukasz temat:umowa zażółć").unwrap();
        assert_eq!(p, vec![t("FROM "), l("Łukasz"), t(" SUBJECT \"umowa\" TEXT "), l("zażółć")]);
        assert!(search_needs_utf8(&p));
        assert!(!search_needs_utf8(&search_parts("abc").unwrap()));
    }

    #[test]
    fn search_rejects_bad_input() {
        assert!(search_parts("").is_err());
        assert!(search_parts("   ").is_err());
        assert!(search_parts("\"niezamknięty").is_err());
        assert!(search_parts("is:bogus").is_err());
        assert!(search_parts("od:").is_err());
        assert!(search_parts("a b c d e f g h i").is_err());
        assert!(search_parts("a\u{0}b").is_err());
        // wstrzyknięcie: cudzysłów i backslash są zawsze escapowane
        assert_eq!(search_parts("a\\b").unwrap(), vec![t("TEXT \"a\\\\b\"")]);
    }
}

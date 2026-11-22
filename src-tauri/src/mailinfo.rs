//! Dodatkowe informacje o wiadomości wyliczane lokalnie z nagłówków (wyłącznie biblioteka standardowa,
//! bez sieci, w pełni testowalne):
//! - wypisywanie się z listy mailingowej (nagłówek List-Unsubscribe),
//! - werdykt weryfikacji nadawcy (DMARC według serwera poczty),
//! - wycinanie bloku nagłówków do podglądu źródła,
//! - normalizacja tematu (do wyciszania rozmów w kontach IMAP, które nie mają wątków).

use serde::{Deserialize, Serialize};

/// Sposób wypisania się z listy. Adres https otwiera się w przeglądarce dopiero po potwierdzeniu
/// użytkownika; adres e-mail otwiera okno nowej wiadomości (nic nie jest wysyłane samo).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unsubscribe {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub mailto: Option<String>,
    #[serde(default)]
    pub mail_subject: String,
    #[serde(default)]
    pub mail_body: String,
    /// Nadawca deklaruje wypisanie jednym kliknięciem (RFC 8058).
    #[serde(default)]
    pub one_click: bool,
}

const MAX_URL: usize = 2000;

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Dekodowanie %XX (adresy mailto). Niepoprawne sekwencje zostają bez zmian.
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn simple_email(e: &str) -> bool {
    let Some((local, domain)) = e.split_once('@') else { return false };
    !local.is_empty()
        && domain.contains('.')
        && !domain.contains('@')
        && e.len() <= 254
        && !e.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | ',' | ';' | '"' | '\'' | '\\'))
}

fn clean_line(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_string()
}

/// Odczytuje nagłówki List-Unsubscribe i List-Unsubscribe-Post. `None`, gdy nie ma żadnego
/// użytecznego sposobu wypisania. Adresy http (bez szyfrowania) są pomijane.
pub fn parse_list_unsubscribe(value: &str, post: &str) -> Option<Unsubscribe> {
    let mut u = Unsubscribe::default();
    // Wartości są w nawiasach ostrych, rozdzielone przecinkami; nagłówek bywa łamany (białe znaki).
    let mut rest = value;
    while let Some(a) = rest.find('<') {
        let Some(b) = rest[a + 1..].find('>') else { break };
        let item: String = rest[a + 1..a + 1 + b].chars().filter(|c| !c.is_whitespace()).collect();
        rest = &rest[a + 1 + b + 1..];
        if item.len() > MAX_URL || item.chars().any(|c| c.is_control()) {
            continue;
        }
        let lower = item.to_ascii_lowercase();
        if lower.starts_with("https://") && u.url.is_none() {
            let authority = item[8..].split(['/', '?', '#']).next().unwrap_or("");
            if !authority.is_empty() && !authority.contains('@') {
                u.url = Some(item);
            }
        } else if lower.starts_with("mailto:") && u.mailto.is_none() {
            let body = &item[7..];
            let (addr, query) = body.split_once('?').unwrap_or((body, ""));
            let addr = percent_decode(addr).trim().to_lowercase();
            if !simple_email(&addr) {
                continue;
            }
            for pair in query.split('&') {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                let v = percent_decode(&v.replace('+', " "));
                match k.to_ascii_lowercase().as_str() {
                    "subject" => u.mail_subject = clean_line(&v, 200),
                    "body" => u.mail_body = v.chars().filter(|c| *c == '\n' || !c.is_control()).take(2000).collect(),
                    _ => {}
                }
            }
            u.mailto = Some(addr);
        }
    }
    if u.url.is_none() && u.mailto.is_none() {
        return None;
    }
    u.one_click = u.url.is_some() && post.to_ascii_lowercase().replace(' ', "").contains("list-unsubscribe=one-click");
    Some(u)
}

/// Werdykt weryfikacji nadawcy na podstawie wyniku DMARC wyliczonego przez serwer poczty:
/// `verified` (domena w polu Od potwierdzona), `failed` (weryfikacja nie powiodła się), `unknown`.
/// DMARC jest jedynym z trzech testów, który sprawdza właśnie adres widoczny w polu „Od”.
pub fn auth_verdict(dmarc: Option<&str>) -> &'static str {
    match dmarc {
        Some("pass") => "verified",
        Some("fail") => "failed",
        _ => "unknown",
    }
}

/// Weryfikacja nadawcy w Poczcie WP. Serwer WP nie dodaje Authentication-Results, tylko nagłówek
/// `X-WP-DKIM-Status: good (id: domena)` z wynikiem sprawdzenia podpisu DKIM.
/// - `verified`: dokładnie jeden taki nagłówek, wynik „good”, a podpisująca domena to domena z pola „Od”
///   (ta sama domena rejestrowalna – jak przy DMARC),
/// - `failed`: wynik wyraźnie negatywny (bad/fail/invalid…),
/// - `unknown`: brak nagłówka, podpis innej domeny (np. firmy wysyłającej newslettery) albo więcej niż
///   jeden nagłówek – wtedy któryś mógł zostać podstawiony przez nadawcę, więc żadnemu nie ufamy.
pub fn wp_dkim_verdict(values: &[String], from_email: &str) -> &'static str {
    if values.len() != 1 {
        return "unknown";
    }
    let v = values[0].trim().to_ascii_lowercase();
    let status = v.split(|c: char| c.is_whitespace() || c == '(').next().unwrap_or("");
    if matches!(status, "bad" | "fail" | "failed" | "invalid" | "error") {
        return "failed";
    }
    if status != "good" {
        return "unknown";
    }
    let Some(id) = v.split("id:").nth(1) else { return "unknown" };
    let signer: String = id.trim().chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-').collect();
    let from_dom = crate::phishing::domain_of(from_email);
    if signer.is_empty() || from_dom.is_empty() || !from_email.contains('@') {
        return "unknown";
    }
    if crate::phishing::registrable(&signer) == crate::phishing::registrable(&from_dom) {
        "verified"
    } else {
        "unknown"
    }
}

/// Czy konto IMAP to Poczta WP (tylko tam nagłówek X-WP-DKIM-Status pochodzi od serwera).
pub fn is_wp_host(host: &str) -> bool {
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    h == "wp.pl" || h.ends_with(".wp.pl")
}

/// Blok nagłówków surowej wiadomości (do pierwszej pustej linii).
pub fn header_block(raw: &[u8]) -> &[u8] {
    let find = |pat: &[u8]| raw.windows(pat.len()).position(|w| w == pat);
    match (find(b"\r\n\r\n"), find(b"\n\n")) {
        (Some(a), Some(b)) => &raw[..a.min(b)],
        (Some(a), None) => &raw[..a],
        (None, Some(b)) => &raw[..b],
        (None, None) => raw,
    }
}

/// Maksymalny rozmiar pokazywanego źródła (większe wiadomości są przycinane – to zwykle załączniki).
pub const SOURCE_MAX: usize = 3_000_000;

/// Źródło do pokazania: nagłówki osobno i całość przycięta do `SOURCE_MAX` bajtów.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub headers: String,
    pub full: String,
    pub truncated: bool,
    pub size: usize,
}

pub fn source_of(raw: &[u8]) -> Source {
    let headers = String::from_utf8_lossy(header_block(raw)).into_owned();
    let truncated = raw.len() > SOURCE_MAX;
    let full = String::from_utf8_lossy(&raw[..raw.len().min(SOURCE_MAX)]).into_owned();
    Source { headers, full, truncated, size: raw.len() }
}

/// Temat bez przedrostków odpowiedzi i przekazania (Re:, Odp:, Fwd:, PD:, AW:, WG: …),
/// małymi literami i z pojedynczymi spacjami. Pusty temat daje pusty wynik.
pub fn normalize_subject(s: &str) -> String {
    const PREFIXES: &[&str] = &["re", "odp", "fw", "fwd", "pd", "aw", "wg", "sv", "vs", "tr", "rv"];
    let mut t: String = s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    loop {
        let trimmed = t.trim_start();
        let mut cut = None;
        // "[lista] Re: temat" – znacznik listy na początku też pomijamy
        if trimmed.starts_with('[') {
            if let Some(end) = trimmed.find(']') {
                if end < 40 {
                    cut = Some(end + 1);
                }
            }
        }
        if cut.is_none() {
            if let Some(colon) = trimmed.find(':') {
                let head = trimmed[..colon].trim();
                // "Re[2]:", "Re 2:" itp.
                let word: String = head.chars().take_while(|c| c.is_alphabetic()).collect();
                let tail = head[word.len()..].trim().trim_start_matches('[').trim_end_matches(']').trim();
                if PREFIXES.contains(&word.as_str()) && tail.chars().all(|c| c.is_ascii_digit()) {
                    cut = Some(colon + 1);
                }
            }
        }
        match cut {
            Some(n) => t = trimmed[n..].to_string(),
            None => {
                t = trimmed.trim().to_string();
                break;
            }
        }
    }
    t.chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsubscribe_https_and_mailto() {
        let u = parse_list_unsubscribe(
            "<mailto:unsub@news.example.com?subject=Wypisz%20mnie&body=stop>, <https://news.example.com/u?id=1&k=2>",
            "List-Unsubscribe=One-Click",
        )
        .unwrap();
        assert_eq!(u.url.as_deref(), Some("https://news.example.com/u?id=1&k=2"));
        assert_eq!(u.mailto.as_deref(), Some("unsub@news.example.com"));
        assert_eq!(u.mail_subject, "Wypisz mnie");
        assert_eq!(u.mail_body, "stop");
        assert!(u.one_click);
    }

    #[test]
    fn unsubscribe_rejects_unsafe() {
        assert!(parse_list_unsubscribe("", "").is_none());
        assert!(parse_list_unsubscribe("<http://plain.example.com/u>", "").is_none());
        assert!(parse_list_unsubscribe("<https://user@evil.example.com/u>", "").is_none());
        assert!(parse_list_unsubscribe("<javascript:alert(1)>", "").is_none());
        assert!(parse_list_unsubscribe("<mailto:not-an-address>", "").is_none());
        // wartość łamana w kilku liniach
        let u = parse_list_unsubscribe("<https://x.example.com/\r\n u>", "").unwrap();
        assert_eq!(u.url.as_deref(), Some("https://x.example.com/u"));
        assert!(!u.one_click);
        // tylko mailto: brak „jednego kliknięcia”, nawet z nagłówkiem Post
        let m = parse_list_unsubscribe("<mailto:a@b.pl>", "List-Unsubscribe=One-Click").unwrap();
        assert!(!m.one_click);
        assert!(m.url.is_none());
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("%C5%BC%C3%B3%C5%82w"), "żółw");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%4"), "%4");
    }

    #[test]
    fn verdicts() {
        assert_eq!(auth_verdict(Some("pass")), "verified");
        assert_eq!(auth_verdict(Some("fail")), "failed");
        assert_eq!(auth_verdict(Some("none")), "unknown");
        assert_eq!(auth_verdict(None), "unknown");
    }

    #[test]
    fn wp_dkim() {
        let one = |s: &str| vec![s.to_string()];
        assert_eq!(wp_dkim_verdict(&one("good (id: przelewy24.pl)   "), "no-reply@przelewy24.pl"), "verified");
        assert_eq!(wp_dkim_verdict(&one("good (id: przelewy24.pl)"), "info@mail.przelewy24.pl"), "verified");
        assert_eq!(wp_dkim_verdict(&one("good (id: sendgrid.net)"), "news@sklep.pl"), "unknown");
        assert_eq!(wp_dkim_verdict(&one("good (id: przelewy24.pl.evil.com)"), "no-reply@przelewy24.pl"), "unknown");
        assert_eq!(wp_dkim_verdict(&one("bad (id: bank.pl)"), "a@bank.pl"), "failed");
        assert_eq!(wp_dkim_verdict(&one("none"), "a@bank.pl"), "unknown");
        assert_eq!(wp_dkim_verdict(&[], "a@bank.pl"), "unknown");
        // dwa nagłówki: jeden mógł podstawić nadawca
        let two = vec!["good (id: bank.pl)".to_string(), "good (id: bank.pl)".to_string()];
        assert_eq!(wp_dkim_verdict(&two, "a@bank.pl"), "unknown");
        assert_eq!(wp_dkim_verdict(&one("good (id: bank.pl)"), "zly-adres"), "unknown");
        assert!(is_wp_host("imap.wp.pl"));
        assert!(is_wp_host("WP.PL"));
        assert!(!is_wp_host("imap.wp.pl.evil.com"));
        assert!(!is_wp_host("notwp.pl"));
    }

    #[test]
    fn header_blocks() {
        assert_eq!(header_block(b"A: 1\r\nB: 2\r\n\r\nbody"), b"A: 1\r\nB: 2");
        assert_eq!(header_block(b"A: 1\n\nbody\r\n\r\nmore"), b"A: 1");
        assert_eq!(header_block(b"only headers"), b"only headers");
        let s = source_of(b"Subject: x\r\n\r\ntre\xc5\x9b\xc4\x87");
        assert_eq!(s.headers, "Subject: x");
        assert!(s.full.ends_with("treść"));
        assert!(!s.truncated);
    }

    #[test]
    fn subjects() {
        assert_eq!(normalize_subject("Re: Odp: FWD:  Spotkanie   w piątek"), "spotkanie w piątek");
        assert_eq!(normalize_subject("[lista] Re[2]: Temat"), "temat");
        assert_eq!(normalize_subject("RE 3: Temat"), "temat");
        assert_eq!(normalize_subject("Rezerwacja: hotel"), "rezerwacja: hotel");
        assert_eq!(normalize_subject("Pd: Faktura"), "faktura");
        assert_eq!(normalize_subject(""), "");
        assert_eq!(normalize_subject("Re:"), "");
    }
}

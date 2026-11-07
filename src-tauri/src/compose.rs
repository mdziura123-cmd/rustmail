//! Budowanie wiadomości e-mail (MIME) do wysłania lub zapisania jako wersja robocza.
//!
//! Ten moduł celowo korzysta tylko z biblioteki standardowej: jest to czysta logika bez sieci,
//! więc da się ją w pełni przetestować. Najważniejsze zabezpieczenie: żadne pole nagłówka nie może
//! zawierać znaków nowej linii (to blokuje wstrzyknięcie dodatkowych nagłówków, np. ukrytej kopii).

pub const MAX_ATTACHMENTS_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_RECIPIENTS: usize = 100;
const MAX_SUBJECT_CHARS: usize = 300;

pub struct OutAttachment {
    pub name: String,
    pub mime: String,
    pub data: Vec<u8>,
}

pub struct Outgoing {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub text: String,
    pub html: String,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub attachments: Vec<OutAttachment>,
}

// ---------- Base64 (standardowy alfabet) ----------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Base64 podzielone na linie po 76 znaków, rozdzielone CRLF (wymóg MIME).
fn base64_wrapped(data: &[u8]) -> String {
    let enc = base64_encode(data);
    let mut out = String::with_capacity(enc.len() + enc.len() / 76 * 2 + 2);
    let bytes = enc.as_bytes();
    for line in bytes.chunks(76) {
        out.push_str(std::str::from_utf8(line).unwrap_or(""));
        out.push_str("\r\n");
    }
    out
}

// ---------- Nagłówki ----------

/// Zastępuje znaki sterujące spacją – nagłówek nigdy nie może zawierać CR/LF.
pub fn clean_header_value(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string()
}

/// Koduje tekst nagłówka (RFC 2047), jeśli zawiera znaki spoza ASCII.
pub fn encode_header_text(s: &str) -> String {
    let s = clean_header_value(s);
    if s.is_ascii() {
        return s;
    }
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if cur.len() + ch.len_utf8() > 42 {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
        .iter()
        .map(|w| format!("=?UTF-8?B?{}?=", base64_encode(w.as_bytes())))
        .collect::<Vec<_>>()
        .join("\r\n ")
}

/// Sprawdza pojedynczy adres e-mail. Odrzuca spacje, znaki sterujące, cudzysłowy i nawiasy.
pub fn validate_address(addr: &str) -> Result<String, String> {
    let a = addr.trim();
    let bad = |c: char| c.is_control() || c.is_whitespace() || "<>(),;:\\\"[]".contains(c);
    if a.is_empty() || a.len() > 254 || a.chars().any(bad) {
        return Err(format!("Niepoprawny adres e-mail: {}", clean_header_value(addr)));
    }
    let Some((local, domain)) = a.split_once('@') else {
        return Err(format!("Niepoprawny adres e-mail: {a}"));
    };
    let domain_ok = domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
        && !domain.contains('@')
        && !domain.starts_with('-');
    if local.is_empty() || local.len() > 64 || !domain_ok {
        return Err(format!("Niepoprawny adres e-mail: {a}"));
    }
    Ok(a.to_string())
}

/// Rozbija tekst wpisany przez użytkownika ("a@x.pl, Jan <b@y.pl>; c@z.pl") na listę adresów.
/// Nazwy wyświetlane są pomijane – do nagłówka trafia sam adres.
pub fn parse_recipients(input: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for part in input.split(|c| c == ',' || c == ';' || c == '\n' || c == '\r') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let candidate = match (p.rfind('<'), p.rfind('>')) {
            (Some(l), Some(r)) if l < r => &p[l + 1..r],
            _ => p,
        };
        let a = validate_address(candidate)?;
        if !out.iter().any(|x| x.eq_ignore_ascii_case(&a)) {
            out.push(a);
        }
    }
    Ok(out)
}

/// Identyfikator wiadomości w nagłówku musi mieć postać <...> bez białych znaków.
fn message_id_list(s: &str) -> Option<String> {
    let ids: Vec<&str> = s
        .split_whitespace()
        .filter(|t| t.starts_with('<') && t.ends_with('>') && t.len() > 2 && !t.chars().any(|c| c.is_control()))
        .collect();
    if ids.is_empty() { None } else { Some(ids.join(" ")) }
}

fn safe_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_control() || c == '"' || c == ';' { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_string();
    if cleaned.is_empty() { "zalacznik".to_string() } else { cleaned.chars().take(120).collect() }
}

fn safe_mime(m: &str) -> String {
    let ok = !m.is_empty()
        && m.len() <= 100
        && m.matches('/').count() == 1
        && m.chars().all(|c| c.is_ascii_alphanumeric() || "/.+-_".contains(c));
    if ok { m.to_ascii_lowercase() } else { "application/octet-stream".to_string() }
}

// ---------- Budowa wiadomości ----------

fn text_part(content_type: &str, body: &str) -> String {
    format!(
        "Content-Type: {content_type}; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}",
        base64_wrapped(body.as_bytes())
    )
}

pub fn build_message(m: &Outgoing, nonce: &str) -> Result<Vec<u8>, String> {
    let total_rcpt = m.to.len() + m.cc.len() + m.bcc.len();
    if total_rcpt > MAX_RECIPIENTS {
        return Err(format!("Zbyt wielu odbiorców (maksymalnie {MAX_RECIPIENTS})."));
    }
    let size: usize = m.attachments.iter().map(|a| a.data.len()).sum();
    if size > MAX_ATTACHMENTS_BYTES {
        return Err("Załączniki są większe niż 20 MB.".to_string());
    }
    if m.subject.chars().count() > MAX_SUBJECT_CHARS {
        return Err("Temat jest zbyt długi.".to_string());
    }
    let nonce: String = nonce.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if nonce.len() < 8 {
        return Err("Wewnętrzny błąd: zbyt krótki identyfikator granicy MIME.".to_string());
    }

    let mut h = String::new();
    h.push_str(&format!("From: {}\r\n", clean_header_value(&m.from)));
    if !m.to.is_empty() {
        h.push_str(&format!("To: {}\r\n", m.to.join(", ")));
    }
    if !m.cc.is_empty() {
        h.push_str(&format!("Cc: {}\r\n", m.cc.join(", ")));
    }
    if !m.bcc.is_empty() {
        h.push_str(&format!("Bcc: {}\r\n", m.bcc.join(", ")));
    }
    h.push_str(&format!("Subject: {}\r\n", encode_header_text(&m.subject)));
    if let Some(v) = m.in_reply_to.as_deref().and_then(message_id_list) {
        // In-Reply-To wskazuje jedną wiadomość – bierzemy ostatni identyfikator.
        let last = v.split(' ').last().unwrap_or("").to_string();
        h.push_str(&format!("In-Reply-To: {last}\r\n"));
    }
    if let Some(v) = m.references.as_deref().and_then(message_id_list) {
        h.push_str(&format!("References: {v}\r\n"));
    }
    h.push_str("MIME-Version: 1.0\r\nX-Mailer: RustMail\r\n");

    let alt_b = format!("rm_alt_{nonce}");
    let alternative = format!(
        "--{alt_b}\r\n{}--{alt_b}\r\n{}--{alt_b}--\r\n",
        text_part("text/plain", &m.text),
        text_part("text/html", &m.html),
    );

    let mut out: Vec<u8> = Vec::new();
    if m.attachments.is_empty() {
        h.push_str(&format!("Content-Type: multipart/alternative; boundary=\"{alt_b}\"\r\n\r\n"));
        out.extend_from_slice(h.as_bytes());
        out.extend_from_slice(alternative.as_bytes());
        return Ok(out);
    }

    let mix_b = format!("rm_mix_{nonce}");
    h.push_str(&format!("Content-Type: multipart/mixed; boundary=\"{mix_b}\"\r\n\r\n"));
    out.extend_from_slice(h.as_bytes());
    out.extend_from_slice(
        format!("--{mix_b}\r\nContent-Type: multipart/alternative; boundary=\"{alt_b}\"\r\n\r\n{alternative}").as_bytes(),
    );
    for a in &m.attachments {
        let name = encode_header_text(&safe_filename(&a.name));
        let mime = safe_mime(&a.mime);
        out.extend_from_slice(
            format!(
                "--{mix_b}\r\nContent-Type: {mime}; name=\"{name}\"\r\nContent-Disposition: attachment; filename=\"{name}\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{}",
                base64_wrapped(&a.data)
            )
            .as_bytes(),
        );
    }
    out.extend_from_slice(format!("--{mix_b}--\r\n").as_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Outgoing {
        Outgoing {
            from: "ja@example.com".into(),
            to: vec!["anna@example.com".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Test".into(),
            text: "Witaj".into(),
            html: "<p>Witaj</p>".into(),
            in_reply_to: None,
            references: None,
            attachments: vec![],
        }
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn wrapped_base64_has_short_lines() {
        let w = base64_wrapped(&vec![7u8; 500]);
        assert!(w.split("\r\n").all(|l| l.len() <= 76));
    }

    #[test]
    fn header_values_never_contain_newlines() {
        assert_eq!(clean_header_value("Temat\r\nBcc: zly@x.pl"), "Temat  Bcc: zly@x.pl");
        let m = Outgoing { subject: "A\r\nBcc: zly@x.pl".into(), ..sample() };
        let msg = String::from_utf8(build_message(&m, "abcdefgh12").unwrap()).unwrap();
        let head = msg.split("\r\n\r\n").next().unwrap();
        assert!(!head.lines().any(|l| l.starts_with("Bcc:")));
    }

    #[test]
    fn non_ascii_subject_is_encoded() {
        let e = encode_header_text("Zażółć gęślą jaźń – bardzo długi temat wiadomości do kogoś");
        assert!(e.starts_with("=?UTF-8?B?"));
        assert!(e.is_ascii());
        assert!(e.split("\r\n ").all(|w| w.len() <= 75));
        assert_eq!(encode_header_text("Hello"), "Hello");
    }

    #[test]
    fn address_validation() {
        assert!(validate_address("anna@example.com").is_ok());
        assert!(validate_address("a.b+tag@sub.example.pl").is_ok());
        for bad in ["", "bez-malpy", "a@b", "a b@x.pl", "a@x.pl\r\nBcc:z@z.pl", "<a@x.pl>", "a@@x.pl", "@x.pl", "a@.x.pl", "a@x..pl", "a\"b@x.pl"] {
            assert!(validate_address(bad).is_err(), "powinno odrzucić: {bad:?}");
        }
    }

    #[test]
    fn recipients_are_split_deduplicated_and_validated() {
        let r = parse_recipients("a@x.pl, Jan Kowalski <b@y.pl>; A@X.PL\nc@z.pl").unwrap();
        assert_eq!(r, vec!["a@x.pl", "b@y.pl", "c@z.pl"]);
        assert!(parse_recipients("a@x.pl, zly").is_err());
        assert!(parse_recipients("  ,; ").unwrap().is_empty());
    }

    #[test]
    fn plain_message_is_multipart_alternative() {
        let msg = String::from_utf8(build_message(&sample(), "abcdefgh12").unwrap()).unwrap();
        assert!(msg.contains("To: anna@example.com\r\n"));
        assert!(msg.contains("multipart/alternative; boundary=\"rm_alt_abcdefgh12\""));
        assert!(msg.contains("text/plain; charset=UTF-8"));
        assert!(msg.contains("text/html; charset=UTF-8"));
        assert!(msg.contains(&base64_encode(b"Witaj")));
        assert!(msg.trim_end().ends_with("--rm_alt_abcdefgh12--"));
        assert!(!msg.contains("multipart/mixed"));
    }

    #[test]
    fn attachments_use_mixed_and_sanitized_names() {
        let m = Outgoing {
            attachments: vec![OutAttachment {
                name: "../../etc/pa\"ss;wd.txt".into(),
                mime: "text/plain\r\nX: y".into(),
                data: b"dane".to_vec(),
            }],
            ..sample()
        };
        let msg = String::from_utf8(build_message(&m, "abcdefgh12").unwrap()).unwrap();
        assert!(msg.contains("multipart/mixed; boundary=\"rm_mix_abcdefgh12\""));
        assert!(msg.contains("Content-Type: application/octet-stream; name=\"pa_ss_wd.txt\""));
        assert!(msg.contains("filename=\"pa_ss_wd.txt\""));
        assert!(msg.contains(&base64_encode(b"dane")));
        assert!(msg.trim_end().ends_with("--rm_mix_abcdefgh12--"));
    }

    #[test]
    fn reply_headers_are_validated() {
        let m = Outgoing {
            in_reply_to: Some("<a@x> <b@x>".into()),
            references: Some("<a@x> <b@x> smiec".into()),
            ..sample()
        };
        let msg = String::from_utf8(build_message(&m, "abcdefgh12").unwrap()).unwrap();
        assert!(msg.contains("In-Reply-To: <b@x>\r\n"));
        assert!(msg.contains("References: <a@x> <b@x>\r\n"));
        let m2 = Outgoing { in_reply_to: Some("bez nawiasow".into()), ..sample() };
        let msg2 = String::from_utf8(build_message(&m2, "abcdefgh12").unwrap()).unwrap();
        assert!(!msg2.contains("In-Reply-To"));
    }

    #[test]
    fn limits_are_enforced() {
        let big = Outgoing {
            attachments: vec![OutAttachment { name: "a".into(), mime: "a/b".into(), data: vec![0; MAX_ATTACHMENTS_BYTES + 1] }],
            ..sample()
        };
        assert!(build_message(&big, "abcdefgh12").is_err());
        let many = Outgoing { to: (0..101).map(|i| format!("u{i}@x.pl")).collect(), ..sample() };
        assert!(build_message(&many, "abcdefgh12").is_err());
        assert!(build_message(&sample(), "ab").is_err());
        let long = Outgoing { subject: "x".repeat(301), ..sample() };
        assert!(build_message(&long, "abcdefgh12").is_err());
    }

    #[test]
    fn bcc_is_only_in_header_when_present() {
        let m = Outgoing { bcc: vec!["ukryty@x.pl".into()], ..sample() };
        let msg = String::from_utf8(build_message(&m, "abcdefgh12").unwrap()).unwrap();
        assert!(msg.contains("Bcc: ukryty@x.pl\r\n"));
        let msg2 = String::from_utf8(build_message(&sample(), "abcdefgh12").unwrap()).unwrap();
        assert!(!msg2.contains("Bcc:"));
    }
}

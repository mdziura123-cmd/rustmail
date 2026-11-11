//! Książka adresów: adresy, z którymi użytkownik pisał. Zapisywana lokalnie w zaszyfrowanej bazie
//! (nie jest wysyłana nigdzie ani synchronizowana). Ten moduł to czysta logika: rozbiór pola adresów i walidacja.

pub const MAX_CONTACTS: i64 = 3000;
pub const MAX_PER_CALL: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Addr {
    pub email: String,
    pub name: String,
}

/// Prosta, ostrożna walidacja adresu: jedno „@”, kropka w domenie, brak spacji i znaków specjalnych.
pub fn valid_email(e: &str) -> bool {
    if e.len() < 5 || e.len() > 254 {
        return false;
    }
    let Some((local, domain)) = e.split_once('@') else { return false };
    if local.is_empty() || local.len() > 64 || domain.contains('@') || !domain.contains('.') {
        return false;
    }
    if domain.starts_with(['.', '-']) || domain.ends_with(['.', '-']) || domain.contains("..") {
        return false;
    }
    !e.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | ',' | ';' | '"' | '\'' | '(' | ')' | '[' | ']' | '\\'))
}

fn clean_name(n: &str) -> String {
    let t: String = n.chars().filter(|c| !c.is_control() && *c != '<' && *c != '>').collect();
    t.trim().trim_matches('"').trim().chars().take(100).collect()
}

/// Dzieli pole adresów po przecinkach i średnikach, pomijając te w cudzysłowach i w <…>.
fn split_list(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut in_q, mut in_a) = (false, false);
    for c in s.chars() {
        match c {
            '"' => { in_q = !in_q; cur.push(c); }
            '<' if !in_q => { in_a = true; cur.push(c); }
            '>' if !in_q => { in_a = false; cur.push(c); }
            ',' | ';' if !in_q && !in_a => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Wyciąga poprawne adresy z pola typu `Jan Kowalski <jan@x.pl>, ala@y.pl`. Powtórzenia są pomijane.
pub fn parse_list(s: &str) -> Vec<Addr> {
    let mut out: Vec<Addr> = Vec::new();
    for part in split_list(s).into_iter().take(MAX_PER_CALL) {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let (name, email) = match (p.rfind('<'), p.rfind('>')) {
            (Some(a), Some(b)) if a < b => (clean_name(&p[..a]), p[a + 1..b].trim().to_lowercase()),
            _ => (String::new(), p.to_lowercase()),
        };
        if valid_email(&email) && !out.iter().any(|x| x.email == email) {
            out.push(Addr { email, name });
        }
    }
    out
}

/// Adresy, których nie warto podpowiadać: automaty i powiadomienia (noreply, mailer-daemon itp.).
pub fn is_automatic(email: &str) -> bool {
    let local = email.split('@').next().unwrap_or("").to_lowercase();
    ["noreply", "no-reply", "no_reply", "donotreply", "do-not-reply", "mailer-daemon", "postmaster", "bounce", "notification"]
        .iter()
        .any(|p| local.contains(p))
}

/// Zapytanie do LIKE: znaki specjalne są poprzedzone znakiem ucieczki `\`.
pub fn like_escape(q: &str) -> String {
    let mut o = String::with_capacity(q.len());
    for c in q.chars() {
        if matches!(c, '\\' | '%' | '_') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(email: &str, name: &str) -> Addr { Addr { email: email.into(), name: name.into() } }

    #[test]
    fn parses_lists() {
        assert_eq!(parse_list("ala@x.pl"), vec![a("ala@x.pl", "")]);
        assert_eq!(parse_list("Jan Kowalski <JAN@X.pl>, ola@y.com; \"Nowak, Piotr\" <p@z.pl>"),
            vec![a("jan@x.pl", "Jan Kowalski"), a("ola@y.com", ""), a("p@z.pl", "Nowak, Piotr")]);
        assert_eq!(parse_list("a@b.pl, a@b.pl"), vec![a("a@b.pl", "")]);
    }

    #[test]
    fn rejects_bad() {
        assert!(parse_list("").is_empty());
        assert!(parse_list("nie-adres, @x.pl, a@b, a b@c.pl, a@@b.pl").is_empty());
        assert!(!valid_email("a@b..pl"));
        assert!(!valid_email("<a@b.pl>"));
        assert!(!valid_email(&format!("{}@b.pl", "x".repeat(70))));
    }

    #[test]
    fn automatic_addresses() {
        assert!(is_automatic("noreply@x.pl"));
        assert!(is_automatic("no-reply@x.pl"));
        assert!(is_automatic("MAILER-DAEMON@x.pl"));
        assert!(!is_automatic("jan.kowalski@x.pl"));
    }

    #[test]
    fn like_escaping() {
        assert_eq!(like_escape("a%b_c\\"), "a\\%b\\_c\\\\");
    }
}

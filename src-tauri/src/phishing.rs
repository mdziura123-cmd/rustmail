//! Lokalna ocena ryzyka phishingu. Wyłącznie biblioteka standardowa, żadnych połączeń sieciowych:
//! treść poczty nigdy nie opuszcza komputera. To zestaw reguł (heurystyk), a nie wyrocznia –
//! każda ocena ma wypisane powody, a użytkownik zawsze może uznać wiadomość za bezpieczną.

pub const WARN_AT: u32 = 3;
pub const HIGH_AT: u32 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Level {
    None,
    Warn,
    High,
}

impl Level {
    pub fn as_str(&self) -> &'static str {
        match self {
            Level::None => "none",
            Level::Warn => "warn",
            Level::High => "high",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assessment {
    pub level: Level,
    pub score: u32,
    pub reasons: Vec<String>,
}

impl Assessment {
    pub fn none() -> Self {
        Assessment { level: Level::None, score: 0, reasons: vec![] }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct AuthResults {
    pub spf: Option<String>,
    pub dkim: Option<String>,
    pub dmarc: Option<String>,
}

#[derive(Default)]
pub struct Signals<'a> {
    pub from_name: &'a str,
    pub from_email: &'a str,
    pub reply_to: &'a str,
    pub subject: &'a str,
    pub auth: AuthResults,
    /// Treść tekstowa (może być pusta przy ocenie samych nagłówków).
    pub body: &'a str,
    /// Surowy HTML (może być pusty) – służy do wyciągnięcia linków.
    pub html: &'a str,
    pub attachment_names: Vec<String>,
    /// Wiadomość wysłana przez użytkownika – nie oceniamy.
    pub mine: bool,
    /// Nadawca jest na liście zablokowanych użytkownika.
    pub blocked: bool,
    /// Nadawca jest na liście zaufanych użytkownika (działa tylko, gdy Gmail potwierdza DMARC=pass,
    /// bo sam adres nadawcy można podrobić).
    pub allowed: bool,
}

// ---------- Wyniki uwierzytelniania (Gmail: mx.google.com) ----------

/// Odczytuje SPF/DKIM/DMARC wyłącznie z nagłówków dodanych przez Gmail (authserv-id `mx.google.com`).
/// Nagłówki Authentication-Results podstawione przez nadawcę mają inny identyfikator i są ignorowane.
pub fn parse_auth_results(values: &[&str]) -> AuthResults {
    let mut out = AuthResults::default();
    for v in values {
        let trimmed = v.trim_start();
        let first = trimmed.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
        if first != "mx.google.com" && !first.starts_with("mx.google.com ") {
            continue;
        }
        for part in trimmed.split(';').skip(1) {
            let p = part.trim();
            for (key, slot) in [("spf=", &mut out.spf), ("dkim=", &mut out.dkim), ("dmarc=", &mut out.dmarc)] {
                if let Some(rest) = p.strip_prefix(key) {
                    let word: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_lowercase();
                    if word.is_empty() {
                        continue;
                    }
                    // przy wielu podpisach DKIM „pass” wygrywa
                    let better = match slot.as_deref() {
                        None => true,
                        Some("pass") => false,
                        Some(_) => word == "pass",
                    };
                    if better {
                        *slot = Some(word);
                    }
                }
            }
        }
    }
    out
}

// ---------- Domeny ----------

pub fn domain_of(email: &str) -> String {
    email.rsplit('@').next().unwrap_or("").trim().trim_matches(['<', '>']).to_ascii_lowercase()
}

const SECOND_LEVEL: &[&str] = &["co", "com", "org", "net", "gov", "edu", "ac"];

/// Domena „rejestrowalna” (przybliżenie): dwa ostatnie człony, a przy co.uk i podobnych – trzy.
pub fn registrable(host: &str) -> String {
    let parts: Vec<&str> = host.split('.').filter(|p| !p.is_empty()).collect();
    if parts.len() <= 2 {
        return parts.join(".");
    }
    let tld = parts[parts.len() - 1];
    let sld = parts[parts.len() - 2];
    let n = if tld.len() == 2 && SECOND_LEVEL.contains(&sld) { 3 } else { 2 };
    parts[parts.len() - n..].join(".")
}

fn is_ip_host(host: &str) -> bool {
    let v4 = host.split('.').count() == 4 && host.split('.').all(|p| !p.is_empty() && p.len() <= 3 && p.chars().all(|c| c.is_ascii_digit()));
    v4 || host.contains(':')
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur.push((prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Zamienia znaki często używane do udawania liter (0→o, 1→l, rn→m, vv→w, 5→s, 3→e).
fn homoglyph_normalize(s: &str) -> String {
    s.to_ascii_lowercase()
        .replace("rn", "m")
        .replace("vv", "w")
        .chars()
        .map(|c| match c {
            '0' => 'o',
            '1' | '!' | '|' => 'l',
            '5' => 's',
            '3' => 'e',
            _ => c,
        })
        .collect()
}

// ---------- Marki często podszywane ----------

struct Brand {
    /// Słowa, których użycie w nazwie nadawcy sugeruje podszywanie się (małe litery).
    keywords: &'static [&'static str],
    /// Domeny rejestrowalne uznawane za prawdziwe.
    domains: &'static [&'static str],
}

const BRANDS: &[Brand] = &[
    Brand { keywords: &["paypal"], domains: &["paypal.com", "paypal.pl", "paypal.eu"] },
    Brand { keywords: &["amazon"], domains: &["amazon.com", "amazon.pl", "amazon.de", "amazon.co.uk", "amazonaws.com", "amazon.es", "amazon.fr", "amazon.it"] },
    Brand { keywords: &["apple", "icloud"], domains: &["apple.com", "icloud.com", "me.com"] },
    Brand { keywords: &["microsoft", "outlook", "office365", "onedrive"], domains: &["microsoft.com", "outlook.com", "live.com", "office.com", "microsoftonline.com", "office365.com", "hotmail.com", "sharepoint.com"] },
    Brand { keywords: &["google", "gmail"], domains: &["google.com", "gmail.com", "youtube.com", "googlemail.com", "accounts.google.com"] },
    Brand { keywords: &["netflix"], domains: &["netflix.com"] },
    Brand { keywords: &["facebook", "instagram", "whatsapp"], domains: &["facebook.com", "facebookmail.com", "instagram.com", "meta.com", "whatsapp.com", "fb.com"] },
    Brand { keywords: &["allegro"], domains: &["allegro.pl", "allegro.com", "allegrolokalnie.pl"] },
    Brand { keywords: &["olx"], domains: &["olx.pl", "olx.com"] },
    Brand { keywords: &["inpost"], domains: &["inpost.pl", "inpost.eu"] },
    Brand { keywords: &["dhl"], domains: &["dhl.com", "dhl.de", "dhl.pl"] },
    Brand { keywords: &["dpd"], domains: &["dpd.com", "dpd.pl", "dpd.de"] },
    Brand { keywords: &["fedex"], domains: &["fedex.com"] },
    Brand { keywords: &["ups"], domains: &["ups.com"] },
    Brand { keywords: &["poczta polska"], domains: &["poczta-polska.pl", "pocztex.pl"] },
    Brand { keywords: &["pko"], domains: &["pkobp.pl", "pko.pl", "ipko.pl", "pkobp.com"] },
    Brand { keywords: &["mbank"], domains: &["mbank.pl", "mbank.com"] },
    Brand { keywords: &["santander"], domains: &["santander.pl", "santander.com", "santanderconsumer.pl"] },
    Brand { keywords: &["millennium"], domains: &["bankmillennium.pl"] },
    Brand { keywords: &["bnp paribas"], domains: &["bnpparibas.pl", "bnpparibas.com"] },
    Brand { keywords: &["ing bank", "ing "], domains: &["ing.pl", "ing.com", "ingbank.pl"] },
    Brand { keywords: &["revolut"], domains: &["revolut.com"] },
    Brand { keywords: &["blik"], domains: &["blik.pl"] },
    Brand { keywords: &["zus"], domains: &["zus.pl", "pue.zus.pl"] },
    Brand { keywords: &["urząd skarbowy", "krajowa administracja skarbowa", "e-urząd"], domains: &["gov.pl", "podatki.gov.pl", "mf.gov.pl"] },
    Brand { keywords: &["steam"], domains: &["steampowered.com", "steamcommunity.com", "valvesoftware.com"] },
    Brand { keywords: &["linkedin"], domains: &["linkedin.com"] },
    Brand { keywords: &["twitter"], domains: &["twitter.com", "x.com"] },
    Brand { keywords: &["dropbox"], domains: &["dropbox.com", "dropboxmail.com"] },
    Brand { keywords: &["docusign"], domains: &["docusign.com", "docusign.net"] },
    Brand { keywords: &["aliexpress"], domains: &["aliexpress.com", "alibaba.com", "aliexpress.ru"] },
];

fn word_in(haystack: &str, kw: &str) -> bool {
    // słowo kluczowe jako osobny wyraz (żeby „ups” nie pasowało do „groups”)
    let mut start = 0;
    while let Some(i) = haystack[start..].find(kw) {
        let a = start + i;
        let b = a + kw.len();
        let before_ok = a == 0 || !haystack[..a].chars().next_back().map_or(false, |c| c.is_alphanumeric());
        let after_ok = kw.ends_with(' ') || b >= haystack.len() || !haystack[b..].chars().next().map_or(false, |c| c.is_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        start = b;
    }
    false
}

fn is_official(brand: &Brand, reg: &str) -> bool {
    brand.domains.iter().any(|d| registrable(d) == reg)
}

// ---------- Linki ----------

/// Wyciąga (adres, widoczny tekst) ze wszystkich <a href=...> w HTML. Proste, odporne na błędy skanowanie.
pub fn extract_links(html: &str) -> Vec<(String, String)> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(rel) = lower[pos..].find("<a") {
        let start = pos + rel;
        let after = start + 2;
        let ok = lower[after..].chars().next().map_or(false, |c| c.is_whitespace());
        if !ok {
            pos = after;
            continue;
        }
        let Some(tag_end_rel) = lower[after..].find('>') else { break };
        let tag_end = after + tag_end_rel;
        let tag = &html[start..tag_end];
        let tag_l = &lower[start..tag_end];
        let href = attr_value(tag, tag_l, "href");
        let close = lower[tag_end..].find("</a").map(|i| tag_end + i).unwrap_or(html.len());
        let text = strip_tags(&html[tag_end + 1..close]);
        if let Some(h) = href {
            out.push((h, text));
        }
        pos = close.min(html.len());
        if out.len() >= 200 {
            break;
        }
    }
    out
}

fn attr_value(tag: &str, tag_l: &str, name: &str) -> Option<String> {
    let key = format!("{name}=");
    let i = tag_l.find(&key)?;
    let rest = &tag[i + key.len()..];
    let v = match rest.chars().next()? {
        q @ ('"' | '\'') => rest[1..].split(q).next()?.to_string(),
        _ => rest.split(|c: char| c.is_whitespace() || c == '>').next()?.to_string(),
    };
    Some(v.replace("&amp;", "&").trim().to_string())
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ").replace("&amp;", "&").trim().to_string()
}

struct ParsedUrl {
    host: String,
    userinfo: bool,
    https: bool,
}

fn parse_url(url: &str) -> Option<ParsedUrl> {
    let u = url.trim();
    let lower = u.to_ascii_lowercase();
    let (https, rest) = if let Some(r) = lower.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = lower.strip_prefix("http://") {
        (false, r)
    } else {
        return None;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let userinfo = authority.contains('@');
    let hostport = authority.rsplit('@').next().unwrap_or("");
    let host = match hostport.rfind(':') {
        Some(i) if hostport[i + 1..].chars().all(|c| c.is_ascii_digit()) && !hostport.ends_with(']') => &hostport[..i],
        _ => hostport,
    };
    let host = host.trim_matches(['[', ']']).to_string();
    if host.is_empty() { None } else { Some(ParsedUrl { host, userinfo, https }) }
}

/// Domena „widoczna” w tekście linku, np. „https://bank.pl/logowanie” albo „www.bank.pl”.
fn domain_in_text(text: &str) -> Option<String> {
    for token in text.split_whitespace() {
        let t = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '/' && c != ':');
        let t = t.trim_start_matches("https://").trim_start_matches("http://");
        let host = t.split(['/', '?', '#', ':']).next().unwrap_or("");
        let labels: Vec<&str> = host.split('.').collect();
        if labels.len() >= 2
            && labels.iter().all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            && labels.last().map_or(false, |l| l.len() >= 2 && l.chars().all(|c| c.is_ascii_alphabetic()))
        {
            return Some(host.to_ascii_lowercase());
        }
    }
    None
}

const SHORTENERS: &[&str] = &[
    "bit.ly", "tinyurl.com", "goo.gl", "is.gd", "cutt.ly", "rb.gy", "ow.ly", "tiny.cc", "shorturl.at", "t.ly", "buff.ly", "rebrand.ly",
];

// ---------- Słowa kluczowe ----------

const URGENCY: &[&str] = &[
    "pilne", "natychmiast", "ostatnie ostrzeżenie", "konto zostanie zablokowane", "konto zostanie zawieszone",
    "konto zostało zawieszone", "konto zostało zablokowane", "weryfikacja konta", "zweryfikuj konto", "w ciągu 24 godzin",
    "w ciągu 24 h", "urgent", "verify your account", "account suspended", "account will be suspended", "within 24 hours",
    "your account has been", "password expires", "action required", "wymagane działanie",
];

const CREDENTIALS: &[&str] = &[
    "podaj hasło", "wpisz hasło", "potwierdź hasło", "zaloguj się, aby", "zaloguj się aby", "dane karty", "numer karty",
    "kod blik", "kod sms", "kod weryfikacyjny", "dane logowania", "enter your password", "confirm your password",
    "credit card number", "card number", "security code", "verify your identity", "log in to confirm", "sign in to verify",
];

const HTML_ATTACH: &[&str] = &["html", "htm", "xhtml", "svg", "shtml", "mht", "mhtml"];
const ARCHIVE_ATTACH: &[&str] = &["zip", "rar", "7z", "gz", "tar", "ace", "cab"];

fn contains_any<'a>(text: &str, list: &[&'a str]) -> Vec<&'a str> {
    let t = text.to_lowercase();
    list.iter().copied().filter(|k| t.contains(k)).collect()
}

fn attachment_exts(name: &str) -> Vec<String> {
    name.to_ascii_lowercase().split('.').skip(1).map(|s| s.trim().to_string()).collect()
}

fn is_risky_ext(name: &str) -> bool {
    const RISKY: &[&str] = &[
        "exe", "msi", "bat", "cmd", "com", "scr", "pif", "vbs", "vbe", "js", "jse", "wsf", "wsh", "ps1", "psm1", "jar", "sh",
        "bash", "run", "bin", "appimage", "desktop", "app", "dmg", "pkg", "deb", "rpm", "apk", "lnk", "reg", "hta", "cpl",
        "dll", "iso", "img", "docm", "xlsm", "pptm", "dotm", "xlam", "py", "pl", "rb",
    ];
    attachment_exts(name).iter().any(|e| RISKY.contains(&e.as_str()))
}

// ---------- Ocena ----------

struct Findings {
    list: Vec<(u32, String)>,
}

impl Findings {
    fn add(&mut self, w: u32, text: impl Into<String>) {
        self.list.push((w, text.into()));
    }
}

pub fn assess(s: &Signals) -> Assessment {
    if s.mine {
        return Assessment::none();
    }
    let mut f = Findings { list: vec![] };
    let sender_domain = domain_of(s.from_email);
    let sender_reg = registrable(&sender_domain);
    let name_l = s.from_name.to_lowercase();

    if s.blocked {
        f.add(6, "Nadawca jest na Twojej liście zablokowanych.");
    }
    let skip_sender = s.allowed && !s.blocked && matches!(s.auth.dmarc.as_deref(), Some("pass"));

    if !skip_sender {
    // 1) Uwierzytelnianie nadawcy według Gmaila
    let bad = |v: &Option<String>| matches!(v.as_deref(), Some("fail" | "softfail" | "permerror"));
    if matches!(s.auth.dmarc.as_deref(), Some("fail")) {
        f.add(4, "Gmail: nadawca nie przeszedł weryfikacji DMARC – wiadomość mogła zostać podrobiona.");
    }
    if bad(&s.auth.spf) {
        f.add(2, "Gmail: serwer, z którego wysłano wiadomość, nie jest uprawniony do wysyłania poczty z tej domeny (SPF).");
    }
    if matches!(s.auth.dkim.as_deref(), Some("fail" | "permerror")) {
        f.add(2, "Gmail: podpis cyfrowy wiadomości (DKIM) jest nieprawidłowy.");
    }

    // 2) Nazwa wyświetlana zawiera inny adres e-mail
    if let Some(at) = name_l.find('@') {
        let dom: String = name_l[at + 1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
            .collect();
        if dom.contains('.') && registrable(&dom) != sender_reg {
            f.add(4, format!("Nazwa nadawcy zawiera adres „{dom}”, ale wiadomość przyszła z domeny „{sender_domain}”."));
        }
    }

    // 3) Podszywanie się pod znaną markę i domeny łudząco podobne
    let mut brand_impersonation = false;
    for b in BRANDS {
        let named = b.keywords.iter().any(|k| word_in(&name_l, k));
        if named && !is_official(b, &sender_reg) && !sender_reg.is_empty() {
            brand_impersonation = true;
            f.add(
                4,
                format!("Nazwa nadawcy sugeruje „{}”, ale adres pochodzi z domeny „{sender_domain}”, która do tej marki nie należy.", b.keywords[0]),
            );
            break;
        }
    }
    let label = sender_reg.split('.').next().unwrap_or("");
    'outer: for b in BRANDS {
        if is_official(b, &sender_reg) {
            continue;
        }
        for d in b.domains {
            let official_label = registrable(d).split('.').next().unwrap_or("").to_string();
            if official_label.len() < 4 || label == official_label {
                continue;
            }
            let hom = homoglyph_normalize(label) == official_label;
            let near = official_label.len() >= 6 && levenshtein(label, &official_label) == 1;
            let norm = homoglyph_normalize(label);
            let embedded = norm.len() > official_label.len() && norm.contains(&official_label) && (norm.contains('-') || official_label.len() >= 6);
            if hom || near || embedded {
                f.add(5, format!("Domena nadawcy „{sender_reg}” łudząco przypomina „{}” (podróbka).", registrable(d)));
                break 'outer;
            }
        }
    }
    if sender_domain.split('.').any(|l| l.starts_with("xn--")) || !sender_domain.is_ascii() {
        f.add(3, "Domena nadawcy zawiera znaki międzynarodowe, które mogą udawać zwykłe litery.");
    }

    // 4) Adres do odpowiedzi
    let rt = domain_of(s.reply_to);
    if !s.reply_to.trim().is_empty() && !rt.is_empty() && registrable(&rt) != sender_reg {
        let w = if brand_impersonation { 2 } else { 1 };
        f.add(w, format!("Odpowiedź trafi na inny adres („{rt}”) niż adres nadawcy („{sender_domain}”)."));
    }

    }

    // 5) Temat i treść
    let subj_hits = contains_any(s.subject, URGENCY);
    if !subj_hits.is_empty() {
        f.add(subj_hits.len().min(2) as u32, "Temat wywiera presję czasu lub grozi zablokowaniem konta.");
    }
    if !s.body.is_empty() {
        if !contains_any(s.body, CREDENTIALS).is_empty() {
            f.add(2, "Treść prosi o hasło, kod lub dane karty – prawdziwe firmy tego nie robią e-mailem.");
        }
        if !contains_any(s.body, URGENCY).is_empty() && subj_hits.is_empty() {
            f.add(1, "Treść wywiera presję czasu lub grozi zablokowaniem konta.");
        }
    }

    // 6) Linki
    if !s.html.is_empty() {
        let mut link_pts = 0u32;
        let mut seen: Vec<String> = Vec::new();
        for (href, text) in extract_links(s.html) {
            let Some(p) = parse_url(&href) else { continue };
            let mut local: Vec<(u32, String)> = vec![];
            if p.userinfo {
                local.push((4, format!("Link zawiera dane przed znakiem @, co ukrywa prawdziwą stronę („{}”).", p.host)));
            }
            if is_ip_host(&p.host) {
                local.push((2, format!("Link prowadzi do adresu IP („{}”) zamiast do nazwy strony.", p.host)));
            }
            if p.host.split('.').any(|l| l.starts_with("xn--")) {
                local.push((2, format!("Link prowadzi do domeny ze znakami międzynarodowymi („{}”).", p.host)));
            }
            if SHORTENERS.iter().any(|d| registrable(&p.host) == *d) {
                local.push((1, "Wiadomość zawiera skrócony link, który ukrywa miejsce docelowe.".to_string()));
            }
            if !p.https {
                local.push((1, "Wiadomość zawiera link bez szyfrowania (http).".to_string()));
            }
            if let Some(shown) = domain_in_text(&text) {
                if registrable(&shown) != registrable(&p.host) {
                    local.push((3, format!("Tekst linku sugeruje „{shown}”, ale link prowadzi do „{}”.", p.host)));
                }
            }
            for (w, t) in local {
                if !seen.contains(&t) && link_pts < 6 {
                    link_pts += w;
                    seen.push(t.clone());
                    f.add(w, t);
                }
            }
        }
    }

    // 7) Załączniki
    for name in &s.attachment_names {
        let exts = attachment_exts(name);
        if is_risky_ext(name) {
            f.add(3, format!("Załącznik „{name}” ma typ pliku, który może uruchomić szkodliwy kod."));
        } else if exts.iter().any(|e| HTML_ATTACH.contains(&e.as_str())) {
            f.add(3, format!("Załącznik „{name}” to strona HTML – często służy do wyłudzania haseł."));
        } else if exts.last().map_or(false, |e| ARCHIVE_ATTACH.contains(&e.as_str())) {
            f.add(1, format!("Załącznik „{name}” to archiwum, które może ukrywać niebezpieczne pliki."));
        }
    }

    f.list.sort_by(|a, b| b.0.cmp(&a.0));
    let score: u32 = f.list.iter().map(|(w, _)| *w).sum();
    let level = if score >= HIGH_AT {
        Level::High
    } else if score >= WARN_AT {
        Level::Warn
    } else {
        Level::None
    };
    let reasons = if level == Level::None { vec![] } else { f.list.into_iter().map(|(_, t)| t).take(8).collect() };
    Assessment { level, score, reasons }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD_AUTH: &str = "mx.google.com; dkim=pass header.i=@x.com; spf=pass smtp.mailfrom=a@x.com; dmarc=pass (p=NONE) header.from=x.com";

    fn base<'a>() -> Signals<'a> {
        Signals { from_name: "Anna", from_email: "anna@example.com", auth: parse_auth_results(&[GOOD_AUTH]), ..Default::default() }
    }

    #[test]
    fn parses_only_gmail_auth_results() {
        let a = parse_auth_results(&[GOOD_AUTH]);
        assert_eq!(a.spf.as_deref(), Some("pass"));
        assert_eq!(a.dkim.as_deref(), Some("pass"));
        assert_eq!(a.dmarc.as_deref(), Some("pass"));
        // podstawiony przez nadawcę nagłówek z innym identyfikatorem jest ignorowany
        let fake = parse_auth_results(&["evil.example.com; spf=pass; dkim=pass; dmarc=pass"]);
        assert_eq!(fake, AuthResults::default());
        let real_and_fake = parse_auth_results(&[
            "evil.example.com; dmarc=pass",
            "mx.google.com; spf=fail smtp.mailfrom=x; dmarc=fail (p=REJECT)",
        ]);
        assert_eq!(real_and_fake.dmarc.as_deref(), Some("fail"));
        assert_eq!(real_and_fake.spf.as_deref(), Some("fail"));
        let multi = parse_auth_results(&["mx.google.com; dkim=fail header.i=@a; dkim=pass header.i=@b"]);
        assert_eq!(multi.dkim.as_deref(), Some("pass"));
    }

    #[test]
    fn ordinary_mail_is_clean() {
        let a = assess(&base());
        assert_eq!(a.level, Level::None);
        assert!(a.reasons.is_empty());
    }

    #[test]
    fn own_mail_is_never_assessed() {
        let s = Signals { mine: true, from_name: "PayPal", from_email: "x@paypa1.ru", ..base() };
        assert_eq!(assess(&s).level, Level::None);
    }

    #[test]
    fn legit_brand_mail_is_not_flagged() {
        for (n, e) in [
            ("AliExpress", "no-reply@notice.aliexpress.com"),
            ("PayPal", "service@paypal.com"),
            ("Amazon Web Services", "no-reply@amazonaws.com"),
            ("mBank", "info@mbank.pl"),
            ("Microsoft account team", "account-security-noreply@accountprotection.microsoft.com"),
            ("Groups Admin", "a@groups.io"),
            ("GitHub", "notifications@github.com"),
            ("OpenAI", "noreply@tm.openai.com"),
        ] {
            let a = assess(&Signals { from_name: n, from_email: e, ..base() });
            assert_eq!(a.level, Level::None, "fałszywy alarm dla {n} <{e}>: {:?}", a.reasons);
        }
    }

    #[test]
    fn classic_paypal_phish_is_high() {
        let html = r#"<p>Twoje konto zostanie zablokowane w ciągu 24 godzin.</p><a href="http://paypa1-secure.ru/login">https://www.paypal.com/verify</a>"#;
        let s = Signals {
            from_name: "PayPal Service",
            from_email: "support@paypa1-secure.ru",
            subject: "Pilne: weryfikacja konta",
            auth: parse_auth_results(&["mx.google.com; spf=fail; dkim=none; dmarc=fail"]),
            body: "Twoje konto zostanie zablokowane. Podaj hasło, aby potwierdzić.",
            html,
            ..Default::default()
        };
        let a = assess(&s);
        assert_eq!(a.level, Level::High);
        assert!(a.score >= 12);
        assert!(a.reasons.iter().any(|r| r.contains("DMARC")));
        assert!(a.reasons.iter().any(|r| r.contains("Tekst linku sugeruje")));
        assert!(a.reasons.iter().any(|r| r.contains("podróbka")));
    }

    #[test]
    fn display_name_with_foreign_address_is_flagged() {
        let s = Signals { from_name: "support@firma.pl", from_email: "x@mail.evil.ru", ..base() };
        let a = assess(&s);
        assert!(a.reasons.iter().any(|r| r.contains("zawiera adres")));
        assert_eq!(a.level, Level::Warn);
    }

    #[test]
    fn lookalike_domains_are_detected() {
        for dom in ["paypa1.com", "arnazon.com", "micr0soft.com", "netfliix.com", "allegro-pl.com", "mbank-logowanie.pl", "g00gle.com"] {
            let email = format!("a@{dom}");
            let a = assess(&Signals { from_name: "Biuro", from_email: &email, ..base() });
            assert!(a.reasons.iter().any(|r| r.contains("podróbka")), "nie wykryto {dom}: {:?}", a.reasons);
        }
        for dom in ["paypal.com", "amazon.pl", "example.com", "pay.pl", "applepie.org"] {
            let email = format!("a@{dom}");
            let a = assess(&Signals { from_name: "Biuro", from_email: &email, ..base() });
            assert!(!a.reasons.iter().any(|r| r.contains("podróbka")), "fałszywy alarm dla {dom}");
        }
    }

    #[test]
    fn auth_failures_alone_warn_but_do_not_reach_high() {
        let s = Signals { auth: parse_auth_results(&["mx.google.com; dmarc=fail"]), ..base() };
        assert_eq!(assess(&s).level, Level::Warn);
        let all = Signals { auth: parse_auth_results(&["mx.google.com; spf=softfail; dkim=fail; dmarc=fail"]), ..base() };
        assert_eq!(assess(&all).level, Level::High);
    }

    #[test]
    fn missing_auth_header_gives_no_penalty() {
        let s = Signals { auth: AuthResults::default(), ..base() };
        assert_eq!(assess(&s).level, Level::None);
    }

    #[test]
    fn reply_to_mismatch_alone_is_not_enough() {
        let s = Signals { reply_to: "kontakt@inna-domena.pl", ..base() };
        assert_eq!(assess(&s).level, Level::None);
    }

    #[test]
    fn link_tricks_are_detected() {
        let html = r#"<a href="https://login.bank.pl@evil.ru/x">Zaloguj</a> <a href="http://192.168.1.5/a">tu</a> <a href="https://bit.ly/abc">https://bank.pl</a>"#;
        let a = assess(&Signals { html, ..base() });
        assert_eq!(a.level, Level::High);
        assert!(a.reasons.iter().any(|r| r.contains("@")));
        assert!(a.reasons.iter().any(|r| r.contains("adresu IP")));
        let short = assess(&Signals { html: r#"<a href="https://bit.ly/abc">https://bank.pl</a>"#, ..base() });
        assert_eq!(short.level, Level::Warn);
        assert!(short.reasons.iter().any(|r| r.contains("skrócony")));
    }

    #[test]
    fn normal_links_do_not_trigger() {
        let html = r#"<a href="https://www.sklep.pl/zamowienie/123?x=1&amp;y=2">Zobacz zamówienie</a> <a href="https://sklep.pl">www.sklep.pl</a> <a href="https://sub.sklep.pl/a">sklep.pl/a</a>"#;
        assert_eq!(assess(&Signals { html, ..base() }).level, Level::None);
    }

    #[test]
    fn dangerous_attachments_are_flagged() {
        let a = assess(&Signals { attachment_names: vec!["faktura.pdf.exe".into()], ..base() });
        assert_eq!(a.level, Level::Warn);
        let b = assess(&Signals { attachment_names: vec!["login.html".into(), "dane.exe".into()], ..base() });
        assert_eq!(b.level, Level::High);
        let c = assess(&Signals { attachment_names: vec!["umowa.pdf".into(), "zdjecia.zip".into()], ..base() });
        assert_eq!(c.level, Level::None);
    }

    #[test]
    fn urgency_in_subject_alone_is_harmless() {
        let s = Signals { subject: "Pilne: spotkanie jutro", ..base() };
        assert_eq!(assess(&s).level, Level::None);
    }

    #[test]
    fn extract_links_handles_messy_html() {
        let l = extract_links(r#"<A  HREF='https://a.pl/x?q=1&amp;z=2' class=x><b>Kliknij</b> tu</A><a name=x>b</a><abbr>x</abbr><a href=https://b.pl>B</a>"#);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0], ("https://a.pl/x?q=1&z=2".to_string(), "Kliknij tu".to_string()));
        assert_eq!(l[1].0, "https://b.pl");
    }

    #[test]
    fn blocked_sender_is_high() {
        let a = assess(&Signals { blocked: true, ..base() });
        assert_eq!(a.level, Level::High);
        assert!(a.reasons[0].contains("zablokowanych"));
        // własnych wiadomości nigdy nie oceniamy
        assert_eq!(assess(&Signals { blocked: true, mine: true, ..base() }).level, Level::None);
    }

    #[test]
    fn allowed_sender_is_trusted_only_when_dmarc_passes() {
        let phish = |auth: AuthResults, allowed: bool| Signals {
            from_name: "PayPal",
            from_email: "x@paypa1.ru",
            auth,
            allowed,
            ..Default::default()
        };
        let pass = parse_auth_results(&["mx.google.com; dmarc=pass"]);
        let fail = parse_auth_results(&["mx.google.com; dmarc=fail"]);
        assert_eq!(assess(&phish(pass.clone(), true)).level, Level::None);
        assert_eq!(assess(&phish(fail.clone(), true)).level, Level::High);
        assert_eq!(assess(&phish(pass, false)).level, Level::High);
        // zaufanie nie wycisza niebezpiecznych załączników
        let att = Signals { allowed: true, attachment_names: vec!["a.exe".into()], ..base() };
        assert_eq!(assess(&att).level, Level::Warn);
        // zablokowany wygrywa z zaufanym
        assert_eq!(assess(&Signals { allowed: true, blocked: true, ..base() }).level, Level::High);
    }

    #[test]
    fn registrable_and_levenshtein() {
        assert_eq!(registrable("a.b.example.com"), "example.com");
        assert_eq!(registrable("x.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(levenshtein("netflix", "netfliix"), 1);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(domain_of("Ala <a@B.PL>"), "b.pl");
    }
}

//! Silniki filtrowania spamu – czysta logika (bez sieci, bazy i Tauri), dlatego da się ją testować osobno.
//!
//! Silniki:
//!  • `headers` – wykorzystuje istniejącą ocenę nagłówków (SPF/DKIM/DMARC, podszywanie się, lista zablokowanych),
//!  • `rules`   – frazy typowe dla spamu (wbudowane + własne użytkownika),
//!  • `bayes`   – lekki filtr statystyczny uczący się z decyzji użytkownika,
//!  • `lists`   – otwarte listy złośliwych domen (nadawca i linki w wiadomości).
//!
//! Każdy silnik zwraca co najwyżej jeden „głos” z siłą 0–100. Tryb działania decyduje, co zrobić z głosami.

use std::collections::{HashMap, HashSet};

pub const E_HEADERS: &str = "headers";
pub const E_RULES: &str = "rules";
pub const E_BAYES: &str = "bayes";
pub const E_LISTS: &str = "lists";

/// Siła, od której pojedynczy głos wystarcza do przeniesienia w trybie „tylko pewne”.
pub const CERTAIN_STRENGTH: u8 = 95;

// ---------- Tryby i decyzje ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Off,
    /// Tylko oznacza podejrzane wiadomości, niczego nie przenosi.
    Flag,
    /// Przenosi, gdy zgadzają się co najmniej dwa silniki (lub trafienie o najwyższej pewności); resztę oznacza.
    Certain,
    /// Przenosi wszystko, co uzna choć jeden silnik.
    Aggressive,
}

impl Mode {
    pub fn parse(s: &str) -> Mode {
        match s {
            "flag" => Mode::Flag,
            "certain" => Mode::Certain,
            "aggressive" => Mode::Aggressive,
            _ => Mode::Off,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Flag => "flag",
            Mode::Certain => "certain",
            Mode::Aggressive => "aggressive",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vote {
    pub engine: &'static str,
    pub strength: u8,
    pub reason: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Keep,
    Flag,
    Move,
}

pub fn decide(mode: Mode, votes: &[Vote]) -> Decision {
    if votes.is_empty() || mode == Mode::Off {
        return Decision::Keep;
    }
    match mode {
        Mode::Off => Decision::Keep,
        Mode::Flag => Decision::Flag,
        Mode::Aggressive => Decision::Move,
        Mode::Certain => {
            let distinct: HashSet<&str> = votes.iter().map(|v| v.engine).collect();
            if distinct.len() >= 2 || votes.iter().any(|v| v.strength >= CERTAIN_STRENGTH) {
                Decision::Move
            } else {
                Decision::Flag
            }
        }
    }
}

/// Czy wątku nie wolno ruszać automatycznie (użytkownik już się nim zajął lub jest dla niego ważny).
pub fn is_protected(starred: bool, dismissed: bool, participants: &str, labels: &[String]) -> bool {
    if starred || dismissed {
        return true;
    }
    // „Ja” w uczestnikach = użytkownik brał udział w rozmowie.
    if participants.split(',').any(|p| p.trim() == "Ja") {
        return true;
    }
    labels.iter().any(|l| l == "STARRED" || l.starts_with("Label_"))
}

// ---------- Normalizacja tekstu ----------

/// Małe litery i bez polskich znaków diakrytycznych (wiele spamu pisze „wygrales” zamiast „wygrałeś”).
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'ą' => 'a',
            'ć' => 'c',
            'ę' => 'e',
            'ł' => 'l',
            'ń' => 'n',
            'ó' => 'o',
            'ś' => 's',
            'ź' | 'ż' => 'z',
            other => other,
        })
        .collect()
}

/// Czy fraza występuje jako osobne słowa (żeby „cash” nie pasowało do „cashew”).
fn contains_phrase(hay: &str, phrase: &str) -> bool {
    if phrase.is_empty() {
        return false;
    }
    let mut start = 0;
    while let Some(i) = hay[start..].find(phrase) {
        let a = start + i;
        let b = a + phrase.len();
        let before_ok = a == 0 || !hay[..a].chars().next_back().map_or(false, |c| c.is_alphanumeric());
        let after_ok = b >= hay.len() || !hay[b..].chars().next().map_or(false, |c| c.is_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        start = b;
    }
    false
}

// ---------- Silnik: reguły i słowa kluczowe ----------

/// Frazy, z których jedna wystarcza (bez polskich znaków, małymi literami).
pub const STRONG: &[&str] = &[
    "wygrales", "wygrana w loterii", "odbierz nagrode", "pozyczka bez bik", "bez weryfikacji bik", "kasyno online",
    "bonus bez depozytu", "viagra", "cialis", "tabletki na potencje", "szybki zarobek", "zarabiaj w domu",
    "inwestycja w bitcoin", "bez recepty", "rozmiar ma znaczenie",
    "you have won", "you've won", "claim your prize", "claim your reward", "online casino", "no deposit bonus",
    "buy viagra", "cheap cialis", "make money fast", "earn money from home", "lottery winner", "crypto giveaway",
    "double your bitcoin", "enlarge your", "nigerian prince", "inheritance fund", "dear beneficiary",
    "risk-free investment",
];

/// Frazy słabsze: dopiero co najmniej trzy różne razem dają głos (żeby zwykłe newslettery nie były karane za jedno słowo).
pub const WEAK: &[&str] = &[
    "darmowy", "darmowe", "gratis", "promocja", "rabat", "ostatnia szansa", "tylko dzis", "oferta ograniczona",
    "kliknij tutaj", "kliknij teraz", "nie przegap", "okazja", "100% gwarancji", "gwarancja zwrotu",
    "free", "limited time", "act now", "click here", "last chance", "special offer", "exclusive deal",
    "don't miss", "winner", "prize", "cash", "bonus", "congratulations", "urgent",
];

pub fn rules_vote(text: &str, custom: &[String]) -> Option<Vote> {
    let hay = fold(text);
    let mut strong: Vec<String> = STRONG.iter().filter(|p| contains_phrase(&hay, p)).map(|s| s.to_string()).collect();
    for c in custom {
        let c = fold(c.trim());
        if !c.is_empty() && contains_phrase(&hay, &c) && !strong.contains(&c) {
            strong.push(c);
        }
    }
    if !strong.is_empty() {
        let shown: Vec<String> = strong.iter().take(3).map(|s| format!("„{s}”")).collect();
        return Some(Vote { engine: E_RULES, strength: 75, reason: format!("Fraza typowa dla spamu: {}", shown.join(", ")) });
    }
    let weak: Vec<&str> = WEAK.iter().copied().filter(|p| contains_phrase(&hay, p)).collect();
    if weak.len() >= 3 {
        let shown: Vec<String> = weak.iter().take(4).map(|s| format!("„{s}”")).collect();
        return Some(Vote { engine: E_RULES, strength: 70, reason: format!("Kilka haseł reklamowych naraz: {}", shown.join(", ")) });
    }
    None
}

// ---------- Silnik: ocena nagłówków (istniejąca) ----------

/// `level` to poziom z istniejącej oceny ryzyka („none”, „warn”, „high”).
pub fn headers_vote(level: &str, reasons: &[String]) -> Option<Vote> {
    if level != "high" {
        return None;
    }
    let blocked = reasons.iter().any(|r| r.contains("zablokowanych"));
    let reason = reasons.first().cloned().unwrap_or_else(|| "Wysokie ryzyko wg oceny nagłówków".to_string());
    Some(Vote { engine: E_HEADERS, strength: if blocked { 100 } else { 80 }, reason })
}

// ---------- Silnik: Bayes ----------

const MIN_TRAINED: u32 = 20;
const MIN_KNOWN_TOKENS: usize = 4;
const TOP_TOKENS: usize = 20;
const MAX_TOKENS: usize = 40_000;
const MAX_TRAINED_IDS: usize = 3_000;

/// Tokeny wiadomości: słowa z tematu (`s:`), nadawcy (`f:`) i fragmentu treści, bez powtórzeń.
pub fn tokenize(subject: &str, snippet: &str, participants: &str) -> Vec<String> {
    fn words(s: &str, prefix: &str, cap: usize, out: &mut Vec<String>, seen: &mut HashSet<String>) {
        let f = fold(s);
        let mut n = 0;
        for w in f.split(|c: char| !c.is_alphanumeric()) {
            let len = w.chars().count();
            if !(3..=32).contains(&len) {
                continue;
            }
            let t = format!("{prefix}{w}");
            if seen.insert(t.clone()) {
                out.push(t);
                n += 1;
                if n >= cap {
                    break;
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    words(subject, "s:", 30, &mut out, &mut seen);
    words(participants, "f:", 8, &mut out, &mut seen);
    words(snippet, "", 60, &mut out, &mut seen);
    out
}

#[derive(Default, Debug)]
pub struct Model {
    pub spam: u32,
    pub ham: u32,
    tokens: HashMap<String, (u32, u32)>,
    trained: HashMap<String, bool>,
    order: Vec<String>,
}

impl Model {
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    pub fn ready(&self) -> bool {
        self.spam >= MIN_TRAINED && self.ham >= MIN_TRAINED
    }

    fn apply(&mut self, tokens: &[String], is_spam: bool, add: bool) {
        for t in tokens {
            let e = self.tokens.entry(t.clone()).or_insert((0, 0));
            let slot = if is_spam { &mut e.0 } else { &mut e.1 };
            *slot = if add { slot.saturating_add(1) } else { slot.saturating_sub(1) };
        }
        let total = if is_spam { &mut self.spam } else { &mut self.ham };
        *total = if add { total.saturating_add(1) } else { total.saturating_sub(1) };
    }

    /// Uczy model na jednej wiadomości. Powtórzenie tej samej decyzji nic nie zmienia, a zmiana decyzji
    /// (spam → nie spam) najpierw cofa poprzednie nauczenie. Zwraca `true`, gdy model się zmienił.
    pub fn train(&mut self, id: &str, tokens: &[String], is_spam: bool) -> bool {
        match self.trained.get(id).copied() {
            Some(prev) if prev == is_spam => return false,
            Some(prev) => self.apply(tokens, prev, false),
            None => self.order.push(id.to_string()),
        }
        self.apply(tokens, is_spam, true);
        self.trained.insert(id.to_string(), is_spam);
        self.prune();
        true
    }

    fn prune(&mut self) {
        if self.trained.len() > MAX_TRAINED_IDS {
            let drop: Vec<String> = self.order.drain(..500.min(self.order.len())).collect();
            for d in drop {
                self.trained.remove(&d);
            }
        }
        if self.tokens.len() > MAX_TOKENS {
            self.tokens.retain(|_, (s, h)| *s + *h >= 2);
        }
        if self.tokens.len() > MAX_TOKENS {
            self.tokens.retain(|_, (s, h)| *s + *h >= 3);
        }
        self.tokens.retain(|_, (s, h)| *s + *h > 0);
    }

    /// Prawdopodobieństwo spamu (0–1) albo `None`, gdy model ma za mało danych lub wiadomość za mało znanych słów.
    pub fn classify(&self, tokens: &[String]) -> Option<f64> {
        if !self.ready() {
            return None;
        }
        let ns = self.spam as f64 + 2.0;
        let nh = self.ham as f64 + 2.0;
        let mut llr: Vec<f64> = tokens
            .iter()
            .filter_map(|t| self.tokens.get(t))
            .filter(|(s, h)| s + h > 0)
            .map(|&(s, h)| ((s as f64 + 1.0) / ns).ln() - ((h as f64 + 1.0) / nh).ln())
            .collect();
        if llr.len() < MIN_KNOWN_TOKENS {
            return None;
        }
        llr.sort_by(|a, b| b.abs().partial_cmp(&a.abs()).unwrap_or(std::cmp::Ordering::Equal));
        let sum: f64 = llr.iter().take(TOP_TOKENS).sum();
        Some(1.0 / (1.0 + (-sum).exp()))
    }

    pub fn to_text(&self) -> String {
        let mut out = format!("RMBAYES1\t{}\t{}\n", self.spam, self.ham);
        let mut toks: Vec<(&String, &(u32, u32))> = self.tokens.iter().collect();
        toks.sort_by(|a, b| a.0.cmp(b.0));
        for (t, (s, h)) in toks {
            out.push_str(&format!("t\t{t}\t{s}\t{h}\n"));
        }
        for id in &self.order {
            if let Some(&sp) = self.trained.get(id) {
                out.push_str(&format!("i\t{id}\t{}\n", if sp { 1 } else { 0 }));
            }
        }
        out
    }

    pub fn from_text(text: &str) -> Model {
        let mut m = Model::default();
        let mut lines = text.lines();
        match lines.next().map(|l| l.split('\t').collect::<Vec<_>>()) {
            Some(h) if h.len() == 3 && h[0] == "RMBAYES1" => {
                m.spam = h[1].parse().unwrap_or(0);
                m.ham = h[2].parse().unwrap_or(0);
            }
            _ => return Model::default(),
        }
        for l in lines {
            let p: Vec<&str> = l.split('\t').collect();
            match p.as_slice() {
                ["t", tok, s, h] => {
                    if let (Ok(s), Ok(h)) = (s.parse::<u32>(), h.parse::<u32>()) {
                        m.tokens.insert((*tok).to_string(), (s, h));
                    }
                }
                ["i", id, v] => {
                    if m.trained.insert((*id).to_string(), *v == "1").is_none() {
                        m.order.push((*id).to_string());
                    }
                }
                _ => {}
            }
        }
        m
    }
}

pub fn bayes_vote(p: f64) -> Option<Vote> {
    let strength = if p >= 0.995 {
        90
    } else if p >= 0.98 {
        80
    } else {
        return None;
    };
    Some(Vote { engine: E_BAYES, strength, reason: format!("Filtr uczący się: {:.0}% pewności, że to spam", p * 100.0) })
}

// ---------- Silnik: otwarte listy złośliwych domen ----------

/// Duże, powszechnie używane domeny, których nigdy nie blokujemy na podstawie list (ochrona przed fałszywymi trafieniami).
pub const PROTECTED: &[&str] = &[
    "google.com", "gmail.com", "googleusercontent.com", "googleapis.com", "youtube.com", "microsoft.com", "live.com",
    "office.com", "outlook.com", "apple.com", "icloud.com", "amazon.com", "github.com", "githubusercontent.com",
    "gitlab.com", "dropbox.com", "facebook.com", "instagram.com", "linkedin.com", "twitter.com", "x.com",
    "paypal.com", "wikipedia.org", "mozilla.org", "cloudflare.com", "allegro.pl", "gov.pl", "mbank.pl", "pkobp.pl",
];

fn plausible_host(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 253
        && h.contains('.')
        && !h.starts_with('.')
        && !h.ends_with('.')
        && !h.contains("..")
        && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        && !h.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Wczytuje listę domen z popularnych formatów: „domena”, „0.0.0.0 domena”, „http://domena/ścieżka”, „||domena^”.
pub fn parse_list(text: &str, max: usize) -> HashSet<String> {
    let mut set = HashSet::new();
    for raw in text.lines() {
        let mut l = raw.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with('!') || l.starts_with('[') {
            continue;
        }
        if let Some(i) = l.find(" #") {
            l = l[..i].trim();
        }
        let mut tok = l.split_whitespace().last().unwrap_or("");
        tok = tok.trim_start_matches("||").trim_end_matches('^');
        let tok = tok.trim_start_matches("https://").trim_start_matches("http://");
        let host = tok.split(['/', '?', '#', ':']).next().unwrap_or("");
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        if plausible_host(&host) && host != "localhost" {
            set.insert(host);
            if set.len() >= max {
                break;
            }
        }
    }
    set
}

/// Sufiksy domeny od najdłuższego, bez samej końcówki (a.b.c → a.b.c, b.c).
fn suffixes(host: &str) -> Vec<&str> {
    let mut v = Vec::new();
    let mut rest = host;
    loop {
        if rest.matches('.').count() >= 1 {
            v.push(rest);
        }
        match rest.find('.') {
            Some(i) => rest = &rest[i + 1..],
            None => break,
        }
    }
    v
}

pub fn host_matches(list: &HashSet<String>, host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let suf = suffixes(&host);
    if suf.iter().any(|s| PROTECTED.contains(s)) {
        return None;
    }
    suf.into_iter().find(|s| list.contains(*s)).map(|s| s.to_string())
}

/// Adresy hostów z tekstu lub HTML (każde „http://” i „https://”). Najwyżej 200, bez powtórzeń.
pub fn extract_hosts(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut out: Vec<String> = Vec::new();
    for scheme in ["https://", "http://"] {
        let mut pos = 0;
        while let Some(i) = lower[pos..].find(scheme) {
            let start = pos + i + scheme.len();
            let rest = &lower[start..];
            let end = rest
                .find(|c: char| matches!(c, '/' | '?' | '#' | '"' | '\'' | '<' | '>' | ' ' | '\\' | ')' | '\n' | '\r' | '\t'))
                .unwrap_or(rest.len());
            let mut hp = &rest[..end];
            if let Some(at) = hp.rfind('@') {
                hp = &hp[at + 1..];
            }
            let host = hp.split(':').next().unwrap_or("").trim_end_matches('.');
            if plausible_host(host) && !out.iter().any(|h| h == host) {
                out.push(host.to_string());
                if out.len() >= 200 {
                    return out;
                }
            }
            pos = start;
        }
    }
    out
}

pub fn domain_of_email(email: &str) -> Option<String> {
    let d = email.rsplit('@').next()?.trim().trim_end_matches('>').to_ascii_lowercase();
    if plausible_host(&d) { Some(d) } else { None }
}

pub fn lists_vote(list: &HashSet<String>, sender_domain: Option<&str>, hosts: &[String]) -> Option<Vote> {
    if list.is_empty() {
        return None;
    }
    if let Some(d) = sender_domain {
        if let Some(m) = host_matches(list, d) {
            return Some(Vote { engine: E_LISTS, strength: CERTAIN_STRENGTH, reason: format!("Domena nadawcy „{m}” jest na liście złośliwych domen") });
        }
    }
    for h in hosts {
        if let Some(m) = host_matches(list, h) {
            return Some(Vote { engine: E_LISTS, strength: CERTAIN_STRENGTH, reason: format!("Link do domeny „{m}” z listy złośliwych domen") });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(engine: &'static str, strength: u8) -> Vote {
        Vote { engine, strength, reason: String::new() }
    }

    #[test]
    fn modes_decide() {
        assert_eq!(decide(Mode::Off, &[v(E_LISTS, 100)]), Decision::Keep);
        assert_eq!(decide(Mode::Flag, &[v(E_RULES, 60)]), Decision::Flag);
        assert_eq!(decide(Mode::Aggressive, &[v(E_RULES, 60)]), Decision::Move);
        assert_eq!(decide(Mode::Aggressive, &[]), Decision::Keep);
        // tryb „pewne”: jeden słabszy głos tylko oznacza, dwa różne silniki przenoszą, trafienie 95+ przenosi samo
        assert_eq!(decide(Mode::Certain, &[v(E_RULES, 75)]), Decision::Flag);
        assert_eq!(decide(Mode::Certain, &[v(E_RULES, 75), v(E_RULES, 70)]), Decision::Flag);
        assert_eq!(decide(Mode::Certain, &[v(E_RULES, 75), v(E_BAYES, 80)]), Decision::Move);
        assert_eq!(decide(Mode::Certain, &[v(E_LISTS, 95)]), Decision::Move);
        assert_eq!(Mode::parse("certain"), Mode::Certain);
        assert_eq!(Mode::parse("???"), Mode::Off);
        assert_eq!(Mode::parse(Mode::Aggressive.as_str()), Mode::Aggressive);
    }

    #[test]
    fn protection() {
        assert!(is_protected(true, false, "Ala", &[]));
        assert!(is_protected(false, true, "Ala", &[]));
        assert!(is_protected(false, false, "Ala, Ja", &[]));
        assert!(!is_protected(false, false, "Jan Kowalski", &[]));
        assert!(!is_protected(false, false, "Jacek", &["INBOX".to_string()]));
        assert!(is_protected(false, false, "Ala", &["Label_12".to_string()]));
    }

    #[test]
    fn folding_and_phrases() {
        assert_eq!(fold("WYGRAŁEŚ Żółć"), "wygrales zolc");
        assert!(contains_phrase("you have won big", "you have won"));
        assert!(!contains_phrase("cashew nuts", "cash"));
        assert!(contains_phrase("take cash now", "cash"));
        assert!(!contains_phrase("", "x"));
    }

    #[test]
    fn rules_engine() {
        let v1 = rules_vote("Gratulacje! Wygrałeś nagrodę", &[]).unwrap();
        assert_eq!(v1.engine, E_RULES);
        assert_eq!(v1.strength, 75);
        assert!(rules_vote("Spotkanie zespołu w czwartek", &[]).is_none());
        // jedno słabe hasło nie wystarcza, trzy – tak
        assert!(rules_vote("Darmowa dostawa", &[]).is_none());
        assert!(rules_vote("Free bonus, click here to get cash", &[]).is_some());
        // frazy użytkownika
        let c = vec!["Hurtownia Śrub".to_string()];
        assert!(rules_vote("Oferta od hurtownia srub", &c).is_some());
        assert!(rules_vote("Oferta od kogoś innego", &c).is_none());
    }

    #[test]
    fn headers_engine() {
        assert!(headers_vote("none", &[]).is_none());
        assert!(headers_vote("warn", &["x".to_string()]).is_none());
        let a = headers_vote("high", &["Nadawca nie przeszedł DMARC".to_string()]).unwrap();
        assert_eq!(a.strength, 80);
        let b = headers_vote("high", &["Nadawca jest na Twojej liście zablokowanych.".to_string()]).unwrap();
        assert_eq!(b.strength, 100);
    }

    #[test]
    fn tokenizer() {
        let t = tokenize("Wygrałeś iPhone!", "Kliknij tutaj aby odebrać", "Sklep Super");
        assert!(t.contains(&"s:wygrales".to_string()));
        assert!(t.contains(&"s:iphone".to_string()));
        assert!(t.contains(&"f:sklep".to_string()));
        assert!(t.contains(&"kliknij".to_string()));
        assert_eq!(t.len(), t.iter().collect::<HashSet<_>>().len());
        assert!(tokenize("", "", "").is_empty());
    }

    fn trained_model() -> Model {
        let mut m = Model::default();
        for i in 0..30 {
            let spam = tokenize(&format!("Wygraj nagrodę {i}"), "kliknij tutaj darmowy bonus kasyno", "Promo Team");
            m.train(&format!("s{i}"), &spam, true);
            let ham = tokenize(&format!("Spotkanie projektowe {i}"), "agenda raport zespół czwartek godzina", "Anna Nowak");
            m.train(&format!("h{i}"), &ham, false);
        }
        m
    }

    #[test]
    fn bayes_classifies() {
        let m = trained_model();
        assert!(m.ready());
        let spam = tokenize("Wygraj nagrodę", "kliknij tutaj darmowy bonus", "Promo Team");
        let ham = tokenize("Spotkanie projektowe", "agenda raport czwartek", "Anna Nowak");
        assert!(m.classify(&spam).unwrap() > 0.99);
        assert!(m.classify(&ham).unwrap() < 0.01);
        assert!(bayes_vote(m.classify(&spam).unwrap()).is_some());
        assert!(bayes_vote(m.classify(&ham).unwrap()).is_none());
        // nieznane słowa: brak opinii
        assert!(m.classify(&tokenize("zzzz qqqq", "wwww xxxx", "")).is_none());
    }

    #[test]
    fn bayes_needs_training_data() {
        let mut m = Model::default();
        let t = tokenize("Wygraj nagrodę", "kliknij tutaj", "X Y");
        m.train("a", &t, true);
        assert!(!m.ready());
        assert!(m.classify(&t).is_none());
    }

    #[test]
    fn bayes_retraining_is_idempotent_and_reversible() {
        let mut m = Model::default();
        let t = tokenize("Temat testowy", "treść testowa", "Ktoś Ktoś");
        assert!(m.train("x1", &t, true));
        assert!(!m.train("x1", &t, true)); // to samo = bez zmian
        assert_eq!((m.spam, m.ham), (1, 0));
        assert!(m.train("x1", &t, false)); // zmiana decyzji cofa poprzednią
        assert_eq!((m.spam, m.ham), (0, 1));
    }

    #[test]
    fn bayes_text_roundtrip() {
        let m = trained_model();
        let t = m.to_text();
        let m2 = Model::from_text(&t);
        assert_eq!((m2.spam, m2.ham, m2.token_count()), (m.spam, m.ham, m.token_count()));
        let spam = tokenize("Wygraj nagrodę", "kliknij tutaj darmowy bonus", "Promo Team");
        assert_eq!(m.classify(&spam), m2.classify(&spam));
        // uszkodzony plik nie psuje działania
        assert_eq!(Model::from_text("śmieci\n\n").spam, 0);
        assert_eq!(Model::from_text("RMBAYES1\t3\tx\nt\tabc\t1\n").token_count(), 0);
    }

    #[test]
    fn list_parsing_formats() {
        let text = "# komentarz\n127.0.0.1 evil.example\n0.0.0.0\tbad.test # opis\nhttps://phish.test/login?x=1\n||ads.example.org^\nplain.example.net\n127.0.0.1 localhost\n12.34.56.78\n!x\n";
        let s = parse_list(text, 100);
        for d in ["evil.example", "bad.test", "phish.test", "ads.example.org", "plain.example.net"] {
            assert!(s.contains(d), "{d}");
        }
        assert!(!s.contains("localhost"));
        assert!(!s.contains("12.34.56.78"));
        assert_eq!(parse_list(text, 2).len(), 2);
    }

    #[test]
    fn host_matching_uses_parent_domains_and_protection() {
        let list = parse_list("evil.example\ngoogle.com\n", 100);
        assert_eq!(host_matches(&list, "evil.example").as_deref(), Some("evil.example"));
        assert_eq!(host_matches(&list, "cdn.evil.example").as_deref(), Some("evil.example"));
        assert!(host_matches(&list, "notevil.example").is_none());
        // duże domeny nigdy nie są blokowane, nawet gdy trafią na listę
        assert!(host_matches(&list, "mail.google.com").is_none());
        assert!(host_matches(&list, "").is_none());
    }

    #[test]
    fn host_extraction() {
        let html = r#"<a href="https://evil.example/path?x=1">x</a> tekst http://user:pw@Bad.Test:8080/a. oraz https://evil.example/again"#;
        let h = extract_hosts(html);
        assert!(h.contains(&"evil.example".to_string()));
        assert!(h.contains(&"bad.test".to_string()));
        assert_eq!(h.iter().filter(|x| *x == "evil.example").count(), 1);
        assert!(extract_hosts("brak linków").is_empty());
        assert_eq!(domain_of_email("Ala <ala@Example.COM>").as_deref(), Some("example.com"));
        assert_eq!(domain_of_email("bez-malpy"), None);
    }

    #[test]
    fn lists_engine() {
        let list = parse_list("evil.example\n", 100);
        let hit = lists_vote(&list, Some("ok.pl"), &["x.evil.example".to_string()]).unwrap();
        assert_eq!(hit.strength, CERTAIN_STRENGTH);
        assert!(lists_vote(&list, Some("evil.example"), &[]).is_some());
        assert!(lists_vote(&list, Some("ok.pl"), &["fine.pl".to_string()]).is_none());
        assert!(lists_vote(&HashSet::new(), Some("evil.example"), &[]).is_none());
    }
}

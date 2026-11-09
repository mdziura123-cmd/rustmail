//! Filtr spamu – część łącząca czystą logikę (`spam.rs`) z plikami, siecią i Gmailem:
//! ustawienia, model Bayesa, dziennik, pobieranie list domen oraz skanowanie skrzynki.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::auth::AppState;
use crate::blocklist::{self, Rules};
use crate::spam::{self, Decision, Mode, Model, Vote};
use crate::{dismissed, gmail};

const LIST_MAX_AGE: Duration = Duration::from_secs(24 * 3600);
const LIST_MAX_BYTES: usize = 40 * 1024 * 1024;
const LIST_MAX_DOMAINS: usize = 1_500_000;
const SEEN_CAP: usize = 3_000;
const LOG_CAP: usize = 100;
const MAX_MOVES_PER_SCAN: usize = 50;
const DETAIL_BUDGET_AUTO: usize = 10;
const DETAIL_BUDGET_MANUAL: usize = 40;

/// (id, nazwa, adres). Adresy są stałe – aplikacja nie pobiera niczego spod adresów podanych z zewnątrz.
pub const PROVIDERS: &[(&str, &str, &str)] = &[
    (
        "urlhaus",
        "URLhaus – domeny z malware (abuse.ch, przez malware-filter)",
        "https://malware-filter.gitlab.io/malware-filter/urlhaus-filter-domains.txt",
    ),
    (
        "phishing",
        "Phishing Filter – domeny phishingowe (malware-filter)",
        "https://malware-filter.gitlab.io/malware-filter/phishing-filter-domains.txt",
    ),
    (
        "phishingdb",
        "Phishing.Database – duża lista domen phishingowych",
        "https://phish.co.za/latest/phishing-domains-ACTIVE.txt",
    ),
];

// ---------- Ustawienia ----------

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// "off" | "flag" | "certain" | "aggressive"
    pub mode: String,
    pub headers: bool,
    pub rules: bool,
    pub bayes: bool,
    pub lists: bool,
    pub provider: String,
    pub custom: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            mode: "off".to_string(),
            headers: true,
            rules: true,
            bayes: true,
            lists: false,
            provider: "urlhaus".to_string(),
            custom: Vec::new(),
        }
    }
}

fn clean_config(mut c: Config) -> Config {
    c.mode = Mode::parse(&c.mode).as_str().to_string();
    if !PROVIDERS.iter().any(|p| p.0 == c.provider) {
        c.provider = "urlhaus".to_string();
    }
    let mut seen = HashSet::new();
    c.custom = c
        .custom
        .iter()
        .map(|s| s.chars().filter(|ch| !ch.is_control()).collect::<String>().trim().chars().take(60).collect::<String>())
        .filter(|s| s.chars().count() >= 3 && seen.insert(spam::fold(s)))
        .take(100)
        .collect();
    c
}

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

fn write_private(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| format!("Nie można utworzyć katalogu: {e}"))?;
    }
    let tmp = path.with_extension("tmp");
    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp).map_err(|e| format!("Nie można zapisać pliku: {e}"))?;
        f.write_all(contents.as_bytes()).map_err(|e| format!("Nie można zapisać pliku: {e}"))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("Nie można zapisać pliku: {e}"))
}

pub fn load_config(app: &AppHandle) -> Config {
    let Ok(d) = dir(app) else { return Config::default() };
    let text = std::fs::read_to_string(d.join("spam_settings.json")).unwrap_or_default();
    clean_config(serde_json::from_str(&text).unwrap_or_default())
}

pub fn save_config(app: &AppHandle, st: &SpamState, c: Config) -> Result<Config, String> {
    let c = clean_config(c);
    let json = serde_json::to_string_pretty(&c).map_err(|e| e.to_string())?;
    write_private(&dir(app)?.join("spam_settings.json"), &json)?;
    if let Ok(mut k) = st.last_key.lock() {
        *k = None; // po zmianie ustawień skrzynka zostanie sprawdzona od nowa
    }
    Ok(c)
}

// ---------- Stan w pamięci ----------

struct LoadedList {
    provider: String,
    set: Arc<HashSet<String>>,
    loaded_at: SystemTime,
}

#[derive(Default)]
struct Seen {
    set: HashSet<String>,
    order: Vec<String>,
    loaded: bool,
}

#[derive(Default)]
pub struct SpamState {
    model: Mutex<Option<Model>>,
    list: Mutex<Option<LoadedList>>,
    seen: Mutex<Seen>,
    pub last_key: Mutex<Option<(u32, u32)>>,
    busy: tokio::sync::Mutex<()>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

// ---------- Model Bayesa ----------

fn model_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(dir(app)?.join("spam_bayes.txt"))
}

fn with_model<R>(app: &AppHandle, st: &SpamState, f: impl FnOnce(&mut Model) -> R) -> R {
    let mut g = lock(&st.model);
    if g.is_none() {
        let text = model_path(app).ok().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
        *g = Some(Model::from_text(&text));
    }
    match g.as_mut() {
        Some(m) => f(m),
        None => f(&mut Model::default()),
    }
}

fn save_model(app: &AppHandle, st: &SpamState) {
    let text = with_model(app, st, |m| m.to_text());
    if let Ok(p) = model_path(app) {
        let _ = write_private(&p, &text);
    }
}

// ---------- Wątki już ocenione ----------

fn seen_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(dir(app)?.join("spam_seen.txt"))
}

fn seen_has(app: &AppHandle, st: &SpamState, id: &str) -> bool {
    let mut g = lock(&st.seen);
    if !g.loaded {
        let text = seen_path(app).ok().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
        for l in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if g.set.insert(l.to_string()) {
                g.order.push(l.to_string());
            }
        }
        g.loaded = true;
    }
    g.set.contains(id)
}

fn seen_add(app: &AppHandle, st: &SpamState, id: &str) {
    let _ = seen_has(app, st, id); // wymusza wczytanie
    let mut g = lock(&st.seen);
    if g.set.insert(id.to_string()) {
        g.order.push(id.to_string());
        if g.order.len() > SEEN_CAP {
            let drop: Vec<String> = g.order.drain(..500).collect();
            for d in drop {
                g.set.remove(&d);
            }
        }
    }
}

fn seen_save(app: &AppHandle, st: &SpamState) {
    let text = lock(&st.seen).order.join("\n");
    if let Ok(p) = seen_path(app) {
        let _ = write_private(&p, &text);
    }
}

// ---------- Dziennik ----------

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: String,
    pub ts: i64,
    /// "moved" | "flagged" | "restored" | "spammed" | "dismissed"
    pub action: String,
    pub subject: String,
    pub sender: String,
    #[serde(default)]
    pub snippet: String,
    pub reason: String,
    /// Czy użytkownik może jeszcze coś z tym zrobić (przywrócić / zdecydować).
    pub open: bool,
}

fn log_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(dir(app)?.join("spam_log.json"))
}

pub fn read_log(app: &AppHandle) -> Vec<LogEntry> {
    log_path(app)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write_log(app: &AppHandle, entries: &[LogEntry]) {
    if let (Ok(p), Ok(json)) = (log_path(app), serde_json::to_string(entries)) {
        let _ = write_private(&p, &json);
    }
}

fn log_add(app: &AppHandle, entry: LogEntry) {
    let mut v = read_log(app);
    v.retain(|e| !(e.id == entry.id && e.open));
    v.insert(0, entry);
    v.truncate(LOG_CAP);
    write_log(app, &v);
}

/// Zamyka otwarte wpisy wątku i ustawia nowy stan (np. po przywróceniu przez użytkownika).
fn log_close(app: &AppHandle, id: &str, action: &str) {
    let mut v = read_log(app);
    let mut changed = false;
    for e in v.iter_mut().filter(|e| e.id == id && e.open) {
        e.open = false;
        e.action = action.to_string();
        changed = true;
    }
    if changed {
        write_log(app, &v);
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct FlagInfo {
    pub id: String,
    pub reason: String,
}

pub fn open_flags(app: &AppHandle) -> Vec<FlagInfo> {
    read_log(app)
        .into_iter()
        .filter(|e| e.open && e.action == "flagged")
        .map(|e| FlagInfo { id: e.id, reason: e.reason })
        .collect()
}

// ---------- Listy domen ----------

fn list_cache_path(app: &AppHandle, provider: &str) -> Result<PathBuf, String> {
    let d = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(d.join("spam_lists").join(format!("{provider}.txt")))
}

fn file_age(path: &Path) -> Option<Duration> {
    std::fs::metadata(path).ok()?.modified().ok()?.elapsed().ok()
}

/// Zwraca wczytaną listę domen. Używa pamięci, potem pliku z ostatnich 24 godzin, a dopiero potem pobiera listę.
/// Gdy pobranie się nie uda, a jest starsza kopia, korzysta ze starszej kopii.
pub async fn ensure_list(
    app: &AppHandle,
    st: &SpamState,
    http: &reqwest::Client,
    provider: &str,
    force: bool,
) -> Result<Arc<HashSet<String>>, String> {
    let url = PROVIDERS.iter().find(|p| p.0 == provider).map(|p| p.2).ok_or("Nieznany dostawca listy.")?;
    if !force {
        let g = lock(&st.list);
        if let Some(l) = g.as_ref() {
            let fresh = l.loaded_at.elapsed().map(|e| e < LIST_MAX_AGE).unwrap_or(false);
            if l.provider == provider && fresh {
                return Ok(l.set.clone());
            }
        }
    }
    let path = list_cache_path(app, provider)?;
    if !force && file_age(&path).map(|a| a < LIST_MAX_AGE).unwrap_or(false) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let set = Arc::new(spam::parse_list(&text, LIST_MAX_DOMAINS));
            if !set.is_empty() {
                *lock(&st.list) = Some(LoadedList { provider: provider.to_string(), set: set.clone(), loaded_at: SystemTime::now() });
                return Ok(set);
            }
        }
    }
    let downloaded = download_list(http, url).await;
    match downloaded {
        Ok(text) => {
            let set = Arc::new(spam::parse_list(&text, LIST_MAX_DOMAINS));
            if set.is_empty() {
                return Err("Pobrana lista jest pusta lub w nieznanym formacie.".to_string());
            }
            let _ = write_private(&path, &text);
            *lock(&st.list) = Some(LoadedList { provider: provider.to_string(), set: set.clone(), loaded_at: SystemTime::now() });
            Ok(set)
        }
        Err(e) => {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let set = Arc::new(spam::parse_list(&text, LIST_MAX_DOMAINS));
                if !set.is_empty() {
                    *lock(&st.list) = Some(LoadedList { provider: provider.to_string(), set: set.clone(), loaded_at: SystemTime::now() });
                    return Ok(set);
                }
            }
            Err(e)
        }
    }
}

async fn download_list(http: &reqwest::Client, url: &str) -> Result<String, String> {
    if !url.starts_with("https://") {
        return Err("Niedozwolony adres listy.".to_string());
    }
    let resp = http
        .get(url)
        .header("Accept", "text/plain")
        .send()
        .await
        .map_err(|e| format!("Brak połączenia z serwerem listy: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("Serwer listy zwrócił błąd {status}."));
    }
    if resp.content_length().map(|l| l as usize > LIST_MAX_BYTES).unwrap_or(false) {
        return Err("Lista jest zbyt duża.".to_string());
    }
    let bytes = resp.bytes().await.map_err(|e| format!("Nie udało się pobrać listy: {e}"))?;
    if bytes.len() > LIST_MAX_BYTES {
        return Err("Lista jest zbyt duża.".to_string());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// ---------- Status ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub bayes_spam: u32,
    pub bayes_ham: u32,
    pub bayes_tokens: usize,
    pub bayes_ready: bool,
    pub list_domains: usize,
    pub list_provider: Option<String>,
    pub list_age_hours: Option<u64>,
    pub providers: Vec<ProviderInfo>,
}

pub fn status(app: &AppHandle, st: &SpamState) -> Status {
    let (bayes_spam, bayes_ham, bayes_tokens, bayes_ready) =
        with_model(app, st, |m| (m.spam, m.ham, m.token_count(), m.ready()));
    let (list_domains, list_provider, list_age_hours) = match lock(&st.list).as_ref() {
        Some(l) => (
            l.set.len(),
            Some(l.provider.clone()),
            l.loaded_at.elapsed().ok().map(|e| e.as_secs() / 3600),
        ),
        None => (0, None, None),
    };
    Status {
        bayes_spam,
        bayes_ham,
        bayes_tokens,
        bayes_ready,
        list_domains,
        list_provider,
        list_age_hours,
        providers: PROVIDERS.iter().map(|p| ProviderInfo { id: p.0, label: p.1 }).collect(),
    }
}

// ---------- Uczenie ----------

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TrainItem {
    pub id: String,
    pub subject: String,
    pub snippet: String,
    pub participants: String,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Uczy model na wiadomościach (spam lub nie spam), oznacza je jako ocenione i zamyka otwarte wpisy dziennika.
pub fn train_items(app: &AppHandle, st: &SpamState, items: &[TrainItem], is_spam: bool) -> usize {
    let mut changed = 0;
    with_model(app, st, |m| {
        for it in items.iter().filter(|i| valid_id(&i.id)).take(200) {
            let toks = spam::tokenize(&it.subject, &it.snippet, &it.participants);
            if m.train(&it.id, &toks, is_spam) {
                changed += 1;
            }
        }
    });
    for it in items.iter().filter(|i| valid_id(&i.id)).take(200) {
        seen_add(app, st, &it.id);
        log_close(app, &it.id, if is_spam { "spammed" } else { "dismissed" });
    }
    if changed > 0 {
        save_model(app, st);
    }
    seen_save(app, st);
    changed
}

#[derive(Serialize)]
pub struct BootstrapResult {
    pub spam: usize,
    pub ham: usize,
}

/// Uczy model na Twojej poczcie: do 100 wątków ze Spamu (jako spam) i do 100 przeczytanych wątków ze skrzynki
/// odbiorczej bez ostrzeżeń (jako zwykła poczta).
pub async fn bootstrap(app: &AppHandle, st: &SpamState, gm: &AppState, email: &str) -> Result<BootstrapResult, String> {
    let _g = st.busy.lock().await;
    let (safe, rules) = filters_data(app)?;
    let filters = gmail::Filters { dismissed: &safe, rules: &rules };
    let mut spam_items: Vec<TrainItem> = Vec::new();
    let mut ham_items: Vec<TrainItem> = Vec::new();
    for folder in ["spam", "inbox"] {
        let mut token: Option<String> = None;
        for _ in 0..5 {
            let page = gmail::list_threads(gm, folder, token.clone(), email, &filters).await?;
            for t in &page.threads {
                let item = TrainItem { id: t.id.clone(), subject: t.subject.clone(), snippet: t.snippet.clone(), participants: t.participants.clone() };
                if folder == "spam" {
                    spam_items.push(item);
                } else if !t.unread && t.risk.level == "none" && !t.dismissed {
                    ham_items.push(item);
                }
            }
            token = page.next_page_token;
            if token.is_none() {
                break;
            }
        }
    }
    let (ns, nh) = (spam_items.len(), ham_items.len());
    with_model(app, st, |m| {
        for it in spam_items.iter().filter(|i| valid_id(&i.id)) {
            m.train(&it.id, &spam::tokenize(&it.subject, &it.snippet, &it.participants), true);
        }
        for it in ham_items.iter().filter(|i| valid_id(&i.id)) {
            m.train(&it.id, &spam::tokenize(&it.subject, &it.snippet, &it.participants), false);
        }
    });
    save_model(app, st);
    Ok(BootstrapResult { spam: ns, ham: nh })
}

// ---------- Skanowanie ----------

fn filters_data(app: &AppHandle) -> Result<(HashSet<String>, Rules), String> {
    let d = dir(app)?;
    let safe = dismissed::load(&d.join("dismissed.txt"));
    let rules = Rules { blocked: blocklist::load_list(&d.join("blocked.txt")), allowed: blocklist::load_list(&d.join("allowed.txt")) };
    Ok((safe, rules))
}

fn engine_label(e: &str) -> &'static str {
    match e {
        spam::E_HEADERS => "Nagłówki",
        spam::E_RULES => "Reguły",
        spam::E_BAYES => "Bayes",
        _ => "Listy domen",
    }
}

fn join_reasons(votes: &[Vote]) -> String {
    votes.iter().map(|v| format!("{}: {}", engine_label(v.engine), v.reason)).collect::<Vec<_>>().join(" • ")
}

#[derive(Serialize, Default, Debug)]
pub struct ScanOutcome {
    pub moved: usize,
    pub flagged: usize,
    pub checked: usize,
    pub notes: Vec<String>,
}

/// Sprawdza skrzynkę odbiorczą. `pages` = ile stron po 20 wątków, `only_unread` = tylko nieprzeczytane (skanowanie
/// automatyczne nie rusza przeczytanych wiadomości).
pub async fn scan(
    app: &AppHandle,
    st: &SpamState,
    gm: &AppState,
    email: &str,
    pages: usize,
    only_unread: bool,
) -> Result<ScanOutcome, String> {
    let cfg = load_config(app);
    let mode = Mode::parse(&cfg.mode);
    let mut out = ScanOutcome::default();
    if mode == Mode::Off {
        return Ok(out);
    }
    let _g = st.busy.lock().await;
    let (safe, rules) = filters_data(app)?;
    let filters = gmail::Filters { dismissed: &safe, rules: &rules };

    let list = if cfg.lists {
        match ensure_list(app, st, &gm.http, &cfg.provider, false).await {
            Ok(l) => Some(l),
            Err(e) => {
                out.notes.push(format!("Listy domen: {e}"));
                None
            }
        }
    } else {
        None
    };
    let mut detail_budget = if only_unread { DETAIL_BUDGET_AUTO } else { DETAIL_BUDGET_MANUAL };

    let mut token: Option<String> = None;
    'pages: for _ in 0..pages.max(1) {
        let page = gmail::list_threads(gm, "inbox", token.clone(), email, &filters).await?;
        for t in &page.threads {
            if seen_has(app, st, &t.id) {
                continue;
            }
            if only_unread && !t.unread {
                continue;
            }
            if spam::is_protected(t.starred, t.dismissed, &t.participants, &t.labels) {
                seen_add(app, st, &t.id);
                continue;
            }
            out.checked += 1;
            let mut votes: Vec<Vote> = Vec::new();
            if cfg.headers {
                votes.extend(spam::headers_vote(&t.risk.level, &t.risk.reasons));
            }
            if cfg.rules {
                let text = format!("{} {} {}", t.subject, t.snippet, t.participants);
                votes.extend(spam::rules_vote(&text, &cfg.custom));
            }
            if cfg.bayes {
                let toks = spam::tokenize(&t.subject, &t.snippet, &t.participants);
                if let Some(p) = with_model(app, st, |m| m.classify(&toks)) {
                    votes.extend(spam::bayes_vote(p));
                }
            }
            if let Some(list) = list.as_ref() {
                if detail_budget == 0 {
                    continue; // zostanie sprawdzone przy następnym skanowaniu
                }
                detail_budget -= 1;
                if let Ok(d) = gmail::get_thread(gm, &t.id, email, &filters).await {
                    let mut hosts: Vec<String> = Vec::new();
                    for m in d.messages.iter().filter(|m| !m.mine) {
                        for h in spam::extract_hosts(&m.html).into_iter().chain(spam::extract_hosts(&m.body)) {
                            if !hosts.contains(&h) && hosts.len() < 300 {
                                hosts.push(h);
                            }
                        }
                    }
                    let sender = d.messages.iter().rev().find(|m| !m.mine).and_then(|m| spam::domain_of_email(&m.from_email));
                    votes.extend(spam::lists_vote(list, sender.as_deref(), &hosts));
                }
            }

            match spam::decide(mode, &votes) {
                Decision::Keep => {}
                Decision::Flag => {
                    log_add(
                        app,
                        LogEntry {
                            id: t.id.clone(),
                            ts: now_ms(),
                            action: "flagged".to_string(),
                            subject: t.subject.clone(),
                            sender: t.participants.clone(),
                            snippet: t.snippet.clone(),
                            reason: join_reasons(&votes),
                            open: true,
                        },
                    );
                    out.flagged += 1;
                }
                Decision::Move => {
                    if out.moved >= MAX_MOVES_PER_SCAN {
                        out.notes.push(format!("Osiągnięto limit {MAX_MOVES_PER_SCAN} przeniesień na jedno skanowanie."));
                        break 'pages;
                    }
                    match gmail::thread_action(gm, &t.id, "spam").await {
                        Ok(()) => {
                            log_add(
                                app,
                                LogEntry {
                                    id: t.id.clone(),
                                    ts: now_ms(),
                                    action: "moved".to_string(),
                                    subject: t.subject.clone(),
                                    sender: t.participants.clone(),
                            snippet: t.snippet.clone(),
                                    reason: join_reasons(&votes),
                                    open: true,
                                },
                            );
                            out.moved += 1;
                        }
                        Err(e) => {
                            out.notes.push(format!("Nie udało się przenieść wiadomości: {e}"));
                            continue;
                        }
                    }
                }
            }
            seen_add(app, st, &t.id);
        }
        token = page.next_page_token;
        if token.is_none() {
            break;
        }
    }
    seen_save(app, st);
    Ok(out)
}

/// Wywoływane przy każdym odpytywaniu poczty: skanuje tylko wtedy, gdy zmieniły się liczniki skrzynki.
pub async fn auto_scan(app: &AppHandle, st: &SpamState, gm: &AppState, email: &str, key: (u32, u32)) -> ScanOutcome {
    if Mode::parse(&load_config(app).mode) == Mode::Off {
        return ScanOutcome::default();
    }
    if *lock(&st.last_key) == Some(key) {
        return ScanOutcome::default();
    }
    match scan(app, st, gm, email, 1, true).await {
        Ok(o) => {
            *lock(&st.last_key) = Some(key);
            o
        }
        Err(_) => ScanOutcome::default(),
    }
}

// ---------- Konta IMAP (np. WP) ----------

const MAX_IMAP_MOVES: usize = 20;

/// Wiadomość ze skrzynki IMAP do oceny (tylko nagłówki: nadawca i temat).
pub struct ImapCand {
    /// Unikalny klucz, np. „imap:konto:uid”.
    pub key: String,
    pub from: String,
    pub subject: String,
    pub starred: bool,
}

pub struct ImapVerdict {
    pub idx: usize,
    pub reason: String,
}

/// Ocenia wiadomości z IMAP tymi samymi silnikami co w Gmailu. Zwraca te, które trzeba przenieść do Spamu;
/// podejrzane (tryby „oznaczaj” / „pewne”) trafiają tylko do dziennika. Silnik list używa domeny nadawcy
/// (linki w treści nie są pobierane – to byłby osobny koszt dla każdej wiadomości).
pub async fn evaluate_imap(app: &AppHandle, st: &SpamState, http: &reqwest::Client, cands: &[ImapCand]) -> Vec<ImapVerdict> {
    let cfg = load_config(app);
    let mode = Mode::parse(&cfg.mode);
    if mode == Mode::Off || cands.is_empty() {
        return Vec::new();
    }
    let _g = st.busy.lock().await;
    let Ok((_safe, rules)) = filters_data(app) else { return Vec::new() };
    let list = if cfg.lists { ensure_list(app, st, http, &cfg.provider, false).await.ok() } else { None };
    let mut out: Vec<ImapVerdict> = Vec::new();
    for (idx, c) in cands.iter().enumerate() {
        if seen_has(app, st, &c.key) {
            continue;
        }
        let addr = crate::imap_proto::bare_addr(&c.from).to_ascii_lowercase();
        if c.starred || rules.is_allowed(&addr) {
            seen_add(app, st, &c.key);
            continue;
        }
        let mut votes: Vec<Vote> = Vec::new();
        if cfg.headers && rules.is_blocked(&addr) {
            votes.extend(spam::headers_vote("high", &["Nadawca jest na Twojej liście zablokowanych".to_string()]));
        }
        if cfg.rules {
            votes.extend(spam::rules_vote(&format!("{} {}", c.subject, c.from), &cfg.custom));
        }
        if cfg.bayes {
            let toks = spam::tokenize(&c.subject, "", &c.from);
            if let Some(p) = with_model(app, st, |m| m.classify(&toks)) {
                votes.extend(spam::bayes_vote(p));
            }
        }
        if let Some(list) = list.as_ref() {
            votes.extend(spam::lists_vote(list, spam::domain_of_email(&addr).as_deref(), &[]));
        }
        match spam::decide(mode, &votes) {
            Decision::Keep => {}
            Decision::Flag => log_add(
                app,
                LogEntry {
                    id: c.key.clone(),
                    ts: now_ms(),
                    action: "flagged".to_string(),
                    subject: c.subject.clone(),
                    sender: c.from.clone(),
                    snippet: String::new(),
                    reason: join_reasons(&votes),
                    open: false,
                },
            ),
            Decision::Move => {
                if out.len() >= MAX_IMAP_MOVES {
                    continue; // reszta przy następnym sprawdzeniu
                }
                out.push(ImapVerdict { idx, reason: join_reasons(&votes) });
                continue; // zapamiętamy dopiero po udanym przeniesieniu
            }
        }
        seen_add(app, st, &c.key);
    }
    seen_save(app, st);
    out
}

/// Zapisuje w dzienniku udane przeniesienie wiadomości z IMAP do Spamu.
pub fn imap_moved(app: &AppHandle, st: &SpamState, c: &ImapCand, reason: &str) {
    log_add(
        app,
        LogEntry {
            id: c.key.clone(),
            ts: now_ms(),
            action: "moved".to_string(),
            subject: c.subject.clone(),
            sender: c.from.clone(),
            snippet: String::new(),
            reason: reason.to_string(),
            open: false,
        },
    );
    seen_add(app, st, &c.key);
    seen_save(app, st);
}

// ---------- Decyzje użytkownika ----------

/// Przywraca wiadomość ze Spamu do skrzynki i uczy filtr, że to nie spam.
pub async fn restore(app: &AppHandle, st: &SpamState, gm: &AppState, item: &TrainItem) -> Result<(), String> {
    gmail::thread_action(gm, &item.id, "notspam").await?;
    log_close(app, &item.id, "restored");
    train_items(app, st, std::slice::from_ref(item), false);
    Ok(())
}

/// Przenosi oznaczoną wiadomość do Spamu i uczy filtr, że to spam.
pub async fn move_to_spam(app: &AppHandle, st: &SpamState, gm: &AppState, item: &TrainItem) -> Result<(), String> {
    gmail::thread_action(gm, &item.id, "spam").await?;
    log_close(app, &item.id, "spammed");
    train_items(app, st, std::slice::from_ref(item), true);
    Ok(())
}

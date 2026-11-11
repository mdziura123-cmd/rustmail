//! Odczyt wątków przez oficjalne Gmail API (etap 2: wyłącznie odczyt).
//! Treść wiadomości jest traktowana jako niezaufana: zwracamy zwykły tekst, nigdy HTML.

use std::collections::HashSet;
use std::time::Duration;

use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::task::JoinSet;

use crate::auth::{access_token, AppState};
use crate::blocklist::Rules;
use crate::phishing::{self, Assessment, Signals};

const API: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
const PAGE_SIZE: &str = "20";

// ---------- Modele odpowiedzi Gmail API ----------

#[derive(Deserialize)]
struct ThreadList {
    #[serde(default)]
    threads: Vec<ThreadRef>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct ThreadRef {
    id: String,
}

#[derive(Deserialize, Default)]
struct RawThread {
    #[serde(default)]
    id: String,
    #[serde(default)]
    messages: Vec<RawMessage>,
}

#[derive(Deserialize, Default)]
struct RawMessage {
    #[serde(default)]
    id: String,
    #[serde(default, rename = "labelIds")]
    label_ids: Vec<String>,
    #[serde(default)]
    snippet: String,
    #[serde(default, rename = "internalDate")]
    internal_date: String,
    payload: Option<Part>,
}

#[derive(Deserialize, Default)]
struct Part {
    #[serde(default, rename = "mimeType")]
    mime_type: String,
    #[serde(default)]
    filename: String,
    #[serde(default)]
    headers: Vec<Header>,
    body: Option<Body>,
    #[serde(default)]
    parts: Vec<Part>,
}

#[derive(Deserialize)]
struct Header {
    name: String,
    value: String,
}

#[derive(Deserialize, Default)]
struct Body {
    #[serde(default)]
    size: u64,
    data: Option<String>,
    #[serde(default, rename = "attachmentId")]
    attachment_id: Option<String>,
}

// ---------- Modele dla interfejsu ----------

/// Lokalne ustawienia użytkownika wpływające na ocenę: wątki uznane za bezpieczne oraz listy nadawców.
pub struct Filters<'a> {
    pub dismissed: &'a HashSet<String>,
    pub rules: &'a Rules,
}

/// Ocena ryzyka phishingu przekazywana do interfejsu (powody po polsku).
#[derive(Serialize, Deserialize, Clone)]
pub struct Risk {
    pub level: String,
    pub score: u32,
    pub reasons: Vec<String>,
}

impl From<Assessment> for Risk {
    fn from(a: Assessment) -> Self {
        Risk { level: a.level.as_str().to_string(), score: a.score, reasons: a.reasons }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    pub id: String,
    pub subject: String,
    pub participants: String,
    pub snippet: String,
    pub last_date_ms: i64,
    pub unread: bool,
    pub starred: bool,
    pub count: usize,
    /// Wstępna ocena z nagłówków (nadawca, uwierzytelnianie, temat); pełna ocena jest po otwarciu wątku.
    pub risk: Risk,
    pub dismissed: bool,
    /// Etykiety Gmail wszystkich wiadomości wątku (do filtrowania folderów w trybie offline).
    #[serde(default)]
    pub labels: Vec<String>,
    /// Adresy nadawców (do reguł automatycznych); wypełniane tylko dla list pobranych z serwera.
    #[serde(default)]
    pub senders: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPage {
    pub threads: Vec<ThreadSummary>,
    pub next_page_token: Option<String>,
    /// Prawda, gdy wynik pochodzi z lokalnej pamięci podręcznej (brak połączenia).
    pub offline: bool,
}

#[derive(Serialize, Deserialize)]
pub struct AttachmentInfo {
    pub name: String,
    pub mime: String,
    pub size: u64,
    /// Identyfikator do pobrania; brak oznacza, że Gmail nie udostępnił załącznika do pobrania.
    pub id: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub id: String,
    pub from: String,
    pub from_email: String,
    pub mine: bool,
    pub date_ms: i64,
    pub body: String,
    pub html_only: bool,
    /// Surowy HTML wiadomości (niezaufany!). Interfejs czyści go przed wyświetleniem.
    pub html: String,
    pub attachments: Vec<AttachmentInfo>,
    pub to: String,
    pub cc: String,
    pub reply_to: String,
    pub message_id: String,
    pub references: String,
    pub risk: Risk,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadDetail {
    pub id: String,
    pub subject: String,
    pub messages: Vec<MessageView>,
    pub unread: bool,
    pub starred: bool,
    pub in_inbox: bool,
    pub in_spam: bool,
    pub in_trash: bool,
    pub is_draft: bool,
    pub dismissed: bool,
    #[serde(default)]
    pub labels: Vec<String>,
    /// Prawda, gdy wątek pochodzi z lokalnej pamięci podręcznej (brak połączenia).
    #[serde(default)]
    pub offline: bool,
}

// ---------- Mapowanie folderów na etykiety Gmail ----------

struct FolderQuery {
    label: Option<&'static str>,
    q: Option<&'static str>,
    spam_trash: bool,
}

fn fq(label: Option<&'static str>, q: Option<&'static str>, spam_trash: bool) -> Result<Option<FolderQuery>, String> {
    Ok(Some(FolderQuery { label, q, spam_trash }))
}

/// `Ok(None)` oznacza folder lokalny (Podejrzane), który nie istnieje w Gmailu.
fn folder_query(folder: &str) -> Result<Option<FolderQuery>, String> {
    match folder {
        "inbox" => fq(Some("INBOX"), None, false),
        "unread" => fq(None, Some("in:inbox is:unread"), false),
        "sent" => fq(Some("SENT"), None, false),
        "drafts" => fq(Some("DRAFT"), None, false),
        "spam" => fq(Some("SPAM"), None, true),
        "trash" => fq(Some("TRASH"), None, true),
        "archive" => fq(None, Some("-in:inbox -in:sent -in:drafts -in:spam -in:trash"), false),
        "starred" => fq(Some("STARRED"), None, false),
        "important" => fq(Some("IMPORTANT"), None, false),
        "suspicious" => Ok(None),
        other => Err(format!("Nieznany folder: {other}")),
    }
}

// ---------- Zapytania HTTP z ponawianiem ----------

async fn get_json<T: DeserializeOwned>(
    http: &reqwest::Client,
    token: &str,
    path: &str,
    params: &[(&str, &str)],
) -> Result<T, String> {
    let url = format!("{API}/{path}");
    let mut last_err = String::from("Nieznany błąd");
    for attempt in 0..3u32 {
        let resp = match http.get(&url).bearer_auth(token).query(params).send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = format!("Brak połączenia z Gmail: {e}");
                tokio::time::sleep(Duration::from_millis(500u64 << attempt)).await;
                continue;
            }
        };
        let status = resp.status();
        if status.as_u16() == 429 || status.is_server_error() {
            last_err = format!("Gmail chwilowo niedostępny lub ogranicza liczbę zapytań ({status})");
            tokio::time::sleep(Duration::from_millis(500u64 << attempt)).await;
            continue;
        }
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if status.as_u16() == 401 {
            return Err("Autoryzacja Gmail wygasła. Zaloguj się ponownie.".to_string());
        }
        if !status.is_success() {
            let short: String = text.chars().take(300).collect();
            return Err(format!("Gmail zwrócił błąd {status}: {short}"));
        }
        return serde_json::from_str(&text).map_err(|e| format!("Nieoczekiwana odpowiedź Gmail: {e}"));
    }
    Err(last_err)
}

pub async fn profile_email(http: &reqwest::Client, token: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Profile {
        #[serde(rename = "emailAddress")]
        email_address: String,
    }
    Ok(get_json::<Profile>(http, token, "profile", &[]).await?.email_address)
}

async fn fetch_thread(http: &reqwest::Client, token: &str, id: &str, format: &str) -> Result<RawThread, String> {
    let path = format!("threads/{id}");
    if format == "metadata" {
        get_json(
            http,
            token,
            &path,
            &[
                ("format", "metadata"),
                ("metadataHeaders", "From"),
                ("metadataHeaders", "Subject"),
                ("metadataHeaders", "Reply-To"),
                ("metadataHeaders", "Authentication-Results"),
            ],
        )
        .await
    } else {
        get_json(http, token, &path, &[("format", format)]).await
    }
}

/// Pobiera stronę wątków (metadane równolegle) i zamienia je na podsumowania z oceną z nagłówków.
async fn page_summaries(
    state: &AppState,
    token: &str,
    params: &[(&str, &str)],
    my_email: &str,
    f: &Filters<'_>,
) -> Result<(Vec<ThreadSummary>, Option<String>), String> {
    let list: ThreadList = get_json(&state.http, token, "threads", params).await?;

    let mut set = JoinSet::new();
    for (i, t) in list.threads.iter().enumerate() {
        let http = state.http.clone();
        let tok = token.to_string();
        let id = t.id.clone();
        set.spawn(async move { (i, fetch_thread(&http, &tok, &id, "metadata").await) });
    }
    let mut results: Vec<(usize, Result<RawThread, String>)> = Vec::new();
    while let Some(r) = set.join_next().await {
        results.push(r.map_err(|e| e.to_string())?);
    }
    results.sort_by_key(|(i, _)| *i);

    let mut threads = Vec::new();
    let mut first_err: Option<String> = None;
    for (_, r) in results {
        match r {
            Ok(raw) => match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| summarize(&raw, my_email, f))) {
                Ok(s) => threads.push(s),
                Err(_) => {
                    first_err.get_or_insert("Nie udało się przetworzyć jednej z wiadomości.".to_string());
                }
            },
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    if threads.is_empty() {
        if let Some(e) = first_err {
            return Err(e);
        }
    }
    Ok((threads, list.next_page_token))
}

/// Folder „Podejrzane”: przeszukuje lokalnie ostatnie 100 wątków skrzynki odbiorczej i zwraca te,
/// które mają ostrzeżenie (i nie zostały uznane za bezpieczne). Nic nie jest przenoszone w Gmailu.
const SCAN_PAGES: usize = 5;

async fn scan_suspicious(
    state: &AppState,
    my_email: &str,
    f: &Filters<'_>,
) -> Result<ThreadPage, String> {
    let token = access_token(state).await?;
    let mut found: Vec<ThreadSummary> = Vec::new();
    let mut page: Option<String> = None;
    for _ in 0..SCAN_PAGES {
        let mut params: Vec<(&str, &str)> = vec![("maxResults", PAGE_SIZE), ("labelIds", "INBOX")];
        if let Some(p) = page.as_deref() {
            params.push(("pageToken", p));
        }
        let (threads, next) = page_summaries(state, &token, &params, my_email, f).await?;
        found.extend(threads.into_iter().filter(|t| t.risk.level != "none" && !t.dismissed));
        match next {
            Some(n) => page = Some(n),
            None => break,
        }
    }
    Ok(ThreadPage { threads: found, next_page_token: None, offline: false })
}

pub async fn list_threads(
    state: &AppState,
    folder: &str,
    page: Option<String>,
    my_email: &str,
    f: &Filters<'_>,
) -> Result<ThreadPage, String> {
    if folder == "suspicious" {
        return scan_suspicious(state, my_email, f).await;
    }
    if let Some(label) = folder.strip_prefix("label:") {
        crate::labels::check_user_label_id(label)?;
        let token = access_token(state).await?;
        let mut params: Vec<(&str, &str)> = vec![("maxResults", PAGE_SIZE), ("labelIds", label)];
        if let Some(p) = page.as_deref() {
            params.push(("pageToken", p));
        }
        let (threads, next) = page_summaries(state, &token, &params, my_email, f).await?;
        return Ok(ThreadPage { threads, next_page_token: next, offline: false });
    }
    let Some(fq) = folder_query(folder)? else {
        return Ok(ThreadPage { threads: vec![], next_page_token: None, offline: false });
    };
    let token = access_token(state).await?;

    let mut params: Vec<(&str, String)> = vec![("maxResults", PAGE_SIZE.to_string())];
    if let Some(l) = fq.label {
        params.push(("labelIds", l.to_string()));
    }
    if let Some(q) = fq.q {
        params.push(("q", q.to_string()));
    }
    if fq.spam_trash {
        params.push(("includeSpamTrash", "true".to_string()));
    }
    if let Some(p) = page {
        params.push(("pageToken", p));
    }
    let borrowed: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let (threads, next) = page_summaries(state, &token, &borrowed, my_email, f).await?;
    Ok(ThreadPage { threads, next_page_token: next, offline: false })
}

/// Nagłówki adresowe (Od, Do, DW) ostatnich wiadomości z danej etykiety – do uzupełnienia książki adresów.
/// Zwraca wartości nagłówków; nic nie jest zmieniane w Gmailu.
pub async fn harvest_headers(state: &AppState, label: &str, count: usize) -> Result<Vec<(String, String, String)>, String> {
    #[derive(Deserialize)]
    struct MsgRef { id: String }
    #[derive(Deserialize)]
    struct MsgList {
        #[serde(default)]
        messages: Vec<MsgRef>,
    }
    let token = access_token(state).await?;
    let max = count.clamp(1, 300).to_string();
    let list: MsgList = get_json(&state.http, &token, "messages", &[("labelIds", label), ("maxResults", &max)]).await?;
    let mut out = Vec::new();
    for chunk in list.messages.chunks(8) {
        let mut set = JoinSet::new();
        for m in chunk {
            check_id(&m.id)?;
            let (http, tok, id) = (state.http.clone(), token.clone(), m.id.clone());
            set.spawn(async move {
                get_json::<RawMessage>(
                    &http, &tok, &format!("messages/{id}"),
                    &[("format", "metadata"), ("metadataHeaders", "From"), ("metadataHeaders", "To"), ("metadataHeaders", "Cc")],
                ).await
            });
        }
        while let Some(r) = set.join_next().await {
            let Ok(Ok(m)) = r else { continue };
            if let Some(p) = &m.payload {
                let g = |n: &str| decode_mime_words(header(p, n).unwrap_or(""));
                out.push((g("From"), g("To"), g("Cc")));
            }
        }
    }
    Ok(out)
}

pub async fn get_thread(
    state: &AppState,
    id: &str,
    my_email: &str,
    f: &Filters<'_>,
) -> Result<ThreadDetail, String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Niepoprawny identyfikator wątku".to_string());
    }
    let token = access_token(state).await?;
    let raw = fetch_thread(&state.http, &token, id, "full").await?;
    let subject = subject_of(&raw);
    let messages = raw.messages.iter().map(|m| message_view(m, my_email, f.rules)).collect();
    let has = |l: &str| raw.messages.iter().any(|m| m.label_ids.iter().any(|x| x == l));
    Ok(ThreadDetail {
        unread: has("UNREAD"),
        starred: has("STARRED"),
        in_inbox: has("INBOX"),
        in_spam: has("SPAM"),
        in_trash: has("TRASH"),
        is_draft: has("DRAFT"),
        dismissed: f.dismissed.contains(&raw.id),
        labels: thread_labels(&raw),
        offline: false,
        id: raw.id.clone(),
        subject,
        messages,
    })
}

// ---------- Wyszukiwanie, podsumowanie z szczegółów i synchronizacja przyrostowa ----------

/// Wyszukiwanie w Gmailu (składnia jak w oknie wyszukiwania Gmaila). Zapytanie jest oczyszczone i przycięte.
pub async fn search_threads(
    state: &AppState,
    query: &str,
    page: Option<String>,
    my_email: &str,
    f: &Filters<'_>,
) -> Result<ThreadPage, String> {
    let q = crate::offline::clean_remote_query(query)?;
    let token = access_token(state).await?;
    let mut params: Vec<(&str, String)> = vec![("maxResults", PAGE_SIZE.to_string()), ("q", q)];
    if let Some(p) = page {
        params.push(("pageToken", p));
    }
    let borrowed: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let (threads, next) = page_summaries(state, &token, &borrowed, my_email, f).await?;
    Ok(ThreadPage { threads, next_page_token: next, offline: false })
}

/// Podsumowanie wątku zbudowane z jego pełnych danych (do zapisu w pamięci podręcznej).
pub fn summary_of_detail(d: &ThreadDetail) -> ThreadSummary {
    let mut names: Vec<String> = Vec::new();
    for m in &d.messages {
        if !m.from.is_empty() && !names.contains(&m.from) {
            names.push(m.from.clone());
        }
    }
    let last = d.messages.last();
    let snippet: String = last
        .map(|m| m.body.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
        .chars()
        .take(160)
        .collect();
    ThreadSummary {
        id: d.id.clone(),
        subject: d.subject.clone(),
        participants: names.into_iter().take(3).collect::<Vec<_>>().join(", "),
        snippet,
        last_date_ms: last.map(|m| m.date_ms).unwrap_or(0),
        unread: d.unread,
        starred: d.starred,
        count: d.messages.len(),
        risk: d
            .messages
            .iter()
            .max_by_key(|m| m.risk.score)
            .map(|m| m.risk.clone())
            .unwrap_or(Risk { level: "none".to_string(), score: 0, reasons: vec![] }),
        dismissed: d.dismissed,
        labels: d.labels.clone(),
        senders: String::new(),
    }
}

/// Bieżący identyfikator historii skrzynki (punkt odniesienia dla synchronizacji przyrostowej).
pub async fn current_history_id(state: &AppState) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Profile {
        #[serde(rename = "historyId")]
        history_id: String,
    }
    let token = access_token(state).await?;
    Ok(get_json::<Profile>(&state.http, &token, "profile", &[]).await?.history_id)
}

pub struct HistoryChanges {
    pub thread_ids: Vec<String>,
    pub new_history_id: String,
}

/// Zmiany od podanego punktu odniesienia. `Ok(None)` = historia wygasła (Gmail trzyma ją krótko),
/// trzeba zacząć od nowa. Zbyt wiele stron zmian też traktujemy jak wygasłą historię.
pub async fn history_since(state: &AppState, start: &str) -> Result<Option<HistoryChanges>, String> {
    #[derive(Deserialize, Default)]
    struct MsgRef {
        #[serde(default, rename = "threadId")]
        thread_id: String,
    }
    #[derive(Deserialize)]
    struct MsgWrap {
        message: MsgRef,
    }
    #[derive(Deserialize, Default)]
    struct Item {
        #[serde(default, rename = "messagesAdded")]
        added: Vec<MsgWrap>,
        #[serde(default, rename = "messagesDeleted")]
        deleted: Vec<MsgWrap>,
        #[serde(default, rename = "labelsAdded")]
        labels_added: Vec<MsgWrap>,
        #[serde(default, rename = "labelsRemoved")]
        labels_removed: Vec<MsgWrap>,
    }
    #[derive(Deserialize)]
    struct HistoryList {
        #[serde(default)]
        history: Vec<Item>,
        #[serde(rename = "nextPageToken")]
        next_page_token: Option<String>,
        #[serde(default, rename = "historyId")]
        history_id: String,
    }
    if start.is_empty() || !start.chars().all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }
    let token = access_token(state).await?;
    let mut ids: Vec<String> = Vec::new();
    let mut page: Option<String> = None;
    let mut latest = String::new();
    for _ in 0..10 {
        let mut params: Vec<(&str, &str)> = vec![
            ("startHistoryId", start),
            ("maxResults", "100"),
            ("historyTypes", "messageAdded"),
            ("historyTypes", "messageDeleted"),
            ("historyTypes", "labelAdded"),
            ("historyTypes", "labelRemoved"),
        ];
        if let Some(p) = page.as_deref() {
            params.push(("pageToken", p));
        }
        let list: HistoryList = match get_json(&state.http, &token, "history", &params).await {
            Ok(l) => l,
            Err(e) if crate::offline::is_not_found(&e) => return Ok(None),
            Err(e) => return Err(e),
        };
        if !list.history_id.is_empty() {
            latest = list.history_id.clone();
        }
        for h in &list.history {
            for w in h.added.iter().chain(&h.deleted).chain(&h.labels_added).chain(&h.labels_removed) {
                let t = &w.message.thread_id;
                if !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric()) && !ids.contains(t) {
                    ids.push(t.clone());
                }
            }
        }
        match list.next_page_token {
            Some(n) => page = Some(n),
            None => {
                if latest.is_empty() {
                    latest = start.to_string();
                }
                return Ok(Some(HistoryChanges { thread_ids: ids, new_history_id: latest }));
            }
        }
    }
    Ok(None)
}

/// Odświeża podsumowanie jednego wątku. `Ok(None)` = wątek już nie istnieje (usunięty).
pub async fn refresh_summary(
    state: &AppState,
    id: &str,
    my_email: &str,
    f: &Filters<'_>,
) -> Result<Option<ThreadSummary>, String> {
    check_id(id)?;
    let token = access_token(state).await?;
    match fetch_thread(&state.http, &token, id, "metadata").await {
        Ok(raw) if raw.messages.is_empty() => Ok(None),
        Ok(raw) => Ok(Some(summarize(&raw, my_email, f))),
        Err(e) if crate::offline::is_not_found(&e) => Ok(None),
        Err(e) => Err(e),
    }
}

// ---------- Liczniki nieprzeczytanych i własne etykiety ----------

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LabelInfo {
    pub id: String,
    pub name: String,
    pub unread: u32,
}

pub struct MailStats {
    pub inbox_unread: u32,
    pub inbox_total: u32,
    pub spam_unread: u32,
    pub labels: Vec<LabelInfo>,
}

#[derive(Deserialize, Default)]
struct RawLabel {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default, rename = "threadsUnread")]
    threads_unread: u32,
    #[serde(default, rename = "threadsTotal")]
    threads_total: u32,
}

const MAX_LABELS: usize = 20;

/// Liczniki skrzynki i własnych etykiet (etykiety systemowe poza Odebrane i Spam są pomijane).
pub async fn mail_stats(state: &AppState) -> Result<MailStats, String> {
    #[derive(Deserialize, Default)]
    struct LabelList {
        #[serde(default)]
        labels: Vec<RawLabel>,
    }
    let token = access_token(state).await?;
    let list: LabelList = get_json(&state.http, &token, "labels", &[]).await?;
    let mut user: Vec<RawLabel> = list
        .labels
        .into_iter()
        .filter(|l| l.kind == "user" && crate::labels::check_user_label_id(&l.id).is_ok())
        .collect();
    user.sort_by_key(|l| l.name.to_lowercase());
    user.truncate(MAX_LABELS);

    let mut ids: Vec<String> = vec!["INBOX".to_string(), "SPAM".to_string()];
    ids.extend(user.iter().map(|l| l.id.clone()));
    let mut set = JoinSet::new();
    for id in ids {
        let http = state.http.clone();
        let tok = token.clone();
        set.spawn(async move {
            let r: Result<RawLabel, String> = get_json(&http, &tok, &format!("labels/{}", crate::labels::path_encode(&id)), &[]).await;
            (id, r)
        });
    }
    let mut got: std::collections::HashMap<String, RawLabel> = std::collections::HashMap::new();
    while let Some(r) = set.join_next().await {
        let (id, res) = r.map_err(|e| e.to_string())?;
        got.insert(id, res?);
    }
    let inbox = got.remove("INBOX").unwrap_or_default();
    let spam = got.remove("SPAM").unwrap_or_default();
    let labels = user
        .into_iter()
        .map(|l| LabelInfo { unread: got.get(&l.id).map(|g| g.threads_unread).unwrap_or(0), id: l.id, name: l.name })
        .collect();
    Ok(MailStats {
        inbox_unread: inbox.threads_unread,
        inbox_total: inbox.threads_total,
        spam_unread: spam.threads_unread,
        labels,
    })
}

pub async fn create_label(state: &AppState, name: &str) -> Result<LabelInfo, String> {
    let name = crate::labels::clean_label_name(name)?;
    let body = serde_json::json!({
        "name": name,
        "labelListVisibility": "labelShow",
        "messageListVisibility": "show",
    });
    let text = post_json(state, "labels", &body).await.map_err(|e| {
        if e.contains("409") {
            "Etykieta o tej nazwie już istnieje.".to_string()
        } else {
            e
        }
    })?;
    let raw: RawLabel = serde_json::from_str(&text).map_err(|e| format!("Nieoczekiwana odpowiedź Gmail: {e}"))?;
    crate::labels::check_user_label_id(&raw.id)?;
    Ok(LabelInfo { id: raw.id, name: raw.name, unread: 0 })
}

/// Zwraca identyfikator etykiety o danej nazwie – tworzy ją, jeśli jeszcze nie istnieje.
pub async fn ensure_label(state: &AppState, name: &str) -> Result<String, String> {
    let name = crate::labels::clean_label_name(name)?;
    match create_label(state, &name).await {
        Ok(l) => return Ok(l.id),
        Err(e) if e.contains("już istnieje") => {}
        Err(e) => return Err(e),
    }
    #[derive(Deserialize)]
    struct L {
        id: String,
        name: String,
    }
    #[derive(Deserialize)]
    struct LL {
        #[serde(default)]
        labels: Vec<L>,
    }
    let token = access_token(state).await?;
    let list: LL = get_json(&state.http, &token, "labels", &[]).await?;
    let found = list.labels.into_iter().find(|l| l.name.eq_ignore_ascii_case(&name)).ok_or("Nie znaleziono etykiety.")?;
    crate::labels::check_user_label_id(&found.id)?;
    Ok(found.id)
}

/// Importuje gotową wiadomość (RFC 822) do skrzynki z zachowaniem daty z nagłówka. Wiadomość dostaje
/// tylko podaną etykietę – nie trafia do Odebranych i nie jest oznaczona jako nieprzeczytana.
pub async fn import_raw(state: &AppState, rfc822: &[u8], label_id: &str) -> Result<String, String> {
    crate::labels::check_user_label_id(label_id)?;
    if rfc822.len() < 20 || rfc822.len() > 30 * 1024 * 1024 {
        return Err("Wiadomość jest pusta albo za duża (limit 30 MB).".to_string());
    }
    let meta = serde_json::json!({ "labelIds": [label_id] }).to_string();
    let text = upload(
        state,
        reqwest::Method::POST,
        "messages/import?internalDateSource=dateHeader&neverMarkSpam=true",
        &meta,
        rfc822,
    )
    .await?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("Nieoczekiwana odpowiedź Gmail: {e}"))?;
    Ok(v.get("id").and_then(|x| x.as_str()).unwrap_or_default().to_string())
}

/// Dodaje lub zdejmuje własną etykietę z wątku (etykiety systemowe są tu niedozwolone).
pub async fn set_thread_label(state: &AppState, id: &str, label: &str, add: bool) -> Result<(), String> {
    check_id(id)?;
    crate::labels::check_user_label_id(label)?;
    let body = if add {
        serde_json::json!({ "addLabelIds": [label], "removeLabelIds": [] })
    } else {
        serde_json::json!({ "addLabelIds": [], "removeLabelIds": [label] })
    };
    post_json(state, &format!("threads/{id}/modify"), &body).await?;
    Ok(())
}

// ---------- Zapis: wysyłanie, wersje robocze, akcje na wątkach ----------

const UPLOAD: &str = "https://gmail.googleapis.com/upload/gmail/v1/users/me";
const REAUTH_MSG: &str =
    "Brak uprawnień do wysyłania i zarządzania pocztą. Zaloguj się ponownie i zaakceptuj dostęp do poczty.";

fn check_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Niepoprawny identyfikator".to_string());
    }
    Ok(())
}

/// Zamienia odpowiedź HTTP na wynik; rozpoznaje brak uprawnień (stary token tylko do odczytu).
async fn read_response(resp: reqwest::Response) -> Result<String, String> {
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if status.as_u16() == 401 {
        return Err("Autoryzacja Gmail wygasła. Zaloguj się ponownie.".to_string());
    }
    if status.as_u16() == 403 && (text.contains("insufficient") || text.contains("SCOPE") || text.contains("scope")) {
        return Err(REAUTH_MSG.to_string());
    }
    if !status.is_success() {
        let short: String = text.chars().take(300).collect();
        return Err(format!("Gmail zwrócił błąd {status}: {short}"));
    }
    Ok(text)
}

/// Ciało zapytania multipart/related: część JSON z metadanymi + surowa wiadomość (message/rfc822).
pub fn multipart_related(boundary: &str, metadata_json: &str, rfc822: &[u8]) -> Vec<u8> {
    let mut b: Vec<u8> = Vec::with_capacity(rfc822.len() + metadata_json.len() + 256);
    b.extend_from_slice(
        format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{metadata_json}\r\n--{boundary}\r\nContent-Type: message/rfc822\r\n\r\n")
            .as_bytes(),
    );
    b.extend_from_slice(rfc822);
    b.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    b
}

async fn upload(
    state: &AppState,
    method: reqwest::Method,
    path: &str,
    metadata_json: &str,
    rfc822: &[u8],
) -> Result<String, String> {
    let token = access_token(state).await?;
    let boundary = format!("rustmail{}", crate::auth::random_urlsafe(12).replace(['-', '_'], "x"));
    let body = multipart_related(&boundary, metadata_json, rfc822);
    let resp = state
        .http
        .request(method, format!("{UPLOAD}/{path}{}uploadType=multipart", if path.contains('?') { '&' } else { '?' }))
        .bearer_auth(token)
        .header("Content-Type", format!("multipart/related; boundary={boundary}"))
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Brak połączenia z Gmail: {e}"))?;
    read_response(resp).await
}

async fn post_json(state: &AppState, path: &str, body: &serde_json::Value) -> Result<String, String> {
    let token = access_token(state).await?;
    let resp = state
        .http
        .post(format!("{API}/{path}"))
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("Brak połączenia z Gmail: {e}"))?;
    read_response(resp).await
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentInfo {
    pub id: String,
    #[serde(default, rename(deserialize = "threadId"))]
    pub thread_id: String,
}

#[derive(Deserialize)]
struct DraftResp {
    id: String,
}

fn thread_meta(thread_id: Option<&str>) -> Result<serde_json::Value, String> {
    match thread_id {
        Some(t) if !t.is_empty() => {
            check_id(t)?;
            Ok(serde_json::json!({ "threadId": t }))
        }
        _ => Ok(serde_json::json!({})),
    }
}

/// Wysyła gotową wiadomość. Nie ponawiamy automatycznie – ponowienie mogłoby wysłać duplikat.
pub async fn send_raw(state: &AppState, rfc822: &[u8], thread_id: Option<&str>) -> Result<SentInfo, String> {
    let meta = thread_meta(thread_id)?.to_string();
    let text = upload(state, reqwest::Method::POST, "messages/send", &meta, rfc822).await?;
    serde_json::from_str(&text).map_err(|e| format!("Nieoczekiwana odpowiedź Gmail: {e}"))
}

/// Tworzy (draft_id = None) lub nadpisuje wersję roboczą. Zwraca identyfikator wersji roboczej.
pub async fn save_draft(
    state: &AppState,
    draft_id: Option<&str>,
    rfc822: &[u8],
    thread_id: Option<&str>,
) -> Result<String, String> {
    let meta = serde_json::json!({ "message": thread_meta(thread_id)? }).to_string();
    let text = match draft_id {
        Some(id) if !id.is_empty() => {
            check_id(id)?;
            upload(state, reqwest::Method::PUT, &format!("drafts/{id}"), &meta, rfc822).await?
        }
        _ => upload(state, reqwest::Method::POST, "drafts", &meta, rfc822).await?,
    };
    let d: DraftResp = serde_json::from_str(&text).map_err(|e| format!("Nieoczekiwana odpowiedź Gmail: {e}"))?;
    Ok(d.id)
}

pub async fn send_draft(state: &AppState, draft_id: &str) -> Result<SentInfo, String> {
    check_id(draft_id)?;
    let text = post_json(state, "drafts/send", &serde_json::json!({ "id": draft_id })).await?;
    serde_json::from_str(&text).map_err(|e| format!("Nieoczekiwana odpowiedź Gmail: {e}"))
}

pub async fn delete_draft(state: &AppState, draft_id: &str) -> Result<(), String> {
    check_id(draft_id)?;
    let token = access_token(state).await?;
    let resp = state
        .http
        .delete(format!("{API}/drafts/{draft_id}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Brak połączenia z Gmail: {e}"))?;
    read_response(resp).await.map(|_| ())
}

/// Etykiety do dodania i usunięcia dla danej akcji na wątku. `None` = akcja nieznana lub specjalna.
pub fn action_labels(action: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    Some(match action {
        "archive" => (&[], &["INBOX"]),
        "unarchive" => (&["INBOX"], &[]),
        "read" => (&[], &["UNREAD"]),
        "unread" => (&["UNREAD"], &[]),
        "star" => (&["STARRED"], &[]),
        "unstar" => (&[], &["STARRED"]),
        "spam" => (&["SPAM"], &["INBOX"]),
        "notspam" => (&["INBOX"], &["SPAM"]),
        _ => return None,
    })
}

pub async fn thread_action(state: &AppState, id: &str, action: &str) -> Result<(), String> {
    check_id(id)?;
    if action == "trash" || action == "untrash" {
        post_json(state, &format!("threads/{id}/{action}"), &serde_json::json!({})).await?;
        return Ok(());
    }
    let (add, remove) = action_labels(action).ok_or_else(|| format!("Nieznana akcja: {action}"))?;
    post_json(
        state,
        &format!("threads/{id}/modify"),
        &serde_json::json!({ "addLabelIds": add, "removeLabelIds": remove }),
    )
    .await?;
    Ok(())
}

// ---------- Pobieranie załącznika ----------

const MAX_ATTACHMENT_BYTES: usize = 50 * 1024 * 1024;

/// Pobiera zawartość załącznika (wyłącznie na żądanie użytkownika).
pub async fn download_attachment(state: &AppState, message_id: &str, attachment_id: &str) -> Result<Vec<u8>, String> {
    check_id(message_id)?;
    let id_ok = !attachment_id.is_empty()
        && attachment_id.len() <= 2000
        && attachment_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !id_ok {
        return Err("Niepoprawny identyfikator załącznika".to_string());
    }
    #[derive(Deserialize)]
    struct AttachBody {
        #[serde(default)]
        data: String,
    }
    let token = access_token(state).await?;
    let body: AttachBody =
        get_json(&state.http, &token, &format!("messages/{message_id}/attachments/{attachment_id}"), &[]).await?;
    let bytes = URL_SAFE_NO_PAD
        .decode(body.data.trim_end_matches('='))
        .map_err(|_| "Nie można odczytać załącznika (błędne dane).".to_string())?;
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err("Załącznik jest zbyt duży.".to_string());
    }
    Ok(bytes)
}

// ---------- Odczyt wersji roboczej do dalszej edycji ----------

#[derive(Deserialize)]
struct DraftList {
    #[serde(default)]
    drafts: Vec<DraftRef>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct DraftRef {
    id: String,
    message: Option<DraftMsgRef>,
}

#[derive(Deserialize)]
struct DraftMsgRef {
    #[serde(default, rename = "threadId")]
    thread_id: String,
}

#[derive(Deserialize)]
struct RawDraft {
    id: String,
    message: Option<RawMessage>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftContent {
    pub draft_id: String,
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub html: String,
    pub in_reply_to: String,
    pub references: String,
}

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Zamienia zwykły tekst na prosty HTML (akapity i łamanie linii) – z zachowaniem bezpieczeństwa.
pub fn text_to_html(text: &str) -> String {
    html_escape(text.trim_end()).replace("\r\n", "\n").replace('\n', "<br>")
}

pub async fn open_draft(state: &AppState, thread_id: &str) -> Result<DraftContent, String> {
    check_id(thread_id)?;
    let token = access_token(state).await?;
    let mut page: Option<String> = None;
    let mut found: Option<String> = None;
    for _ in 0..5 {
        let mut params: Vec<(&str, &str)> = vec![("maxResults", "100")];
        if let Some(p) = page.as_deref() {
            params.push(("pageToken", p));
        }
        let list: DraftList = get_json(&state.http, &token, "drafts", &params).await?;
        if let Some(d) = list
            .drafts
            .iter()
            .find(|d| d.message.as_ref().map_or(false, |m| m.thread_id == thread_id))
        {
            found = Some(d.id.clone());
            break;
        }
        match list.next_page_token {
            Some(n) => page = Some(n),
            None => break,
        }
    }
    let draft_id = found.ok_or("Nie znaleziono wersji roboczej tej konwersacji.")?;
    let raw: RawDraft = get_json(&state.http, &token, &format!("drafts/{draft_id}"), &[("format", "full")]).await?;
    let msg = raw.message.ok_or("Wersja robocza jest pusta.")?;
    let payload = msg.payload.as_ref();
    let html = match payload.and_then(|p| find_part(p, "text/html")) {
        Some(part) => decode_part(part),
        None => text_to_html(&payload.and_then(|p| find_part(p, "text/plain")).map(decode_part).unwrap_or_default()),
    };
    Ok(DraftContent {
        draft_id: raw.id,
        to: hdr(&msg, "To"),
        cc: hdr(&msg, "Cc"),
        bcc: hdr(&msg, "Bcc"),
        subject: hdr(&msg, "Subject"),
        html,
        in_reply_to: hdr(&msg, "In-Reply-To"),
        references: hdr(&msg, "References"),
    })
}

// ---------- Przetwarzanie danych ----------

fn header<'a>(p: &'a Part, name: &str) -> Option<&'a str> {
    p.headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

pub fn split_address(from: &str) -> (String, String) {
    let from = from.trim();
    if let (Some(l), Some(r)) = (from.rfind('<'), from.rfind('>')) {
        if l < r {
            let email = from[l + 1..r].trim().to_string();
            let name = from[..l].trim().trim_matches('"').trim().to_string();
            return (name, email);
        }
    }
    (String::new(), from.to_string())
}

fn from_of(m: &RawMessage) -> (String, String) {
    m.payload
        .as_ref()
        .and_then(|p| header(p, "From"))
        .map(|f| split_address(&decode_mime_words(f)))
        .unwrap_or_default()
}

fn is_mine(m: &RawMessage, my_email: &str) -> bool {
    m.label_ids.iter().any(|l| l == "SENT") || from_of(m).1.eq_ignore_ascii_case(my_email)
}

fn subject_of(t: &RawThread) -> String {
    t.messages
        .first()
        .and_then(|m| m.payload.as_ref())
        .and_then(|p| header(p, "Subject"))
        .map(decode_mime_words)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "(bez tematu)".to_string())
}

/// Ocena jednej wiadomości. Przy ocenie samych nagłówków `body`, `html` i `attachments` są puste.
fn assess_message(
    m: &RawMessage,
    my_email: &str,
    rules: &Rules,
    body: &str,
    html: &str,
    attachments: &[AttachmentInfo],
) -> Assessment {
    let (name, email) = from_of(m);
    let reply_to = hdr(m, "Reply-To");
    let subject = hdr(m, "Subject");
    let auth_values: Vec<&str> = m
        .payload
        .as_ref()
        .map(|p| {
            p.headers
                .iter()
                .filter(|h| h.name.eq_ignore_ascii_case("Authentication-Results"))
                .map(|h| h.value.as_str())
                .collect()
        })
        .unwrap_or_default();
    phishing::assess(&Signals {
        from_name: &name,
        from_email: &email,
        reply_to: &reply_to,
        subject: &subject,
        auth: phishing::parse_auth_results(&auth_values),
        body,
        html,
        attachment_names: attachments.iter().map(|a| a.name.clone()).collect(),
        mine: is_mine(m, my_email),
        blocked: rules.is_blocked(&email),
        allowed: rules.is_allowed(&email),
    })
}

fn summarize(t: &RawThread, my_email: &str, f: &Filters<'_>) -> ThreadSummary {
    let last = t.messages.last();
    let mut names: Vec<String> = Vec::new();
    for m in &t.messages {
        let label = if is_mine(m, my_email) {
            "Ja".to_string()
        } else {
            let (n, e) = from_of(m);
            if n.is_empty() { e } else { n }
        };
        if !label.is_empty() && !names.contains(&label) {
            names.push(label);
        }
    }
    ThreadSummary {
        id: t.id.clone(),
        subject: subject_of(t),
        participants: names.into_iter().take(3).collect::<Vec<_>>().join(", "),
        snippet: decode_entities(last.map(|m| m.snippet.as_str()).unwrap_or("")),
        last_date_ms: last.map(|m| m.internal_date.parse::<i64>().unwrap_or(0)).unwrap_or(0),
        unread: t.messages.iter().any(|m| m.label_ids.iter().any(|l| l == "UNREAD")),
        starred: t.messages.iter().any(|m| m.label_ids.iter().any(|l| l == "STARRED")),
        count: t.messages.len(),
        risk: t
            .messages
            .iter()
            .map(|m| assess_message(m, my_email, f.rules, "", "", &[]))
            .max_by_key(|a| a.score)
            .map(Risk::from)
            .unwrap_or_else(|| Assessment::none().into()),
        dismissed: f.dismissed.contains(&t.id),
        labels: thread_labels(t),
        senders: {
            let mut v: Vec<String> = Vec::new();
            for m in t.messages.iter().filter(|m| !is_mine(m, my_email)) {
                let e = from_of(m).1;
                if !e.is_empty() && !v.contains(&e) {
                    v.push(e);
                }
            }
            v.join(" ")
        },
    }
}

/// Suma etykiet wszystkich wiadomości wątku (bez duplikatów, w kolejności pierwszego wystąpienia).
fn thread_labels(t: &RawThread) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in &t.messages {
        for l in &m.label_ids {
            if !out.contains(l) {
                out.push(l.clone());
            }
        }
    }
    out
}

/// Nagłówek (zdekodowany z MIME), pusty gdy go nie ma.
fn hdr(m: &RawMessage, name: &str) -> String {
    m.payload.as_ref().and_then(|p| header(p, name)).map(decode_mime_words).unwrap_or_default()
}

fn message_view(m: &RawMessage, my_email: &str, rules: &Rules) -> MessageView {
    let (name, email) = from_of(m);
    let mine = is_mine(m, my_email);
    let (body, html_only) = m.payload.as_ref().map(extract_body).unwrap_or_default();
    let html: String = m
        .payload
        .as_ref()
        .and_then(|p| find_part(p, "text/html"))
        .map(|part| decode_part(part).chars().take(400_000).collect())
        .unwrap_or_default();
    let mut attachments = Vec::new();
    if let Some(p) = &m.payload {
        collect_attachments(p, &mut attachments);
    }
    let risk: Risk = assess_message(m, my_email, rules, &body, &html, &attachments).into();
    MessageView {
        id: m.id.clone(),
        from: if mine {
            "Ja".to_string()
        } else if name.is_empty() {
            email.clone()
        } else {
            name
        },
        from_email: email,
        mine,
        date_ms: m.internal_date.parse::<i64>().unwrap_or(0),
        body: body.chars().take(100_000).collect(),
        html_only,
        html,
        attachments,
        to: hdr(m, "To"),
        cc: hdr(m, "Cc"),
        reply_to: hdr(m, "Reply-To"),
        message_id: m
            .payload
            .as_ref()
            .and_then(|p| header(p, "Message-ID").or_else(|| header(p, "Message-Id")))
            .unwrap_or("")
            .trim()
            .to_string(),
        references: hdr(m, "References"),
        risk,
    }
}

fn find_part<'a>(p: &'a Part, mime: &str) -> Option<&'a Part> {
    let has_data = p.body.as_ref().and_then(|b| b.data.as_ref()).is_some();
    if p.mime_type == mime && p.filename.is_empty() && has_data {
        return Some(p);
    }
    p.parts.iter().find_map(|c| find_part(c, mime))
}

fn charset_of(p: &Part) -> Option<String> {
    let ct = header(p, "Content-Type")?;
    let idx = ct.to_ascii_lowercase().find("charset=")?;
    let rest = &ct[idx + 8..];
    Some(rest.split(';').next()?.trim().trim_matches('"').to_string())
}

fn decode_part(p: &Part) -> String {
    let data = match p.body.as_ref().and_then(|b| b.data.as_deref()) {
        Some(d) => d,
        None => return String::new(),
    };
    let bytes = URL_SAFE_NO_PAD.decode(data.trim_end_matches('=')).unwrap_or_default();
    let enc = charset_of(p)
        .and_then(|cs| encoding_rs::Encoding::for_label(cs.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    enc.decode(&bytes).0.into_owned()
}

fn extract_body(p: &Part) -> (String, bool) {
    if let Some(part) = find_part(p, "text/plain") {
        return (decode_part(part), false);
    }
    if let Some(part) = find_part(p, "text/html") {
        return (strip_html(&decode_part(part)), true);
    }
    (String::new(), false)
}

fn collect_attachments(p: &Part, out: &mut Vec<AttachmentInfo>) {
    if !p.filename.is_empty() {
        out.push(AttachmentInfo {
            name: p.filename.clone(),
            mime: p.mime_type.clone(),
            size: p.body.as_ref().map(|b| b.size).unwrap_or(0),
            id: p.body.as_ref().and_then(|b| b.attachment_id.clone()),
        });
    }
    for c in &p.parts {
        collect_attachments(c, out);
    }
}

// ---------- Dekodowanie tekstu ----------

pub fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn remove_block(s: &str, tag: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::new();
    let mut pos = 0;
    while let Some(rel) = lower[pos..].find(&open) {
        let start = pos + rel;
        out.push_str(&s[pos..start]);
        match lower[start..].find(&close) {
            Some(end) => pos = start + end + close.len(),
            None => {
                pos = s.len();
                break;
            }
        }
    }
    out.push_str(&s[pos..]);
    out
}

/// Prosty, bezpieczny konwerter HTML na tekst (wynik jest wyświetlany jako zwykły tekst).
pub fn strip_html(html: &str) -> String {
    let cleaned = remove_block(&remove_block(html, "script"), "style");
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for ch in cleaned.chars() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
                let closing = tag.starts_with('/');
                let name = tag.trim_start_matches('/').to_ascii_lowercase();
                let first = name.split_whitespace().next().unwrap_or("");
                let block = matches!(first, "p" | "div" | "tr" | "li" | "h1" | "h2" | "h3");
                if (closing && block) || first.starts_with("br") {
                    out.push('\n');
                }
                tag.clear();
            } else {
                tag.push(ch);
            }
        } else if ch == '<' {
            in_tag = true;
        } else {
            out.push(ch);
        }
    }
    decode_entities(out.trim())
}

fn decode_q(t: &str) -> Vec<u8> {
    let b = t.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'_' => {
                out.push(b' ');
                i += 1;
            }
            b'=' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(v) => {
                        out.push(v);
                        i += 3;
                    }
                    None => {
                        out.push(b'=');
                        i += 1;
                    }
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Dekoduje nagłówki zakodowane wg RFC 2047, np. `=?UTF-8?B?...?=` (polskie znaki w temacie).
pub fn decode_mime_words(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("=?") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let mut it = after.splitn(3, '?');
        let (cs, enc, tail) = match (it.next(), it.next(), it.next()) {
            (Some(a), Some(b), Some(c)) => (a, b, c),
            _ => {
                out.push_str("=?");
                rest = after;
                continue;
            }
        };
        let end = match tail.find("?=") {
            Some(e) => e,
            None => {
                out.push_str("=?");
                rest = after;
                continue;
            }
        };
        let text = &tail[..end];
        let consumed = 2 + cs.len() + 1 + enc.len() + 1 + end + 2;
        let bytes = match enc.to_ascii_uppercase().as_str() {
            "B" => STANDARD.decode(text).ok(),
            "Q" => Some(decode_q(text)),
            _ => None,
        };
        match bytes {
            Some(b) => {
                let e = encoding_rs::Encoding::for_label(cs.as_bytes()).unwrap_or(encoding_rs::UTF_8);
                out.push_str(&e.decode(&b).0);
            }
            None => out.push_str(&rest[start..start + consumed]),
        }
        rest = &rest[start + consumed..];
        let trimmed = rest.trim_start();
        if trimmed.starts_with("=?") {
            rest = trimmed;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipart_body_has_two_parts_and_closing_boundary() {
        let b = String::from_utf8(multipart_related("BB", "{\"threadId\":\"abc\"}", b"Subject: x\r\n\r\nhi")).unwrap();
        assert!(b.starts_with("--BB\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{\"threadId\":\"abc\"}\r\n--BB\r\nContent-Type: message/rfc822\r\n\r\nSubject: x"));
        assert!(b.ends_with("hi\r\n--BB--\r\n"));
    }

    #[test]
    fn thread_actions_map_to_labels() {
        assert_eq!(action_labels("archive"), Some((&[][..], &["INBOX"][..])));
        assert_eq!(action_labels("spam"), Some((&["SPAM"][..], &["INBOX"][..])));
        assert_eq!(action_labels("read"), Some((&[][..], &["UNREAD"][..])));
        assert!(action_labels("delete_forever").is_none());
        assert!(action_labels("trash").is_none());
    }

    #[test]
    fn ids_are_checked() {
        assert!(check_id("18c3fA09").is_ok());
        assert!(check_id("").is_err());
        assert!(check_id("a/b").is_err());
        assert!(check_id("a?b=c").is_err());
    }

    #[test]
    fn plain_text_becomes_escaped_html() {
        assert_eq!(text_to_html("a<b>\nc & d\n"), "a&lt;b&gt;<br>c &amp; d");
    }

    #[test]
    fn splits_addresses() {
        assert_eq!(
            split_address("\"Anna K\" <anna@example.com>"),
            ("Anna K".to_string(), "anna@example.com".to_string())
        );
        assert_eq!(split_address("bob@example.com"), (String::new(), "bob@example.com".to_string()));
    }

    #[test]
    fn decodes_polish_mime_words() {
        assert_eq!(decode_mime_words("=?UTF-8?B?xYLEhQ==?="), "łą");
        assert_eq!(decode_mime_words("Re: =?UTF-8?Q?za=C5=BC=C3=B3=C5=82=C4=87?="), "Re: zażółć");
        assert_eq!(decode_mime_words("zwykły temat"), "zwykły temat");
    }

    #[test]
    fn strips_html_and_scripts() {
        let html = "<p>Hej&nbsp;<b>świecie</b></p><script>alert(1)</script><style>p{}</style>";
        let text = strip_html(html);
        assert_eq!(text, "Hej świecie");
        assert!(!text.contains("alert"));
    }

    #[test]
    fn maps_folders() {
        assert_eq!(folder_query("inbox").unwrap().unwrap().label, Some("INBOX"));
        assert!(folder_query("spam").unwrap().unwrap().spam_trash);
        assert!(folder_query("suspicious").unwrap().is_none());
        assert!(folder_query("nonsens").is_err());
    }

    #[test]
    fn decodes_base64url_body() {
        let p = Part {
            body: Some(Body { size: 5, data: Some("SGVsbG8".to_string()), attachment_id: None }),
            ..Default::default()
        };
        assert_eq!(decode_part(&p), "Hello");
    }

    #[test]
    fn summarizes_thread_from_gmail_json() {
        let raw: RawThread = serde_json::from_str(
            r#"{"id":"t1","messages":[
              {"id":"m1","labelIds":["INBOX","UNREAD"],"snippet":"Cze&#39;","internalDate":"1000",
               "payload":{"headers":[{"name":"From","value":"Anna <anna@example.com>"},
                                     {"name":"Subject","value":"=?UTF-8?B?xYLEhQ==?="}]}},
              {"id":"m2","labelIds":["SENT"],"snippet":"OK","internalDate":"2000",
               "payload":{"headers":[{"name":"From","value":"me@example.com"}]}}]}"#,
        )
        .unwrap();
        let none = HashSet::new();
        let rules = Rules::default();
        let s = summarize(&raw, "me@example.com", &Filters { dismissed: &none, rules: &rules });
        assert_eq!(s.risk.level, "none");
        assert!(!s.dismissed);
        assert_eq!(s.subject, "łą");
        assert_eq!(s.participants, "Anna, Ja");
        assert_eq!(s.snippet, "OK");
        assert_eq!(s.count, 2);
        assert_eq!(s.last_date_ms, 2000);
        assert!(s.unread);
        assert!(!s.starred);
    }

    #[test]
    fn builds_message_view_with_attachment() {
        let m: RawMessage = serde_json::from_str(
            r#"{"id":"m1","labelIds":["INBOX"],"internalDate":"5",
              "payload":{"mimeType":"multipart/mixed","headers":[{"name":"From","value":"Anna <anna@example.com>"}],
                "parts":[
                  {"mimeType":"text/plain","body":{"size":5,"data":"SGVsbG8"}},
                  {"mimeType":"application/pdf","filename":"a.pdf","body":{"size":10,"attachmentId":"x"}}]}}"#,
        )
        .unwrap();
        let v = message_view(&m, "me@example.com", &Rules::default());
        assert_eq!(v.body, "Hello");
        assert!(!v.html_only);
        assert!(!v.mine);
        assert_eq!(v.from, "Anna");
        assert_eq!(v.attachments.len(), 1);
        assert_eq!(v.attachments[0].name, "a.pdf");
    }
}

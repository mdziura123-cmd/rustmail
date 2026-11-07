//! Odczyt wątków przez oficjalne Gmail API (etap 2: wyłącznie odczyt).
//! Treść wiadomości jest traktowana jako niezaufana: zwracamy zwykły tekst, nigdy HTML.

use std::time::Duration;

use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::task::JoinSet;

use crate::auth::{access_token, AppState};

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
}

// ---------- Modele dla interfejsu ----------

#[derive(Serialize)]
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPage {
    pub threads: Vec<ThreadSummary>,
    pub next_page_token: Option<String>,
}

#[derive(Serialize)]
pub struct AttachmentInfo {
    pub name: String,
    pub mime: String,
    pub size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub id: String,
    pub from: String,
    pub from_email: String,
    pub mine: bool,
    pub date_ms: i64,
    pub body: String,
    pub html_only: bool,
    pub attachments: Vec<AttachmentInfo>,
}

#[derive(Serialize)]
pub struct ThreadDetail {
    pub id: String,
    pub subject: String,
    pub messages: Vec<MessageView>,
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
            &[("format", "metadata"), ("metadataHeaders", "From"), ("metadataHeaders", "Subject")],
        )
        .await
    } else {
        get_json(http, token, &path, &[("format", format)]).await
    }
}

pub async fn list_threads(
    state: &AppState,
    folder: &str,
    page: Option<String>,
    my_email: &str,
) -> Result<ThreadPage, String> {
    let Some(fq) = folder_query(folder)? else {
        return Ok(ThreadPage { threads: vec![], next_page_token: None });
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
    let list: ThreadList = get_json(&state.http, &token, "threads", &borrowed).await?;

    let mut set = JoinSet::new();
    for (i, t) in list.threads.iter().enumerate() {
        let http = state.http.clone();
        let tok = token.clone();
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
            Ok(raw) => threads.push(summarize(&raw, my_email)),
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
    Ok(ThreadPage { threads, next_page_token: list.next_page_token })
}

pub async fn get_thread(state: &AppState, id: &str, my_email: &str) -> Result<ThreadDetail, String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Niepoprawny identyfikator wątku".to_string());
    }
    let token = access_token(state).await?;
    let raw = fetch_thread(&state.http, &token, id, "full").await?;
    let subject = subject_of(&raw);
    let messages = raw.messages.iter().map(|m| message_view(m, my_email)).collect();
    Ok(ThreadDetail { id: raw.id, subject, messages })
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

fn summarize(t: &RawThread, my_email: &str) -> ThreadSummary {
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
    }
}

fn message_view(m: &RawMessage, my_email: &str) -> MessageView {
    let (name, email) = from_of(m);
    let mine = is_mine(m, my_email);
    let (body, html_only) = m.payload.as_ref().map(extract_body).unwrap_or_default();
    let mut attachments = Vec::new();
    if let Some(p) = &m.payload {
        collect_attachments(p, &mut attachments);
    }
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
        attachments,
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
            body: Some(Body { size: 5, data: Some("SGVsbG8".to_string()) }),
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
        let s = summarize(&raw, "me@example.com");
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
        let v = message_view(&m, "me@example.com");
        assert_eq!(v.body, "Hello");
        assert!(!v.html_only);
        assert!(!v.mine);
        assert_eq!(v.from, "Anna");
        assert_eq!(v.attachments.len(), 1);
        assert_eq!(v.attachments[0].name, "a.pdf");
    }
}

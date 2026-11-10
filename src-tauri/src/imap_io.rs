//! Poczta przez IMAP (WP, O2 i inne serwery) – osobny, niezależny od Gmaila moduł.
//! Zasady: połączenie tylko szyfrowane (TLS, port 993) z weryfikacją certyfikatu; hasło w systemowym sejfie;
//! każde polecenie z interfejsu otwiera własne krótkie połączenie (prosto i bez wiszących sesji).

use std::sync::Arc;
use std::time::Duration;

use mailparse::{DispositionType, MailHeaderMap, ParsedMail};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

use crate::imap_proto::{self as p, LIT};
use crate::auth::AppState;
use crate::notify;
use crate::secrets;
use crate::spam_io::{self, ImapCand, SpamState};
use crate::store::{OutboxItem, Store};
use tauri::State;

const KEY: &str = "imap_accounts";
const OLD_KEY: &str = "imap_account";
const IO_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_LITERAL: usize = 40 * 1024 * 1024;
const PAGE: u32 = 25;
/// Ile wiadomości na jedno sprawdzenie dostaje ocenę z początkiem treści (oszczędza łącze i serwer).
const PREVIEWS_PER_POLL: usize = 15;
const PREVIEW_BYTES: usize = 30_000;

#[derive(Serialize, Deserialize, Clone)]
struct Creds {
    email: String,
    password: String,
    host: String,
}

fn save_all(v: &[Creds]) -> Result<(), String> {
    if v.is_empty() {
        secrets::delete(KEY)
    } else {
        secrets::set(KEY, &serde_json::to_string(v).map_err(|e| e.to_string())?)
    }
}

/// Wszystkie zapisane konta IMAP (z jednorazową migracją z dawnego pojedynczego konta).
fn all() -> Result<Vec<Creds>, String> {
    if let Some(j) = secrets::get(KEY)? {
        return Ok(serde_json::from_str(&j).unwrap_or_default());
    }
    if let Some(j) = secrets::get(OLD_KEY)? {
        if let Ok(c) = serde_json::from_str::<Creds>(&j) {
            let v = vec![c];
            save_all(&v)?;
            let _ = secrets::delete(OLD_KEY);
            return Ok(v);
        }
    }
    Ok(Vec::new())
}

fn load(account: &str) -> Result<Creds, String> {
    all()?
        .into_iter()
        .find(|c| c.email.eq_ignore_ascii_case(account))
        .ok_or_else(|| "Nie znaleziono tego konta IMAP – zaloguj się ponownie.".to_string())
}

fn short(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(200).collect()
}

async fn tmo<T>(
    d: Duration,
    f: impl std::future::Future<Output = std::io::Result<T>>,
    what: &str,
) -> Result<T, String> {
    match tokio::time::timeout(d, f).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(format!("{what}: {e}")),
        Err(_) => Err(format!("{what}: przekroczono czas oczekiwania")),
    }
}

fn tls_config() -> Result<Arc<ClientConfig>, String> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let cfg = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("Konfiguracja TLS: {e}"))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}

struct Resp {
    head: String,
    lits: Vec<Vec<u8>>,
}

struct Reply {
    untagged: Vec<Resp>,
    ok: bool,
    text: String,
}

struct Conn {
    r: BufReader<ReadHalf<TlsStream<TcpStream>>>,
    w: WriteHalf<TlsStream<TcpStream>>,
    n: u32,
}

impl Conn {
    async fn read_resp(&mut self) -> Result<Resp, String> {
        let mut head = String::new();
        let mut lits: Vec<Vec<u8>> = Vec::new();
        loop {
            let mut buf: Vec<u8> = Vec::new();
            let n = tmo(IO_TIMEOUT, self.r.read_until(b'\n', &mut buf), "Odczyt z serwera").await?;
            if n == 0 {
                return Err("Serwer zamknął połączenie.".to_string());
            }
            if buf.len() > 2_000_000 {
                return Err("Serwer przysłał zbyt długą linię.".to_string());
            }
            let line = String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string();
            if let Some((pos, len)) = p::trailing_literal(&line) {
                if len > MAX_LITERAL {
                    return Err("Wiadomość jest za duża (limit 40 MB).".to_string());
                }
                head.push_str(&line[..pos]);
                head.push(LIT);
                let mut data = vec![0u8; len];
                tmo(IO_TIMEOUT, self.r.read_exact(&mut data), "Odczyt z serwera").await?;
                lits.push(data);
                continue;
            }
            head.push_str(&line);
            return Ok(Resp { head, lits });
        }
    }

    async fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        tmo(IO_TIMEOUT, self.w.write_all(bytes), "Zapis do serwera").await?;
        tmo(IO_TIMEOUT, self.w.flush(), "Zapis do serwera").await
    }

    async fn collect(&mut self, tag: &str) -> Result<Reply, String> {
        let prefix = format!("{tag} ");
        let mut untagged: Vec<Resp> = Vec::new();
        loop {
            let r = self.read_resp().await?;
            if let Some(rest) = r.head.strip_prefix(&prefix) {
                let (st, txt) = rest.split_once(' ').unwrap_or((rest, ""));
                return Ok(Reply { untagged, ok: st.eq_ignore_ascii_case("OK"), text: txt.to_string() });
            }
            if r.head.starts_with('+') {
                continue;
            }
            untagged.push(r);
        }
    }

    async fn cmd(&mut self, text: &str) -> Result<Reply, String> {
        self.n += 1;
        let tag = format!("A{:04}", self.n);
        self.send(format!("{tag} {text}\r\n").as_bytes()).await?;
        self.collect(&tag).await
    }

    /// Polecenie złożone z tekstu i literałów (wartości z polskimi znakami). Każdy literał jest wysyłany
    /// dopiero po zgłoszeniu gotowości przez serwer (`+`), a polecenie kończy się po ostatnim fragmencie.
    async fn ok_parts(&mut self, prefix: &str, parts: &[p::SPart]) -> Result<Reply, String> {
        self.n += 1;
        let tag = format!("A{:04}", self.n);
        let want = format!("{tag} ");
        let mut line = format!("{tag} {prefix}");
        for part in parts {
            match part {
                p::SPart::Text(t) => line.push_str(t),
                p::SPart::Lit(v) => {
                    line.push_str(&format!("{{{}}}\r\n", v.len()));
                    self.send(line.as_bytes()).await?;
                    loop {
                        let r = self.read_resp().await?;
                        if r.head.starts_with('+') {
                            break;
                        }
                        if r.head.starts_with(&want) {
                            return Err("Serwer odrzucił zapytanie.".to_string());
                        }
                    }
                    self.send(v.as_bytes()).await?;
                    line = String::new();
                }
            }
        }
        line.push_str("\r\n");
        self.send(line.as_bytes()).await?;
        let r = self.collect(&tag).await?;
        if r.ok {
            Ok(r)
        } else {
            Err(format!("Serwer odrzucił zapytanie: {}", short(&r.text)))
        }
    }

    async fn ok(&mut self, text: &str) -> Result<Reply, String> {
        let r = self.cmd(text).await?;
        if r.ok {
            Ok(r)
        } else {
            Err(format!("Serwer odrzucił polecenie: {}", short(&r.text)))
        }
    }

    async fn bye(mut self) {
        let _ = tokio::time::timeout(Duration::from_secs(5), self.cmd("LOGOUT")).await;
    }

    async fn select(&mut self, folder: &str) -> Result<u32, String> {
        let r = self.ok(&format!("SELECT {}", p::quote(folder)?)).await?;
        Ok(r.untagged.iter().filter_map(|u| p::parse_exists(&u.head)).last().unwrap_or(0))
    }
}

async fn tls_connect(host: &str, port: u16) -> Result<TlsStream<TcpStream>, String> {
    if !p::valid_host(host) {
        return Err("Niepoprawna nazwa serwera.".to_string());
    }
    let tcp = match tokio::time::timeout(Duration::from_secs(15), TcpStream::connect((host, port))).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(format!("Nie można połączyć z serwerem {host}: {e}")),
        Err(_) => return Err(format!("Serwer {host} nie odpowiada.")),
    };
    let name = ServerName::try_from(host.to_string()).map_err(|_| "Niepoprawna nazwa serwera.".to_string())?;
    TlsConnector::from(tls_config()?)
        .connect(name, tcp)
        .await
        .map_err(|e| format!("Błąd bezpiecznego połączenia (TLS) z {host}: {e}"))
}

async fn open(c: &Creds) -> Result<Conn, String> {
    let tls = tls_connect(&c.host, 993).await?;
    let (r, w) = tokio::io::split(tls);
    let mut conn = Conn { r: BufReader::new(r), w, n: 0 };
    let greet = conn.read_resp().await?;
    if !(greet.head.starts_with("* OK") || greet.head.starts_with("* PREAUTH")) {
        return Err("Serwer nie odpowiedział jak serwer IMAP.".to_string());
    }
    let login = format!("LOGIN {} {}", p::quote(&c.email)?, p::quote(&c.password)?);
    let r = conn.cmd(&login).await?;
    if !r.ok {
        return Err(format!(
            "Serwer odrzucił logowanie ({}). Sprawdź adres i hasło (w WP trzeba użyć hasła do aplikacji zewnętrznych) oraz czy w ustawieniach poczty włączono dostęp przez IMAP.",
            short(&r.text)
        ));
    }
    Ok(conn)
}

// ---------- Polecenia dla interfejsu ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImapAccount {
    pub email: String,
    pub host: String,
}

#[tauri::command]
pub fn imap_accounts() -> Result<Vec<ImapAccount>, String> {
    Ok(all()?.into_iter().map(|c| ImapAccount { email: c.email, host: c.host }).collect())
}

#[tauri::command]
pub async fn imap_login(email: String, password: String, host: String) -> Result<ImapAccount, String> {
    let email = email.trim().to_string();
    if !email.contains('@') || email.len() > 120 {
        return Err("Podaj poprawny adres e-mail.".to_string());
    }
    let host = if host.trim().is_empty() {
        p::default_host(&email).ok_or("Dla tej domeny nie znam serwera IMAP – wpisz jego adres w polu „Serwer IMAP”.")?.to_string()
    } else {
        host.trim().to_ascii_lowercase()
    };
    if !p::valid_host(&host) {
        return Err("Niepoprawna nazwa serwera IMAP.".to_string());
    }
    if password.is_empty() {
        return Err("Podaj hasło.".to_string());
    }
    let creds = Creds { email: email.clone(), password, host: host.clone() };
    let conn = open(&creds).await?;
    conn.bye().await;
    let mut list = all()?;
    list.retain(|c| !c.email.eq_ignore_ascii_case(&creds.email));
    list.push(creds);
    save_all(&list)?;
    Ok(ImapAccount { email, host })
}

#[tauri::command]
pub fn imap_logout(store: State<'_, Store>, account: String) -> Result<(), String> {
    let mut list = all()?;
    list.retain(|c| !c.email.eq_ignore_ascii_case(&account));
    save_all(&list)?;
    // Wiadomości czekające w kolejce tego konta nie zostają na dysku po wylogowaniu.
    let _ = store.purge_outbox(&outbox_key(&account));
    let _ = store.prio_purge(&format!("i:{account}:"));
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub name: String,
    pub label: String,
    pub role: String,
}

async fn list_folders(conn: &mut Conn) -> Result<Vec<Folder>, String> {
    let r = conn.ok("LIST \"\" \"*\"").await?;
    let mut out: Vec<Folder> = Vec::new();
    for u in &r.untagged {
        let Some(e) = p::parse_list(&u.head, &u.lits) else { continue };
        if e.attrs.iter().any(|a| a.eq_ignore_ascii_case("\\Noselect") || a.eq_ignore_ascii_case("\\NonExistent")) {
            continue;
        }
        if out.iter().any(|f| f.name == e.name) {
            continue;
        }
        let role = p::role_of(&e.name, &e.attrs).to_string();
        let label = if role == "inbox" { "Odebrane".to_string() } else { p::mutf7_decode(&e.name) };
        out.push(Folder { name: e.name, label, role });
    }
    let rank = |r: &str| match r {
        "inbox" => 0,
        "sent" => 1,
        "drafts" => 2,
        "spam" => 3,
        "trash" => 4,
        _ => 5,
    };
    out.sort_by(|a, b| rank(&a.role).cmp(&rank(&b.role)).then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase())));
    Ok(out)
}

#[tauri::command]
pub async fn imap_folders(account: String) -> Result<Vec<Folder>, String> {
    let creds = load(&account)?;
    let mut conn = open(&creds).await?;
    let res = list_folders(&mut conn).await;
    conn.bye().await;
    res
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub uid: u32,
    pub from: String,
    pub subject: String,
    pub date_ms: i64,
    pub unread: bool,
    pub starred: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub rows: Vec<Row>,
    pub next: Option<u32>,
    pub total: u32,
}

fn clip(s: String, max: usize) -> String {
    s.chars().filter(|c| !c.is_control() || *c == ' ').take(max).collect()
}

fn date_ms(v: Option<String>) -> i64 {
    v.and_then(|d| mailparse::dateparse(&d).ok()).map(|s| s * 1000).unwrap_or(0)
}

#[tauri::command]
pub async fn imap_list(account: String, folder: String, offset: u32) -> Result<Page, String> {
    let creds = load(&account)?;
    let mut conn = open(&creds).await?;
    let res = list_inner(&mut conn, &folder, offset).await;
    conn.bye().await;
    res
}

async fn fetch_rows(conn: &mut Conn, prefix: &str) -> Result<Vec<Row>, String> {
    let r = conn
        .ok(&format!("{prefix} (UID FLAGS INTERNALDATE BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE)])"))
        .await?;
    let mut rows: Vec<Row> = Vec::new();
    for u in &r.untagged {
        let Some(info) = p::parse_fetch(&u.head) else { continue };
        let Some(uid) = info.uid else { continue };
        if info.flags.iter().any(|f| f.eq_ignore_ascii_case("\\Deleted")) {
            continue;
        }
        let hdr = u.lits.first().map(|b| b.as_slice()).unwrap_or(&[]);
        let (h, _) = mailparse::parse_headers(hdr).unwrap_or((Vec::new(), 0));
        rows.push(Row {
            uid,
            from: clip(h.get_first_value("From").unwrap_or_default(), 120),
            subject: clip(h.get_first_value("Subject").unwrap_or_default(), 200),
            date_ms: date_ms(h.get_first_value("Date")),
            unread: !info.flags.iter().any(|f| f.eq_ignore_ascii_case("\\Seen")),
            starred: info.flags.iter().any(|f| f.eq_ignore_ascii_case("\\Flagged")),
        });
    }
    rows.sort_by(|a, b| b.uid.cmp(&a.uid));
    Ok(rows)
}

async fn list_inner(conn: &mut Conn, folder: &str, offset: u32) -> Result<Page, String> {
    let total = conn.select(folder).await?;
    if total == 0 || offset >= total {
        return Ok(Page { rows: Vec::new(), next: None, total });
    }
    let hi = total - offset;
    let lo = hi.saturating_sub(PAGE - 1).max(1);
    let rows = fetch_rows(conn, &format!("FETCH {lo}:{hi}")).await?;
    let next = if lo > 1 { Some(offset + PAGE) } else { None };
    Ok(Page { rows, next, total })
}

#[tauri::command]
pub async fn imap_search(account: String, folder: String, query: String) -> Result<Page, String> {
    let creds = load(&account)?;
    let mut conn = open(&creds).await?;
    let res = search_inner(&mut conn, &folder, &query).await;
    conn.bye().await;
    res
}

async fn search_inner(conn: &mut Conn, folder: &str, query: &str) -> Result<Page, String> {
    let q = query.trim();
    if q.chars().count() > 300 {
        return Err("Wpisz krótszą frazę.".to_string());
    }
    let parts = p::search_parts(q)?;
    conn.select(folder).await?;
    let prefix = if p::search_needs_utf8(&parts) { "UID SEARCH CHARSET UTF-8 " } else { "UID SEARCH " };
    let r = conn.ok_parts(prefix, &parts).await?;
    let mut uids: Vec<u32> = Vec::new();
    for u in &r.untagged {
        if let Some(rest) = u.head.strip_prefix("* SEARCH") {
            uids.extend(rest.split_whitespace().filter_map(|x| x.parse::<u32>().ok()));
        }
    }
    uids.sort_unstable_by(|a, b| b.cmp(a));
    uids.truncate(50);
    if uids.is_empty() {
        return Ok(Page { rows: Vec::new(), next: None, total: 0 });
    }
    let set = uids.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
    let rows = fetch_rows(conn, &format!("UID FETCH {set}")).await?;
    let total = rows.len() as u32;
    Ok(Page { rows, next: None, total })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Count {
    pub name: String,
    pub unseen: u32,
}

/// Liczba nieprzeczytanych w Odebranych i w Spamie.
#[tauri::command]
pub async fn imap_counts(account: String) -> Result<Vec<Count>, String> {
    let creds = load(&account)?;
    let mut conn = open(&creds).await?;
    let res = counts_inner(&mut conn).await;
    conn.bye().await;
    res
}

async fn counts_inner(conn: &mut Conn) -> Result<Vec<Count>, String> {
    let folders = list_folders(conn).await?;
    let mut out = Vec::new();
    for f in folders.iter().filter(|f| f.role == "inbox" || f.role == "spam") {
        let Ok(r) = conn.ok(&format!("STATUS {} (UNSEEN)", p::quote(&f.name)?)).await else { continue };
        for u in &r.untagged {
            if let Some(pos) = u.head.find("UNSEEN ") {
                let n: String = u.head[pos + 7..].chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(unseen) = n.parse() {
                    out.push(Count { name: f.name.clone(), unseen });
                }
            }
        }
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Att {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Msg {
    pub uid: u32,
    pub from: String,
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub date_ms: i64,
    pub message_id: String,
    pub references: String,
    pub reply_to: String,
    pub text: String,
    pub html: String,
    pub attachments: Vec<Att>,
    pub unread: bool,
    pub starred: bool,
}

/// Nazwa załącznika, jeśli ta część wiadomości jest załącznikiem.
fn att_name(part: &ParsedMail) -> Option<String> {
    let cd = part.get_content_disposition();
    let fname = cd.params.get("filename").or_else(|| part.ctype.params.get("name")).cloned();
    if cd.disposition == DispositionType::Attachment || fname.is_some() {
        Some(fname.unwrap_or_else(|| "załącznik".to_string()))
    } else {
        None
    }
}

/// Zawartość załącznika o danym numerze (w kolejności, w jakiej `walk` je wylicza).
fn find_att(part: &ParsedMail, counter: &mut usize, want: usize) -> Option<Vec<u8>> {
    if !part.subparts.is_empty() {
        for s in &part.subparts {
            if let Some(b) = find_att(s, counter, want) {
                return Some(b);
            }
        }
        return None;
    }
    if att_name(part).is_some() {
        if *counter == want {
            return part.get_body_raw().ok();
        }
        *counter += 1;
    }
    None
}

#[tauri::command]
pub async fn imap_download(
    app: tauri::AppHandle,
    account: String,
    folder: String,
    uid: u32,
    index: u32,
    name: String,
) -> Result<crate::commands::SavedFile, String> {
    if uid == 0 {
        return Err("Niepoprawny numer wiadomości.".to_string());
    }
    let creds = load(&account)?;
    let mut conn = open(&creds).await?;
    let res = fetch_raw(&mut conn, &folder, uid).await;
    conn.bye().await;
    let raw = res?;
    let mail = mailparse::parse_mail(&raw).map_err(|e| format!("Nie udało się odczytać wiadomości: {e}"))?;
    let mut counter = 0usize;
    let bytes = find_att(&mail, &mut counter, index as usize).ok_or("Nie znaleziono załącznika w tej wiadomości.")?;
    crate::commands::save_scanned(&app, &name, &bytes).await
}

async fn fetch_raw(conn: &mut Conn, folder: &str, uid: u32) -> Result<Vec<u8>, String> {
    conn.select(folder).await?;
    let r = conn.ok(&format!("UID FETCH {uid} (BODY.PEEK[])")).await?;
    for u in &r.untagged {
        if p::parse_fetch(&u.head).is_some() {
            if let Some(raw) = u.lits.first() {
                return Ok(raw.clone());
            }
        }
    }
    Err("Nie znaleziono wiadomości (mogła zostać usunięta).".to_string())
}

/// Początek treści wiadomości do lokalnej oceny spamu: pierwsze ~30 KB, bez zmiany flag (PEEK).
async fn fetch_preview(conn: &mut Conn, folder: &str, uid: u32) -> Result<spam_io::ImapDetail, String> {
    conn.select(folder).await?;
    let r = conn.ok(&format!("UID FETCH {uid} (BODY.PEEK[]<0.{PREVIEW_BYTES}>)")).await?;
    let raw = r
        .untagged
        .iter()
        .filter(|u| p::parse_fetch(&u.head).is_some())
        .find_map(|u| u.lits.first().cloned())
        .ok_or("Brak treści wiadomości.")?;
    let mail = mailparse::parse_mail(&raw).map_err(|e| e.to_string())?;
    let (mut text, mut html, mut atts) = (String::new(), String::new(), Vec::new());
    walk(&mail, &mut text, &mut html, &mut atts);
    if text.trim().is_empty() && !html.is_empty() {
        text = crate::gmail::strip_html(&html);
    }
    Ok(spam_io::ImapDetail {
        text: text.chars().take(3000).collect(),
        html: html.chars().take(40_000).collect(),
        reply_to: mail.headers.get_first_value("Reply-To").unwrap_or_default().chars().take(300).collect(),
        attachment_names: atts.into_iter().map(|a| a.name).take(20).collect(),
    })
}

fn walk(part: &ParsedMail, text: &mut String, html: &mut String, atts: &mut Vec<Att>) {
    if !part.subparts.is_empty() {
        for s in &part.subparts {
            walk(s, text, html, atts);
        }
        return;
    }
    let mime = part.ctype.mimetype.to_ascii_lowercase();
    if let Some(fname) = att_name(part) {
        let size = part.get_body_raw().map(|b| b.len()).unwrap_or(0);
        atts.push(Att { id: atts.len().to_string(), name: clip(fname, 150), mime, size });
    } else if mime == "text/plain" {
        if text.is_empty() {
            *text = part.get_body().unwrap_or_default();
        }
    } else if mime == "text/html" && html.is_empty() {
        *html = part.get_body().unwrap_or_default();
    }
}

#[tauri::command]
pub async fn imap_get(account: String, folder: String, uid: u32) -> Result<Msg, String> {
    if uid == 0 {
        return Err("Niepoprawny numer wiadomości.".to_string());
    }
    let creds = load(&account)?;
    let mut conn = open(&creds).await?;
    let res = get_inner(&mut conn, &folder, uid).await;
    conn.bye().await;
    res
}

async fn get_inner(conn: &mut Conn, folder: &str, uid: u32) -> Result<Msg, String> {
    conn.select(folder).await?;
    let r = conn.ok(&format!("UID FETCH {uid} (FLAGS BODY.PEEK[])")).await?;
    let mut found: Option<(Vec<String>, Vec<u8>)> = None;
    for u in &r.untagged {
        if let Some(info) = p::parse_fetch(&u.head) {
            if let Some(raw) = u.lits.first() {
                found = Some((info.flags, raw.clone()));
                break;
            }
        }
    }
    let (flags, raw) = found.ok_or("Nie znaleziono wiadomości (mogła zostać usunięta).")?;
    let mail = mailparse::parse_mail(&raw).map_err(|e| format!("Nie udało się odczytać wiadomości: {e}"))?;
    let (mut text, mut html, mut atts) = (String::new(), String::new(), Vec::new());
    walk(&mail, &mut text, &mut html, &mut atts);
    let h = &mail.headers;
    let msg = Msg {
        uid,
        from: clip(h.get_first_value("From").unwrap_or_default(), 200),
        to: clip(h.get_first_value("To").unwrap_or_default(), 500),
        cc: clip(h.get_first_value("Cc").unwrap_or_default(), 500),
        subject: clip(h.get_first_value("Subject").unwrap_or_default(), 300),
        date_ms: date_ms(h.get_first_value("Date")),
        message_id: clip(h.get_first_value("Message-ID").unwrap_or_default(), 300),
        references: clip(h.get_first_value("References").unwrap_or_default(), 2000),
        reply_to: clip(h.get_first_value("Reply-To").unwrap_or_default(), 300),
        text: text.chars().take(500_000).collect(),
        html: html.chars().take(1_500_000).collect(),
        attachments: atts,
        unread: !flags.iter().any(|f| f.eq_ignore_ascii_case("\\Seen")),
        starred: flags.iter().any(|f| f.eq_ignore_ascii_case("\\Flagged")),
    };
    // Oznaczamy jako przeczytaną (nieudane oznaczenie nie przeszkadza w czytaniu).
    let _ = conn.cmd(&format!("UID STORE {uid} +FLAGS (\\Seen)")).await;
    Ok(msg)
}

#[tauri::command]
pub async fn imap_act(account: String, folder: String, uid: u32, action: String) -> Result<ActOut, String> {
    if uid == 0 {
        return Err("Niepoprawny numer wiadomości.".to_string());
    }
    let creds = load(&account)?;
    let mut conn = open(&creds).await?;
    let res = act_inner(&mut conn, &folder, uid, &action).await;
    conn.bye().await;
    res
}

/// Gdzie trafiła wiadomość po przeniesieniu (do cofnięcia). Pola puste, gdy serwer nie podał nowego numeru.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ActOut {
    pub folder: Option<String>,
    pub uid: Option<u32>,
}

/// Przenosi wiadomość: najpierw kopia, dopiero po jej powodzeniu usunięcie oryginału – nic nie zginie po drodze.
async fn move_msg(conn: &mut Conn, folder: &str, uid: u32, target: &str) -> Result<Option<u32>, String> {
    conn.select(folder).await?;
    let r = conn.ok(&format!("UID COPY {uid} {}", p::quote(target)?)).await?;
    let new_uid = p::resp_uid(&r.text, "COPYUID");
    conn.ok(&format!("UID STORE {uid} +FLAGS (\\Deleted)")).await?;
    let _ = conn.cmd(&format!("UID EXPUNGE {uid}")).await;
    Ok(new_uid)
}

/// Przenosi wiadomość do wskazanego folderu (np. cofnięcie usunięcia).
#[tauri::command]
pub async fn imap_move(account: String, folder: String, uid: u32, target: String) -> Result<ActOut, String> {
    if uid == 0 {
        return Err("Niepoprawny numer wiadomości.".to_string());
    }
    let creds = load(&account)?;
    let mut conn = open(&creds).await?;
    let res = async {
        let new_uid = move_msg(&mut conn, &folder, uid, &target).await?;
        Ok(ActOut { folder: Some(target.clone()), uid: new_uid })
    }
    .await;
    conn.bye().await;
    res
}

async fn act_inner(conn: &mut Conn, folder: &str, uid: u32, action: &str) -> Result<ActOut, String> {
    let flag = |add: bool, f: &str| format!("UID STORE {uid} {}FLAGS ({f})", if add { "+" } else { "-" });
    match action {
        "read" | "unread" | "star" | "unstar" => {
            conn.select(folder).await?;
            let cmd = match action {
                "read" => flag(true, "\\Seen"),
                "unread" => flag(false, "\\Seen"),
                "star" => flag(true, "\\Flagged"),
                _ => flag(false, "\\Flagged"),
            };
            conn.ok(&cmd).await?;
            Ok(ActOut::default())
        }
        "trash" | "spam" | "archive" | "inbox" => {
            let want = action;
            let (name_pl, here_msg) = match want {
                "trash" => ("Kosz", "Ta wiadomość już jest w koszu."),
                "spam" => ("Spam", "Ta wiadomość już jest w Spamie."),
                "archive" => ("Archiwum", "Ta wiadomość już jest w archiwum."),
                _ => ("Odebrane", "Ta wiadomość już jest w Odebranych."),
            };
            let folders = list_folders(conn).await?;
            let here = folders.iter().find(|f| f.name == folder).map(|f| f.role.clone()).unwrap_or_default();
            if here == want {
                return Err(here_msg.to_string());
            }
            let target = folders
                .iter()
                .find(|f| f.role == want)
                .ok_or_else(|| format!("Nie znaleziono folderu: {name_pl}."))?
                .name
                .clone();
            let new_uid = move_msg(conn, folder, uid, &target).await?;
            Ok(ActOut { folder: Some(target), uid: new_uid })
        }
        _ => Err("Nieznana akcja.".to_string()),
    }
}

// ---------- Sprawdzanie poczty w tle: filtr spamu i powiadomienia ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PollOut {
    pub moved: usize,
    pub notified: usize,
    /// Ile nowych wiadomości doszło od poprzedniego sprawdzenia (do dźwięku).
    pub new_count: usize,
}

/// Co minutę dla każdego konta IMAP: filtr spamu na nieprzeczytanych z Odebranych, potem powiadomienie o nowych.
/// Przy pierwszym sprawdzeniu po starcie niczego nie ogłasza.
#[tauri::command]
pub async fn imap_poll(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    spam: tauri::State<'_, SpamState>,
    seen: tauri::State<'_, notify::ImapSeen>,
    account: String,
    notify_enabled: bool,
    show_content: bool,
) -> Result<PollOut, String> {
    let creds = load(&account)?;
    let mut conn = open(&creds).await?;
    let res = poll_inner(&mut conn, &app, &state, &spam, &seen, &account, notify_enabled, show_content).await;
    conn.bye().await;
    res
}

#[allow(clippy::too_many_arguments)]
async fn poll_inner(
    conn: &mut Conn,
    app: &tauri::AppHandle,
    state: &AppState,
    spam: &SpamState,
    seen: &notify::ImapSeen,
    account: &str,
    notify_enabled: bool,
    show_content: bool,
) -> Result<PollOut, String> {
    let folders = list_folders(conn).await?;
    let inbox = folders.iter().find(|f| f.role == "inbox").map(|f| f.name.clone()).unwrap_or_else(|| "INBOX".to_string());
    let page = list_inner(conn, &inbox, 0).await?;
    let unread: Vec<&Row> = page.rows.iter().filter(|r| r.unread).collect();
    // Oceniamy tylko wiadomości, których jeszcze nie oceniono. Dla każdej pobieramy początek treści
    // (do lokalnej oceny filtra); najwyżej PREVIEWS_PER_POLL na jedno sprawdzenie, reszta przy następnym.
    let filter_on = spam_io::imap_filter_active(app);
    let mut cand_rows: Vec<&Row> = Vec::new();
    let mut cands: Vec<ImapCand> = Vec::new();
    let mut previews = 0usize;
    for r in unread.iter().copied() {
        let key = format!("imap:{account}:{}", r.uid);
        if filter_on && spam_io::imap_seen(app, spam, &key) {
            continue;
        }
        let mut detail = None;
        if filter_on && !r.starred {
            if previews >= PREVIEWS_PER_POLL {
                continue;
            }
            previews += 1;
            detail = fetch_preview(conn, &inbox, r.uid).await.ok();
        }
        cand_rows.push(r);
        cands.push(ImapCand { key, from: r.from.clone(), subject: r.subject.clone(), starred: r.starred, detail });
    }
    let verdicts = spam_io::evaluate_imap(app, spam, &state.http, &cands).await;
    let mut moved_uids: Vec<u32> = Vec::new();
    for v in &verdicts {
        let uid = cand_rows[v.idx].uid;
        if act_inner(conn, &inbox, uid, "spam").await.is_ok() {
            spam_io::imap_moved(app, spam, &cands[v.idx], &v.reason);
            moved_uids.push(uid);
        }
    }
    let remaining: Vec<&&Row> = unread.iter().filter(|r| !moved_uids.contains(&r.uid)).collect();
    let ids: Vec<String> = remaining.iter().map(|r| r.uid.to_string()).collect();
    let new_ids = match seen.0.lock() {
        Ok(mut m) => notify::diff_new(m.entry(account.to_string()).or_insert(None), &ids),
        Err(_) => Vec::new(),
    };
    let mut notified = 0usize;
    let new_count = new_ids.len();
    if notify_enabled && !new_ids.is_empty() {
        let items: Vec<notify::Item> = remaining
            .iter()
            .filter(|r| new_ids.contains(&r.uid.to_string()))
            .map(|r| notify::Item { sender: &r.from, subject: &r.subject, risky: false })
            .collect();
        if let Some((title, body)) = notify::build_text(&items, show_content) {
            if notify::send(&title, &body).is_ok() {
                notified = items.len();
            }
        }
    }
    Ok(PollOut { moved: moved_uids.len(), notified, new_count })
}

// ---------- Wysyłanie (SMTP, port 465 z szyfrowaniem) ----------

struct Smtp {
    r: BufReader<ReadHalf<TlsStream<TcpStream>>>,
    w: WriteHalf<TlsStream<TcpStream>>,
}

impl Smtp {
    async fn reply(&mut self) -> Result<(u16, String), String> {
        let mut text = String::new();
        loop {
            let mut buf: Vec<u8> = Vec::new();
            let n = tmo(IO_TIMEOUT, self.r.read_until(b'\n', &mut buf), "Odczyt z serwera SMTP").await?;
            if n == 0 {
                return Err("Serwer SMTP zamknął połączenie.".to_string());
            }
            let line = String::from_utf8_lossy(&buf).trim_end().to_string();
            let Some((code, more, t)) = p::parse_smtp_line(&line) else {
                return Err("Niepoprawna odpowiedź serwera SMTP.".to_string());
            };
            text.push_str(&t);
            text.push('\n');
            if !more {
                return Ok((code, text));
            }
        }
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        tmo(IO_TIMEOUT, self.w.write_all(bytes), "Zapis do serwera SMTP").await?;
        tmo(IO_TIMEOUT, self.w.flush(), "Zapis do serwera SMTP").await
    }

    async fn expect(&mut self, cmd: &str, ok: &[u16]) -> Result<String, String> {
        self.write(format!("{cmd}\r\n").as_bytes()).await?;
        let (code, text) = self.reply().await?;
        if ok.contains(&code) {
            Ok(text)
        } else {
            Err(format!("Serwer SMTP odrzucił polecenie ({code}): {}", short(&text)))
        }
    }
}

fn safe_addr(a: &str) -> Result<&str, String> {
    if a.is_empty() || a.chars().any(|c| c.is_control() || c.is_whitespace() || c == '<' || c == '>') {
        return Err("Niepoprawny adres.".to_string());
    }
    Ok(a)
}

async fn smtp_send(c: &Creds, rcpts: &[String], data: &[u8]) -> Result<(), String> {
    let host = p::smtp_host(&c.host);
    let tls = tls_connect(&host, 465).await?;
    let (r, w) = tokio::io::split(tls);
    let mut s = Smtp { r: BufReader::new(r), w };
    let (code, _) = s.reply().await?;
    if code != 220 {
        return Err("Serwer SMTP nie odpowiedział poprawnie.".to_string());
    }
    let caps = s.expect("EHLO rustmail.local", &[250]).await?.to_ascii_uppercase();
    let b64 = |x: &[u8]| crate::compose::base64_encode(x);
    let login_err = |e: String| format!("Logowanie do serwera SMTP nie powiodło się ({e}). Sprawdź hasło i czy WP pozwala na wysyłanie przez programy pocztowe.");
    if caps.contains("PLAIN") {
        let mut payload = vec![0u8];
        payload.extend_from_slice(c.email.as_bytes());
        payload.push(0);
        payload.extend_from_slice(c.password.as_bytes());
        s.expect(&format!("AUTH PLAIN {}", b64(&payload)), &[235]).await.map_err(login_err)?;
    } else if caps.contains("LOGIN") {
        s.expect("AUTH LOGIN", &[334]).await.map_err(login_err)?;
        s.expect(&b64(c.email.as_bytes()), &[334]).await.map_err(login_err)?;
        s.expect(&b64(c.password.as_bytes()), &[235]).await.map_err(login_err)?;
    } else {
        return Err("Serwer SMTP nie obsługuje logowania PLAIN ani LOGIN.".to_string());
    }
    s.expect(&format!("MAIL FROM:<{}>", safe_addr(&c.email)?), &[250]).await?;
    for a in rcpts {
        s.expect(&format!("RCPT TO:<{}>", safe_addr(a)?), &[250, 251]).await?;
    }
    s.expect("DATA", &[354]).await?;
    s.write(&p::dot_stuff(data)).await?;
    let (code, text) = s.reply().await?;
    if code != 250 {
        return Err(format!("Serwer SMTP nie przyjął wiadomości ({code}): {}", short(&text)));
    }
    let _ = tokio::time::timeout(Duration::from_secs(5), s.expect("QUIT", &[221])).await;
    Ok(())
}

async fn append_sent(conn: &mut Conn, folder: &str, raw: &[u8]) -> Result<(), String> {
    append_msg(conn, folder, "\\Seen", raw).await.map(|_| ())
}

/// Dopisuje wiadomość do folderu. Zwraca jej numer UID, jeśli serwer go poda (rozszerzenie UIDPLUS).
async fn append_msg(conn: &mut Conn, folder: &str, flags: &str, raw: &[u8]) -> Result<Option<u32>, String> {
    conn.n += 1;
    let tag = format!("A{:04}", conn.n);
    conn.send(format!("{tag} APPEND {} ({flags}) {{{}}}\r\n", p::quote(folder)?, raw.len()).as_bytes()).await?;
    let want = format!("{tag} ");
    loop {
        let r = conn.read_resp().await?;
        if r.head.starts_with('+') {
            break;
        }
        if r.head.starts_with(&want) {
            return Err("Serwer odrzucił zapis do folderu Wysłane.".to_string());
        }
    }
    let mut data = raw.to_vec();
    data.extend_from_slice(b"\r\n");
    conn.send(&data).await?;
    let r = conn.collect(&tag).await?;
    if r.ok {
        Ok(p::resp_uid(&r.text, "APPENDUID"))
    } else {
        Err(format!("Serwer odrzucił zapis do folderu: {}", short(&r.text)))
    }
}

// ---------- Wersje robocze (folder „Wersje robocze” na serwerze) ----------

async fn drafts_folder(conn: &mut Conn) -> Result<String, String> {
    let folders = list_folders(conn).await?;
    folders.iter().find(|f| f.role == "drafts").map(|f| f.name.clone()).ok_or_else(|| "Nie znaleziono folderu Wersje robocze na serwerze.".to_string())
}

async fn delete_uid(conn: &mut Conn, folder: &str, uid: u32) -> Result<(), String> {
    conn.select(folder).await?;
    conn.ok(&format!("UID STORE {uid} +FLAGS (\\Deleted)")).await?;
    let _ = conn.cmd(&format!("UID EXPUNGE {uid}")).await;
    Ok(())
}

/// Zapisuje wersję roboczą na serwerze (zastępując poprzednią, jeśli podano jej numer). Zwraca numer UID (0 = nieznany).
#[tauri::command]
pub async fn imap_save_draft(account: String, msg: crate::commands::OutgoingDto, old_uid: Option<u32>) -> Result<u32, String> {
    let creds = load(&account)?;
    let body = crate::commands::prepare(&msg, &creds.email, false)?;
    let token = format!("{}-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0), std::process::id());
    let mut raw = format!("X-RustMail-Draft: {token}\r\n").into_bytes();
    raw.extend_from_slice(&body);
    let mut conn = open(&creds).await?;
    let res = async {
        let folder = drafts_folder(&mut conn).await?;
        let mut uid = append_msg(&mut conn, &folder, "\\Draft \\Seen", &raw).await?;
        if uid.is_none() {
            // Serwer nie podał numeru – znajdujemy zapisaną wersję po własnym nagłówku.
            conn.select(&folder).await?;
            let r = conn.ok(&format!("UID SEARCH HEADER X-RustMail-Draft {}", p::quote(&token)?)).await?;
            for u in &r.untagged {
                if let Some(rest) = u.head.strip_prefix("* SEARCH") {
                    uid = rest.split_whitespace().filter_map(|x| x.parse::<u32>().ok()).max();
                }
            }
        }
        if let (Some(old), Some(new)) = (old_uid, uid) {
            if old != new && old > 0 {
                let _ = delete_uid(&mut conn, &folder, old).await;
            }
        }
        Ok(uid.unwrap_or(0))
    }
    .await;
    conn.bye().await;
    res
}

/// Usuwa wersję roboczą z serwera (po wysłaniu albo odrzuceniu).
#[tauri::command]
pub async fn imap_delete_draft(account: String, uid: u32) -> Result<(), String> {
    if uid == 0 {
        return Ok(());
    }
    let creds = load(&account)?;
    let mut conn = open(&creds).await?;
    let res = async {
        let folder = drafts_folder(&mut conn).await?;
        delete_uid(&mut conn, &folder, uid).await
    }
    .await;
    conn.bye().await;
    res
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    pub warning: Option<String>,
}

/// Wysyła gotową wiadomość (zbudowaną przez `prepare`) i próbuje zapisać kopię w Wysłanych.
/// Odbiorcy są odczytywani z nagłówków To/Cc/Bcc, więc ta sama ścieżka obsługuje wysyłkę z kolejki.
async fn send_raw(creds: &Creds, raw: &[u8]) -> Result<SendResult, String> {
    let mut rcpts = crate::compose::parse_recipients(&p::recipient_headers(raw))?;
    rcpts.sort();
    rcpts.dedup();
    if rcpts.is_empty() {
        return Err("Podaj co najmniej jednego odbiorcę.".to_string());
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let nonce: String = crate::auth::random_urlsafe(12).chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let domain = creds.email.rsplit('@').next().unwrap_or("rustmail.local");
    let prepared = p::smtp_prepare(raw, &format!("{secs}.{nonce}@{domain}"), &p::rfc2822_date(secs));
    smtp_send(creds, &rcpts, &prepared).await?;
    // Kopia w folderze Wysłane (serwery SMTP same jej nie zapisują). Niepowodzenie nie cofa wysłania.
    let warning = match save_copy(creds, &prepared).await {
        Ok(()) => None,
        Err(e) => Some(format!("Wiadomość wysłano, ale nie zapisano kopii w Wysłanych: {e}")),
    };
    Ok(SendResult { warning })
}

#[tauri::command]
pub async fn imap_send(account: String, msg: crate::commands::OutgoingDto) -> Result<SendResult, String> {
    let creds = load(&account)?;
    let raw = crate::commands::prepare(&msg, &creds.email, true)?;
    send_raw(&creds, &raw).await
}

// ---------- Kolejka „Do wysłania” (gdy nie ma połączenia z serwerem) ----------
// Kolejka leży w zaszyfrowanej bazie lokalnej (ta sama tabela co dla Gmaila, osobny klucz konta).
// Do kolejki trafia tylko wiadomość, której wysyłka nie zaczęła się z powodu braku połączenia, i to dopiero po
// zgodzie użytkownika. Błąd po rozpoczęciu wysyłki nie jest ponawiany automatycznie (ryzyko dwukrotnego wysłania).

fn outbox_key(account: &str) -> String {
    format!("imap:{}", account.to_ascii_lowercase())
}

#[tauri::command]
pub async fn imap_outbox_queue(store: State<'_, Store>, account: String, msg: crate::commands::OutgoingDto) -> Result<i64, String> {
    let creds = load(&account)?;
    let raw = crate::commands::prepare(&msg, &creds.email, true)?;
    let subject: String = crate::compose::clean_header_value(&msg.subject).chars().take(200).collect();
    store.enqueue(
        &outbox_key(&account),
        &crate::commands::recipients_summary(&msg),
        &subject,
        None,
        msg.draft_id.as_deref().filter(|d| !d.is_empty()),
        &raw,
    )
}

#[tauri::command]
pub fn imap_outbox_list(store: State<'_, Store>, account: String) -> Result<Vec<OutboxItem>, String> {
    store.list_outbox(&outbox_key(&account))
}

#[tauri::command]
pub fn imap_outbox_retry(store: State<'_, Store>, account: String, id: i64) -> Result<(), String> {
    store.retry_outbox(&outbox_key(&account), id)
}

#[tauri::command]
pub fn imap_outbox_delete(store: State<'_, Store>, account: String, id: i64) -> Result<(), String> {
    store.delete_outbox(&outbox_key(&account), id)
}

/// Wysyła czekające wiadomości tego konta, od najstarszej. Brak połączenia przerywa pętlę (wiadomość wraca
/// do kolejki); inny błąd oznacza wiadomość jako nieudaną i nie jest ponawiany automatycznie.
#[tauri::command]
pub async fn imap_outbox_flush(store: State<'_, Store>, account: String) -> Result<crate::commands::FlushResult, String> {
    let key = outbox_key(&account);
    let mut out = crate::commands::FlushResult { sent: 0, failed: 0 };
    if store.list_outbox(&key)?.iter().all(|i| i.state != "queued") {
        return Ok(out);
    }
    let creds = load(&account)?;
    for _ in 0..60 {
        let Some(m) = store.claim_next(&key)? else { break };
        match send_raw(&creds, &m.raw).await {
            Ok(_) => {
                store.delete_outbox(&key, m.id)?;
                out.sent += 1;
                if let Some(uid) = m.draft_id.as_deref().and_then(|d| d.parse::<u32>().ok()) {
                    let _ = imap_delete_draft(account.clone(), uid).await;
                }
            }
            Err(e) if p::is_connect_error(&e) => {
                store.set_outbox_state(&key, m.id, "queued", None)?;
                break;
            }
            Err(e) => {
                store.set_outbox_state(&key, m.id, "failed", Some(&e))?;
                out.failed += 1;
            }
        }
    }
    Ok(out)
}

async fn save_copy(creds: &Creds, raw: &[u8]) -> Result<(), String> {
    let mut conn = open(creds).await?;
    let res = async {
        let folders = list_folders(&mut conn).await?;
        let sent = folders.iter().find(|f| f.role == "sent").ok_or("Nie znaleziono folderu Wysłane.")?;
        append_sent(&mut conn, &sent.name, raw).await
    }
    .await;
    conn.bye().await;
    res
}

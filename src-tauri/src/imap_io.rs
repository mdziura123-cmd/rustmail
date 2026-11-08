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
use crate::secrets;

const KEY: &str = "imap_account";
const IO_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_LITERAL: usize = 40 * 1024 * 1024;
const PAGE: u32 = 25;

#[derive(Serialize, Deserialize, Clone)]
struct Creds {
    email: String,
    password: String,
    host: String,
}

fn load() -> Result<Creds, String> {
    let j = secrets::get(KEY)?.ok_or("Nie zalogowano do poczty IMAP.")?;
    serde_json::from_str(&j).map_err(|_| "Zapisane dane konta IMAP są uszkodzone – zaloguj się ponownie.".to_string())
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

    async fn cmd(&mut self, text: &str) -> Result<Reply, String> {
        self.n += 1;
        let tag = format!("A{:04}", self.n);
        let line = format!("{tag} {text}\r\n");
        tmo(IO_TIMEOUT, self.w.write_all(line.as_bytes()), "Zapis do serwera").await?;
        tmo(IO_TIMEOUT, self.w.flush(), "Zapis do serwera").await?;
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

async fn open(c: &Creds) -> Result<Conn, String> {
    if !p::valid_host(&c.host) {
        return Err("Niepoprawna nazwa serwera IMAP.".to_string());
    }
    let tcp = match tokio::time::timeout(Duration::from_secs(15), TcpStream::connect((c.host.as_str(), 993u16))).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(format!("Nie można połączyć z serwerem {}: {e}", c.host)),
        Err(_) => return Err(format!("Serwer {} nie odpowiada.", c.host)),
    };
    let name = ServerName::try_from(c.host.clone()).map_err(|_| "Niepoprawna nazwa serwera.".to_string())?;
    let tls = TlsConnector::from(tls_config()?)
        .connect(name, tcp)
        .await
        .map_err(|e| format!("Błąd bezpiecznego połączenia (TLS) z {}: {e}", c.host))?;
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
pub fn imap_status() -> Result<Option<ImapAccount>, String> {
    match secrets::get(KEY)? {
        Some(j) => Ok(serde_json::from_str::<Creds>(&j).ok().map(|c| ImapAccount { email: c.email, host: c.host })),
        None => Ok(None),
    }
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
    let json = serde_json::to_string(&creds).map_err(|e| e.to_string())?;
    secrets::set(KEY, &json)?;
    Ok(ImapAccount { email, host })
}

#[tauri::command]
pub fn imap_logout() -> Result<(), String> {
    secrets::delete(KEY)
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
pub async fn imap_folders() -> Result<Vec<Folder>, String> {
    let creds = load()?;
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
pub async fn imap_list(folder: String, offset: u32) -> Result<Page, String> {
    let creds = load()?;
    let mut conn = open(&creds).await?;
    let res = list_inner(&mut conn, &folder, offset).await;
    conn.bye().await;
    res
}

async fn list_inner(conn: &mut Conn, folder: &str, offset: u32) -> Result<Page, String> {
    let total = conn.select(folder).await?;
    if total == 0 || offset >= total {
        return Ok(Page { rows: Vec::new(), next: None, total });
    }
    let hi = total - offset;
    let lo = hi.saturating_sub(PAGE - 1).max(1);
    let r = conn
        .ok(&format!("FETCH {lo}:{hi} (UID FLAGS INTERNALDATE BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE)])"))
        .await?;
    let mut rows: Vec<(u32, Row)> = Vec::new();
    for u in &r.untagged {
        let Some(info) = p::parse_fetch(&u.head) else { continue };
        let Some(uid) = info.uid else { continue };
        if info.flags.iter().any(|f| f.eq_ignore_ascii_case("\\Deleted")) {
            continue;
        }
        let hdr = u.lits.first().map(|b| b.as_slice()).unwrap_or(&[]);
        let (h, _) = mailparse::parse_headers(hdr).unwrap_or((Vec::new(), 0));
        rows.push((
            info.seq,
            Row {
                uid,
                from: clip(h.get_first_value("From").unwrap_or_default(), 120),
                subject: clip(h.get_first_value("Subject").unwrap_or_default(), 200),
                date_ms: date_ms(h.get_first_value("Date")),
                unread: !info.flags.iter().any(|f| f.eq_ignore_ascii_case("\\Seen")),
                starred: info.flags.iter().any(|f| f.eq_ignore_ascii_case("\\Flagged")),
            },
        ));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    let next = if lo > 1 { Some(offset + PAGE) } else { None };
    Ok(Page { rows: rows.into_iter().map(|(_, r)| r).collect(), next, total })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Att {
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
    pub text: String,
    pub html: String,
    pub attachments: Vec<Att>,
    pub unread: bool,
    pub starred: bool,
}

fn walk(part: &ParsedMail, text: &mut String, html: &mut String, atts: &mut Vec<Att>) {
    if !part.subparts.is_empty() {
        for s in &part.subparts {
            walk(s, text, html, atts);
        }
        return;
    }
    let mime = part.ctype.mimetype.to_ascii_lowercase();
    let cd = part.get_content_disposition();
    let fname = cd.params.get("filename").or_else(|| part.ctype.params.get("name")).cloned();
    let is_att = cd.disposition == DispositionType::Attachment || fname.is_some();
    if is_att {
        let size = part.get_body_raw().map(|b| b.len()).unwrap_or(0);
        atts.push(Att { name: clip(fname.unwrap_or_else(|| "załącznik".to_string()), 150), mime, size });
    } else if mime == "text/plain" {
        if text.is_empty() {
            *text = part.get_body().unwrap_or_default();
        }
    } else if mime == "text/html" && html.is_empty() {
        *html = part.get_body().unwrap_or_default();
    }
}

#[tauri::command]
pub async fn imap_get(folder: String, uid: u32) -> Result<Msg, String> {
    if uid == 0 {
        return Err("Niepoprawny numer wiadomości.".to_string());
    }
    let creds = load()?;
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
pub async fn imap_act(folder: String, uid: u32, action: String) -> Result<(), String> {
    if uid == 0 {
        return Err("Niepoprawny numer wiadomości.".to_string());
    }
    let creds = load()?;
    let mut conn = open(&creds).await?;
    let res = act_inner(&mut conn, &folder, uid, &action).await;
    conn.bye().await;
    res
}

async fn act_inner(conn: &mut Conn, folder: &str, uid: u32, action: &str) -> Result<(), String> {
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
            Ok(())
        }
        "trash" | "spam" => {
            let want = if action == "trash" { "trash" } else { "spam" };
            let folders = list_folders(conn).await?;
            let here = folders.iter().find(|f| f.name == folder).map(|f| f.role.clone()).unwrap_or_default();
            if here == want {
                return Err(if want == "trash" {
                    "Ta wiadomość już jest w koszu.".to_string()
                } else {
                    "Ta wiadomość już jest w Spamie.".to_string()
                });
            }
            let target = folders
                .iter()
                .find(|f| f.role == want)
                .ok_or(if want == "trash" { "Nie znaleziono folderu Kosz." } else { "Nie znaleziono folderu Spam." })?
                .name
                .clone();
            conn.select(folder).await?;
            // Najpierw kopia, dopiero po jej powodzeniu oznaczenie oryginału do usunięcia – nic nie zginie po drodze.
            conn.ok(&format!("UID COPY {uid} {}", p::quote(&target)?)).await?;
            conn.ok(&flag(true, "\\Deleted")).await?;
            let _ = conn.cmd(&format!("UID EXPUNGE {uid}")).await;
            Ok(())
        }
        _ => Err("Nieznana akcja.".to_string()),
    }
}

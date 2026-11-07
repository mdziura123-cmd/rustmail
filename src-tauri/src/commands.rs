//! Komendy Tauri udostępniane frontendowi. Każda jest jawnie zarejestrowana w lib.rs;
//! frontend nie ma dostępu do powłoki ani systemu plików.

use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::auth::{self, AppState, ClientCreds};
use crate::compose::{self, OutAttachment, Outgoing};
use crate::blocklist::{self, Rules};
use crate::{dismissed, filetype, gmail, remote, safety, scan, secrets, signature};

#[derive(Debug, Serialize, PartialEq)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub platform: &'static str,
    pub appimage: bool,
}

pub fn build_info(appimage: bool) -> AppInfo {
    AppInfo {
        name: "RustMail",
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        appimage,
    }
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    // Zmienna APPIMAGE jest ustawiana przez runtime AppImage.
    build_info(std::env::var_os("APPIMAGE").is_some())
}

#[derive(Serialize)]
pub struct AuthStatus {
    pub configured: bool,
    pub builtin: bool,
    pub email: Option<String>,
    /// Zalogowano, ale bez uprawnienia do wysyłania poczty – trzeba zalogować się ponownie.
    pub needs_reauth: bool,
}

#[tauri::command]
pub fn auth_status() -> Result<AuthStatus, String> {
    let configured = auth::load_creds()?.is_some();
    let email = if secrets::get(auth::KEY_REFRESH)?.is_some() {
        secrets::get(auth::KEY_EMAIL)?
    } else {
        None
    };
    let needs_reauth = email.is_some() && !auth::scope_allows_modify(secrets::get(auth::KEY_SCOPE)?.as_deref());
    Ok(AuthStatus { configured, builtin: auth::builtin_creds().is_some(), email, needs_reauth })
}

#[tauri::command]
pub fn save_client(client_id: String, client_secret: String) -> Result<(), String> {
    let id = client_id.trim();
    let secret = client_secret.trim();
    if !id.ends_with(".apps.googleusercontent.com") {
        return Err("Identyfikator klienta powinien kończyć się na .apps.googleusercontent.com".to_string());
    }
    if secret.is_empty() {
        return Err("Podaj klucz tajny klienta.".to_string());
    }
    auth::save_creds(&ClientCreds { client_id: id.to_string(), client_secret: secret.to_string() })
}

#[tauri::command]
pub async fn clear_client(state: State<'_, AppState>) -> Result<(), String> {
    secrets::delete(auth::KEY_CLIENT)?;
    secrets::delete(auth::KEY_REFRESH)?;
    secrets::delete(auth::KEY_EMAIL)?;
    secrets::delete(auth::KEY_SCOPE)?;
    *state.token.lock().await = None;
    Ok(())
}

#[tauri::command]
pub async fn sign_in(state: State<'_, AppState>) -> Result<String, String> {
    let creds = auth::load_creds()?.ok_or("Najpierw zapisz dane klienta Google.")?;
    let tr = auth::sign_in_flow(&state.http, &creds).await?;
    let refresh = tr.refresh_token.clone().ok_or(
        "Google nie zwrócił tokenu odświeżania. Usuń dostęp RustMail w koncie Google (Zabezpieczenia → Dostęp aplikacji innych firm) i spróbuj ponownie.",
    )?;
    let email = gmail::profile_email(&state.http, &tr.access_token).await?;
    secrets::set(auth::KEY_REFRESH, &refresh)?;
    secrets::set(auth::KEY_EMAIL, &email)?;
    secrets::set(auth::KEY_SCOPE, tr.scope.as_deref().unwrap_or(""))?;
    *state.token.lock().await = Some((tr.access_token, Instant::now() + Duration::from_secs(tr.expires_in)));
    Ok(email)
}

#[tauri::command]
pub async fn sign_out(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(token) = secrets::get(auth::KEY_REFRESH)? {
        auth::revoke(&state.http, &token).await;
    }
    secrets::delete(auth::KEY_REFRESH)?;
    secrets::delete(auth::KEY_EMAIL)?;
    secrets::delete(auth::KEY_SCOPE)?;
    *state.token.lock().await = None;
    Ok(())
}

fn config_file(app: &AppHandle, name: &str) -> Result<std::path::PathBuf, String> {
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join(name))
}

fn dismissed_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    config_file(app, "dismissed.txt")
}

fn load_rules(app: &AppHandle) -> Result<Rules, String> {
    Ok(Rules {
        blocked: blocklist::load_list(&config_file(app, "blocked.txt")?),
        allowed: blocklist::load_list(&config_file(app, "allowed.txt")?),
    })
}

#[tauri::command]
pub async fn list_threads(
    app: AppHandle,
    state: State<'_, AppState>,
    folder: String,
    page_token: Option<String>,
) -> Result<gmail::ThreadPage, String> {
    let email = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    let safe = dismissed::load(&dismissed_path(&app)?);
    let rules = load_rules(&app)?;
    gmail::list_threads(&state, &folder, page_token, &email, &gmail::Filters { dismissed: &safe, rules: &rules }).await
}

#[tauri::command]
pub async fn get_thread(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<gmail::ThreadDetail, String> {
    let email = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    let safe = dismissed::load(&dismissed_path(&app)?);
    let rules = load_rules(&app)?;
    gmail::get_thread(&state, &id, &email, &gmail::Filters { dismissed: &safe, rules: &rules }).await
}

#[derive(Serialize)]
pub struct SenderLists {
    pub blocked: Vec<String>,
    pub allowed: Vec<String>,
}

#[tauri::command]
pub fn get_sender_lists(app: AppHandle) -> Result<SenderLists, String> {
    let r = load_rules(&app)?;
    Ok(SenderLists { blocked: r.blocked, allowed: r.allowed })
}

fn list_file(app: &AppHandle, kind: &str) -> Result<std::path::PathBuf, String> {
    match kind {
        "blocked" => config_file(app, "blocked.txt"),
        "allowed" => config_file(app, "allowed.txt"),
        _ => Err("Nieznany rodzaj listy.".to_string()),
    }
}

/// Dodaje adres lub domenę do listy. Wpis nie może być jednocześnie zablokowany i zaufany.
#[tauri::command]
pub fn add_sender_rule(app: AppHandle, kind: String, entry: String) -> Result<String, String> {
    let path = list_file(&app, &kind)?;
    let other = list_file(&app, if kind == "blocked" { "allowed" } else { "blocked" })?;
    let added = blocklist::add(&path, &entry)?;
    blocklist::remove(&other, &added)?;
    Ok(added)
}

#[tauri::command]
pub fn remove_sender_rule(app: AppHandle, kind: String, entry: String) -> Result<(), String> {
    blocklist::remove(&list_file(&app, &kind)?, &entry)
}

/// Użytkownik uznaje wątek za bezpieczny (fałszywy alarm) albo cofa tę decyzję.
#[tauri::command]
pub fn set_thread_safe(app: AppHandle, id: String, safe: bool) -> Result<(), String> {
    let path = dismissed_path(&app)?;
    if safe { dismissed::add(&path, &id) } else { dismissed::remove(&path, &id) }
}

// ---------- Etap 3: redagowanie, wysyłanie, wersje robocze, akcje ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentDto {
    pub name: String,
    pub mime: String,
    pub data_base64: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingDto {
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub html: String,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub thread_id: Option<String>,
    pub draft_id: Option<String>,
    pub attachments: Vec<AttachmentDto>,
}

/// Składa wiadomość z danych z interfejsu. Przy wysyłaniu wymaga co najmniej jednego odbiorcy.
fn prepare(dto: &OutgoingDto, from: &str, require_to: bool) -> Result<Vec<u8>, String> {
    let to = compose::parse_recipients(&dto.to)?;
    let cc = compose::parse_recipients(&dto.cc)?;
    let bcc = compose::parse_recipients(&dto.bcc)?;
    if require_to && to.is_empty() && cc.is_empty() && bcc.is_empty() {
        return Err("Podaj co najmniej jednego odbiorcę.".to_string());
    }
    let mut attachments = Vec::new();
    for a in &dto.attachments {
        let data = STANDARD
            .decode(a.data_base64.trim())
            .map_err(|_| format!("Nie można odczytać załącznika „{}”.", compose::clean_header_value(&a.name)))?;
        attachments.push(OutAttachment { name: a.name.clone(), mime: a.mime.clone(), data });
    }
    let nonce: String = auth::random_urlsafe(12).chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let text = gmail::strip_html(&dto.html);
    compose::build_message(
        &Outgoing {
            from: from.to_string(),
            to,
            cc,
            bcc,
            subject: dto.subject.clone(),
            text,
            html: dto.html.clone(),
            in_reply_to: dto.in_reply_to.clone(),
            references: dto.references.clone(),
            attachments,
        },
        &format!("{nonce}00000000"),
    )
}

#[tauri::command]
pub async fn send_message(state: State<'_, AppState>, msg: OutgoingDto) -> Result<gmail::SentInfo, String> {
    let from = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    let raw = prepare(&msg, &from, true)?;
    let thread = msg.thread_id.as_deref();
    match msg.draft_id.as_deref().filter(|d| !d.is_empty()) {
        Some(draft) => {
            gmail::save_draft(&state, Some(draft), &raw, thread).await?;
            gmail::send_draft(&state, draft).await
        }
        None => gmail::send_raw(&state, &raw, thread).await,
    }
}

#[tauri::command]
pub async fn save_draft(state: State<'_, AppState>, msg: OutgoingDto) -> Result<String, String> {
    let from = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    let raw = prepare(&msg, &from, false)?;
    gmail::save_draft(&state, msg.draft_id.as_deref(), &raw, msg.thread_id.as_deref()).await
}

#[tauri::command]
pub async fn delete_draft(state: State<'_, AppState>, draft_id: String) -> Result<(), String> {
    gmail::delete_draft(&state, &draft_id).await
}

#[tauri::command]
pub async fn open_draft(state: State<'_, AppState>, thread_id: String) -> Result<gmail::DraftContent, String> {
    gmail::open_draft(&state, &thread_id).await
}

#[tauri::command]
pub async fn thread_action(state: State<'_, AppState>, id: String, action: String) -> Result<(), String> {
    gmail::thread_action(&state, &id, &action).await
}

// ---------- Etap 4: zdalna zawartość, linki, załączniki (tylko na żądanie użytkownika) ----------

#[tauri::command]
pub async fn fetch_image(url: String) -> Result<String, String> {
    remote::fetch_image(&url).await
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    remote::open_external(&url)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFile {
    /// Ścieżka zapisanego pliku; `None`, gdy skaner wykrył zagrożenie i plik NIE został zapisany.
    pub path: Option<String>,
    pub risky: bool,
    /// "clean" | "infected" | "unavailable" | "failed"
    pub scan: &'static str,
    pub scan_detail: String,
    /// Ostrzeżenie, gdy zawartość pliku nie zgadza się z jego nazwą.
    pub type_warning: Option<String>,
}

/// Pobiera załącznik, sprawdza typ po zawartości, skanuje ClamAV (jeśli jest) i dopiero wtedy zapisuje.
/// Plik z wykrytym zagrożeniem nie trafia na dysk w katalogu Pobrane.
#[tauri::command]
pub async fn download_attachment(
    app: AppHandle,
    state: State<'_, AppState>,
    message_id: String,
    attachment_id: String,
    name: String,
) -> Result<SavedFile, String> {
    let bytes = gmail::download_attachment(&state, &message_id, &attachment_id).await?;
    let risky = safety::risky_extension(&name);
    let type_warning = filetype::mismatch(&name, &bytes);

    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cache).map_err(|e| format!("Nie można utworzyć katalogu tymczasowego: {e}"))?;
    let nonce: String = auth::random_urlsafe(12).chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let tmp = cache.join(format!("scan-{nonce}.bin"));
    scan::write_private(&tmp, &bytes).map_err(|e| format!("Nie można utworzyć pliku do skanowania: {e}"))?;
    let tmp2 = tmp.clone();
    let status = tokio::task::spawn_blocking(move || scan::scan_file(&tmp2)).await.map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&tmp);
    let status = status?;

    let (scan_label, detail) = match &status {
        scan::ScanStatus::Clean => ("clean", String::new()),
        scan::ScanStatus::Infected(sig) => ("infected", sig.clone()),
        scan::ScanStatus::Unavailable => ("unavailable", String::new()),
        scan::ScanStatus::Failed(why) => ("failed", why.clone()),
    };
    if matches!(status, scan::ScanStatus::Infected(_)) {
        return Ok(SavedFile { path: None, risky, scan: scan_label, scan_detail: detail, type_warning });
    }
    let path = remote::save_download(&name, &bytes)?;
    Ok(SavedFile {
        path: Some(path.to_string_lossy().into_owned()),
        risky,
        scan: scan_label,
        scan_detail: detail,
        type_warning,
    })
}

fn signature_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join("signature.txt"))
}

#[tauri::command]
pub fn get_signature(app: AppHandle) -> Result<String, String> {
    Ok(signature::read_from(&signature_path(&app)?))
}

#[tauri::command]
pub fn set_signature(app: AppHandle, text: String) -> Result<(), String> {
    signature::write_to(&signature_path(&app)?, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_name_and_version() {
        let info = build_info(false);
        assert_eq!(info.name, "RustMail");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert!(!info.appimage);
    }

    #[test]
    fn appimage_flag_is_passed_through() {
        assert!(build_info(true).appimage);
    }

    fn dto() -> OutgoingDto {
        OutgoingDto {
            to: "anna@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Temat".into(),
            html: "<p>Witaj</p>".into(),
            in_reply_to: None,
            references: None,
            thread_id: None,
            draft_id: None,
            attachments: vec![],
        }
    }

    #[test]
    fn send_requires_a_recipient_but_draft_does_not() {
        let empty = OutgoingDto { to: String::new(), ..dto() };
        assert!(prepare(&empty, "ja@example.com", true).is_err());
        assert!(prepare(&empty, "ja@example.com", false).is_ok());
        assert!(prepare(&dto(), "ja@example.com", true).is_ok());
    }

    #[test]
    fn bad_recipient_or_attachment_is_rejected() {
        let bad = OutgoingDto { to: "to-nie-adres".into(), ..dto() };
        assert!(prepare(&bad, "ja@example.com", true).is_err());
        let att = OutgoingDto {
            attachments: vec![AttachmentDto { name: "a.txt".into(), mime: "text/plain".into(), data_base64: "!!!".into() }],
            ..dto()
        };
        assert!(prepare(&att, "ja@example.com", true).is_err());
    }

    #[test]
    fn rejects_malformed_client_id_before_touching_keyring() {
        assert!(save_client("zly-id".to_string(), "sekret".to_string()).is_err());
    }
}

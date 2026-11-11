//! Komendy Tauri udostępniane frontendowi. Każda jest jawnie zarejestrowana w lib.rs;
//! frontend nie ma dostępu do powłoki ani systemu plików.

use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::auth::{self, AppState, ClientCreds};
use crate::compose::{self, OutAttachment, Outgoing};
use crate::blocklist::{self, Rules};
use crate::store::{CacheStatus, Store};
use crate::notify::{self, Notifier};
use crate::spam_io::{self, SpamState};
use crate::accounts::{self, Account};
use crate::{contacts, dismissed, filetype, gmail, imap_io, offline, remote, safety, scan, secrets, signature};

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
pub async fn clear_client(state: State<'_, AppState>, store: State<'_, Store>) -> Result<(), String> {
    let _ = store.clear();
    secrets::delete(auth::KEY_CLIENT)?;
    secrets::delete(auth::KEY_ACCOUNTS)?;
    secrets::delete(auth::KEY_REFRESH)?;
    secrets::delete(auth::KEY_EMAIL)?;
    secrets::delete(auth::KEY_SCOPE)?;
    *state.token.lock().await = None;
    Ok(())
}

#[tauri::command]
pub async fn sign_in(state: State<'_, AppState>, store: State<'_, Store>, notifier: State<'_, Notifier>) -> Result<String, String> {
    if let Ok(mut s) = notifier.0.lock() {
        *s = notify::Seen::default();
    }
    let creds = auth::load_creds()?.ok_or("Najpierw zapisz dane klienta Google.")?;
    let tr = auth::sign_in_flow(&state.http, &creds).await?;
    let refresh = tr.refresh_token.clone().ok_or(
        "Google nie zwrócił tokenu odświeżania. Usuń dostęp RustMail w koncie Google (Zabezpieczenia → Dostęp aplikacji innych firm) i spróbuj ponownie.",
    )?;
    let email = gmail::profile_email(&state.http, &tr.access_token).await?;
    secrets::set(auth::KEY_REFRESH, &refresh)?;
    secrets::set(auth::KEY_EMAIL, &email)?;
    secrets::set(auth::KEY_SCOPE, tr.scope.as_deref().unwrap_or(""))?;
    let mut list = load_accounts()?;
    accounts::upsert(
        &mut list,
        Account { email: email.clone(), refresh: refresh.clone(), scope: tr.scope.clone().unwrap_or_default() },
    )?;
    save_accounts(&list)?;
    // Pamięć podręczna mogła należeć do innego konta.
    let _ = store.clear();
    *state.token.lock().await = Some((tr.access_token, Instant::now() + Duration::from_secs(tr.expires_in)));
    Ok(email)
}

#[tauri::command]
pub async fn sign_out(state: State<'_, AppState>, store: State<'_, Store>, notifier: State<'_, Notifier>) -> Result<(), String> {
    if let Ok(mut s) = notifier.0.lock() {
        *s = notify::Seen::default();
    }
    let _ = store.clear();
    let _ = store.backup_clear();
    let current = secrets::get(auth::KEY_EMAIL)?;
    if let Some(e) = current.as_deref() {
        let _ = store.prio_purge(&format!("g:{e}:"));
    }
    if let Some(token) = secrets::get(auth::KEY_REFRESH)? {
        auth::revoke(&state.http, &token).await;
    }
    secrets::delete(auth::KEY_REFRESH)?;
    secrets::delete(auth::KEY_EMAIL)?;
    secrets::delete(auth::KEY_SCOPE)?;
    *state.token.lock().await = None;
    // Wylogowane konto znika z rejestru; jeśli są inne, pierwsze z nich staje się aktywne.
    let mut list = load_accounts()?;
    if let Some(e) = current {
        accounts::remove(&mut list, &e);
    }
    save_accounts(&list)?;
    if let Some(next) = list.first() {
        activate(next)?;
    }
    Ok(())
}

// ---------- Etap 10: wiele kont ----------

fn load_accounts() -> Result<Vec<Account>, String> {
    Ok(secrets::get(auth::KEY_ACCOUNTS)?.map(|s| accounts::parse(&s)).unwrap_or_default())
}

fn save_accounts(list: &[Account]) -> Result<(), String> {
    if list.is_empty() {
        secrets::delete(auth::KEY_ACCOUNTS)
    } else {
        secrets::set(auth::KEY_ACCOUNTS, &accounts::serialize(list))
    }
}

/// Ustawia konto jako aktywne (wszystkie polecenia pracują na aktywnym koncie).
fn activate(a: &Account) -> Result<(), String> {
    secrets::set(auth::KEY_REFRESH, &a.refresh)?;
    secrets::set(auth::KEY_EMAIL, &a.email)?;
    secrets::set(auth::KEY_SCOPE, &a.scope)
}

/// Konto zalogowane przed etapem 10 nie ma jeszcze wpisu w rejestrze – dopisujemy je.
fn ensure_registered() -> Result<Vec<Account>, String> {
    let mut list = load_accounts()?;
    if let (Some(refresh), Some(email)) = (secrets::get(auth::KEY_REFRESH)?, secrets::get(auth::KEY_EMAIL)?) {
        if accounts::find(&list, &email).is_none() {
            let scope = secrets::get(auth::KEY_SCOPE)?.unwrap_or_default();
            accounts::upsert(&mut list, Account { email, refresh, scope })?;
            save_accounts(&list)?;
        }
    }
    Ok(list)
}

#[derive(Serialize)]
pub struct AccountDto {
    pub email: String,
    pub active: bool,
}

#[tauri::command]
pub fn list_accounts() -> Result<Vec<AccountDto>, String> {
    let list = ensure_registered()?;
    let active = secrets::get(auth::KEY_EMAIL)?.unwrap_or_default();
    Ok(list
        .into_iter()
        .map(|a| AccountDto { active: a.email.eq_ignore_ascii_case(&active), email: a.email })
        .collect())
}

/// Przełącza aktywne konto. Pamięć podręczna wątków jest czyszczona (dotyczy jednego konta naraz);
/// kolejka wysyłania zostaje i wysyła się dopiero po powrocie na konto, z którego została zapisana.
#[tauri::command]
pub async fn switch_account(
    state: State<'_, AppState>,
    store: State<'_, Store>,
    notifier: State<'_, Notifier>,
    spam: State<'_, SpamState>,
    email: String,
) -> Result<String, String> {
    let list = ensure_registered()?;
    let acc = accounts::find(&list, &email).ok_or("Nie ma takiego konta.")?.clone();
    activate(&acc)?;
    *state.token.lock().await = None;
    let _ = store.clear();
    if let Ok(mut s) = notifier.0.lock() {
        *s = notify::Seen::default();
    }
    if let Ok(mut k) = spam.last_key.lock() {
        *k = None;
    }
    Ok(acc.email)
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

/// Odświeża flagę „uznany za bezpieczny” w wynikach z pamięci podręcznej (zmienia się lokalnie).
fn refresh_dismissed(rows: &mut [gmail::ThreadSummary], safe: &std::collections::HashSet<String>) {
    for t in rows.iter_mut() {
        t.dismissed = safe.contains(&t.id);
    }
}

const PAGE: usize = 20;

#[tauri::command]
pub async fn list_threads(
    app: AppHandle,
    state: State<'_, AppState>,
    store: State<'_, Store>,
    folder: String,
    page_token: Option<String>,
) -> Result<gmail::ThreadPage, String> {
    let email = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    let safe = dismissed::load(&dismissed_path(&app)?);
    let rules = load_rules(&app)?;
    let filters = gmail::Filters { dismissed: &safe, rules: &rules };
    match gmail::list_threads(&state, &folder, page_token.clone(), &email, &filters).await {
        Ok(page) => {
            if store.ready() {
                let _ = store.put_summaries(&page.threads);
            }
            Ok(page)
        }
        Err(e) if offline::is_network_error(&e) && store.ready() => {
            let off = offline::parse_offset(page_token.as_deref());
            let mut rows = if folder == "suspicious" {
                let mut all = store.list_folder("suspicious", 0, 1000)?;
                refresh_dismissed(&mut all, &safe);
                all.into_iter().filter(|t| !t.dismissed).skip(off).take(PAGE + 1).collect()
            } else {
                let mut r = store.list_folder(&folder, off, PAGE + 1)?;
                refresh_dismissed(&mut r, &safe);
                r
            };
            let next = if rows.len() > PAGE {
                rows.truncate(PAGE);
                Some((off + PAGE).to_string())
            } else {
                None
            };
            Ok(gmail::ThreadPage { threads: rows, next_page_token: next, offline: true })
        }
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub async fn get_thread(
    app: AppHandle,
    state: State<'_, AppState>,
    store: State<'_, Store>,
    id: String,
) -> Result<gmail::ThreadDetail, String> {
    let email = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    let safe = dismissed::load(&dismissed_path(&app)?);
    let rules = load_rules(&app)?;
    match gmail::get_thread(&state, &id, &email, &gmail::Filters { dismissed: &safe, rules: &rules }).await {
        Ok(d) => {
            if store.ready() {
                let summary = gmail::summary_of_detail(&d);
                let _ = store.put_detail(&d, &summary);
            }
            Ok(d)
        }
        Err(e) if offline::is_network_error(&e) && store.ready() => match store.get_detail(&id) {
            Some(mut d) => {
                d.offline = true;
                d.dismissed = safe.contains(&d.id);
                Ok(d)
            }
            None => Err("Brak połączenia z Gmail, a ten wątek nie jest zapisany w pamięci offline.".to_string()),
        },
        Err(e) => Err(e),
    }
}

/// Wyszukiwanie: najpierw w Gmailu; bez sieci (i z włączonym trybem offline) w zapisanych wątkach.
#[tauri::command]
pub async fn search_threads(
    app: AppHandle,
    state: State<'_, AppState>,
    store: State<'_, Store>,
    query: String,
    page_token: Option<String>,
) -> Result<gmail::ThreadPage, String> {
    let email = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    let safe = dismissed::load(&dismissed_path(&app)?);
    let rules = load_rules(&app)?;
    let filters = gmail::Filters { dismissed: &safe, rules: &rules };
    match gmail::search_threads(&state, &query, page_token, &email, &filters).await {
        Ok(page) => {
            if store.ready() {
                let _ = store.put_summaries(&page.threads);
            }
            Ok(page)
        }
        Err(e) if offline::is_network_error(&e) && store.ready() => {
            let mut rows = store.search(&query, 50)?;
            refresh_dismissed(&mut rows, &safe);
            Ok(gmail::ThreadPage { threads: rows, next_page_token: None, offline: true })
        }
        Err(e) => Err(e),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    /// Fałsz, gdy tryb offline jest wyłączony (nic nie zrobiono).
    pub active: bool,
    /// Identyfikatory wątków zmienionych od ostatniej synchronizacji.
    pub changed: Vec<String>,
    /// Prawda, gdy pamięć podręczną opróżniono i zaczęto od nowa.
    pub reset: bool,
}

const MAX_SYNC_THREADS: usize = 100;

/// Synchronizacja przyrostowa przez Gmail History API: odświeża tylko zapisane wątki, które się zmieniły.
#[tauri::command]
pub async fn sync_now(app: AppHandle, state: State<'_, AppState>, store: State<'_, Store>) -> Result<SyncResult, String> {
    let idle = SyncResult { active: false, changed: vec![], reset: false };
    if !store.enabled() {
        return Ok(idle);
    }
    let restart = |h: String| -> Result<SyncResult, String> {
        store.clear_threads()?;
        store.set_history_id(&h)?;
        Ok(SyncResult { active: true, changed: vec![], reset: true })
    };
    let Some(start) = store.history_id() else {
        let h = gmail::current_history_id(&state).await?;
        return restart(h);
    };
    let Some(ch) = gmail::history_since(&state, &start).await? else {
        let h = gmail::current_history_id(&state).await?;
        return restart(h);
    };
    if ch.thread_ids.len() > MAX_SYNC_THREADS {
        return restart(ch.new_history_id);
    }
    let email = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    let safe = dismissed::load(&dismissed_path(&app)?);
    let rules = load_rules(&app)?;
    let filters = gmail::Filters { dismissed: &safe, rules: &rules };
    for id in &ch.thread_ids {
        if !store.has_thread(id) {
            continue;
        }
        // Błąd (np. brak sieci) przerywa synchronizację BEZ przesuwania punktu odniesienia: zmiany zostaną ponowione.
        match gmail::refresh_summary(&state, id, &email, &filters).await? {
            Some(s) => {
                store.put_summaries(&[s])?;
                store.invalidate_detail(id)?;
            }
            None => store.delete_thread(id)?,
        }
    }
    store.set_history_id(&ch.new_history_id)?;
    Ok(SyncResult { active: true, changed: ch.thread_ids, reset: false })
}

fn cache_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("cache"))
}

#[tauri::command]
pub fn cache_status(app: AppHandle, store: State<'_, Store>) -> Result<CacheStatus, String> {
    let path = cache_dir(&app)?.join("rustmail.enc.db");
    Ok(store.status(path.to_string_lossy().to_string()))
}

/// Włącza lub wyłącza tryb offline. Zawsze zaczyna od pustej bazy; wyłączenie usuwa zapisaną treść.
#[tauri::command]
pub fn set_offline_enabled(app: AppHandle, store: State<'_, Store>, enabled: bool) -> Result<CacheStatus, String> {
    if !store.available() {
        return Err("Nie udało się otworzyć lokalnej bazy danych.".to_string());
    }
    store.set_enabled(enabled)?;
    cache_status(app, store)
}

#[tauri::command]
pub fn clear_cache(app: AppHandle, store: State<'_, Store>) -> Result<CacheStatus, String> {
    store.clear()?;
    cache_status(app, store)
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
pub(crate) fn prepare(dto: &OutgoingDto, from: &str, require_to: bool) -> Result<Vec<u8>, String> {
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
    save_scanned(&app, &name, &bytes).await
}

/// Wspólna część pobierania: typ po zawartości, skan ClamAV, zapis w Pobranych.
pub(crate) async fn save_scanned(app: &AppHandle, name: &str, bytes: &[u8]) -> Result<SavedFile, String> {
    let (name, bytes) = (name.to_string(), bytes.to_vec());
    let risky = safety::risky_extension(&name);
    let type_warning = filetype::mismatch(&name, &bytes);

    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    crate::privfs::ensure_private_dir(&cache).map_err(|e| format!("Nie można utworzyć katalogu tymczasowego: {e}"))?;
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

// ---------- Etap 8: liczniki, powiadomienia, własne etykiety ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PollResult {
    pub inbox_unread: u32,
    pub spam_unread: u32,
    pub labels: Vec<gmail::LabelInfo>,
    /// Ile nowych wiadomości zgłoszono w powiadomieniu pulpitowym.
    pub notified: usize,
    /// Ile nowych wiadomości doszło od poprzedniego sprawdzenia (niezależnie od powiadomień; do dźwięku).
    pub new_count: usize,
    /// Ile wiadomości filtr spamu przeniósł przy tym sprawdzeniu.
    pub spam_moved: usize,
    /// Wiadomości oznaczone przez filtr jako podejrzane (czekają na decyzję użytkownika).
    pub spam_flagged: Vec<spam_io::FlagInfo>,
}

/// Odpytywanie poczty co minutę: liczniki + wykrycie nowych wiadomości w skrzynce. Powiadomienie pokazuje się tylko
/// o wiadomościach, które pojawiły się PO pierwszym sprawdzeniu (przy starcie nic nie jest ogłaszane).
#[tauri::command]
pub async fn poll_mail(
    app: AppHandle,
    state: State<'_, AppState>,
    notifier: State<'_, Notifier>,
    spam: State<'_, SpamState>,
    notify_enabled: bool,
    show_content: bool,
) -> Result<PollResult, String> {
    let stats = gmail::mail_stats(&state).await?;
    let key = (stats.inbox_unread, stats.inbox_total);
    // Filtr spamu działa przed powiadomieniami, żeby o wiadomościach przeniesionych do Spamu nie powiadamiać.
    let mut spam_moved = 0usize;
    if let Some(my_email) = secrets::get(auth::KEY_EMAIL)? {
        spam_moved = spam_io::auto_scan(&app, &spam, &state, &my_email, key).await.moved;
    }
    let unchanged = notifier.0.lock().map(|s| s.counts == Some(key) && s.ids.is_some()).unwrap_or(false);
    let mut notified = 0usize;
    let mut new_count = 0usize;
    if !unchanged {
        let email = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
        let safe = dismissed::load(&dismissed_path(&app)?);
        let rules = load_rules(&app)?;
        let page = gmail::list_threads(&state, "unread", None, &email, &gmail::Filters { dismissed: &safe, rules: &rules }).await?;
        let ids: Vec<String> = page.threads.iter().map(|t| t.id.clone()).collect();
        let new_ids = match notifier.0.lock() {
            Ok(mut s) => {
                let n = notify::diff_new(&mut s.ids, &ids);
                s.counts = Some(key);
                n
            }
            Err(_) => Vec::new(),
        };
        new_count = new_ids.len();
        if notify_enabled && !new_ids.is_empty() {
            let items: Vec<notify::Item> = page
                .threads
                .iter()
                .filter(|t| new_ids.contains(&t.id))
                .map(|t| notify::Item { sender: &t.participants, subject: &t.subject, risky: t.risk.level != "none" && !t.dismissed })
                .collect();
            if let Some((title, body)) = notify::build_text(&items, show_content) {
                if notify::send(&title, &body).is_ok() {
                    notified = items.len();
                }
            }
        }
    }
    Ok(PollResult {
        inbox_unread: stats.inbox_unread,
        spam_unread: stats.spam_unread,
        labels: stats.labels,
        notified,
        new_count,
        spam_moved,
        spam_flagged: spam_io::open_flags(&app),
    })
}

#[tauri::command]
pub async fn import_label(state: State<'_, AppState>, name: String) -> Result<String, String> {
    gmail::ensure_label(&state, &name).await
}

#[tauri::command]
pub async fn import_message(
    state: State<'_, AppState>,
    data_base64: String,
    label_id: String,
) -> Result<String, String> {
    let raw = STANDARD.decode(data_base64.trim()).map_err(|_| "Niepoprawne dane wiadomości.".to_string())?;
    gmail::import_raw(&state, &raw, &label_id).await
}

#[tauri::command]
pub async fn create_label(state: State<'_, AppState>, name: String) -> Result<gmail::LabelInfo, String> {
    gmail::create_label(&state, &name).await
}

#[tauri::command]
pub async fn set_thread_label(state: State<'_, AppState>, id: String, label: String, add: bool) -> Result<(), String> {
    gmail::set_thread_label(&state, &id, &label, add).await
}

/// Własne powiadomienie z interfejsu (np. powrót odłożonej wiadomości).
#[tauri::command]
pub fn show_notification(title: String, body: String) -> Result<(), String> {
    notify::send(&title, &body)
}

#[tauri::command]
pub fn test_notification() -> Result<(), String> {
    notify::send("RustMail", "Powiadomienia działają.")
}

// ---------- Etap 9: kolejka wysyłania ----------

/// Podsumowanie odbiorców do listy kolejki (adresy z pól Do/DW/UDW, najwyżej 3 + liczba pozostałych).
pub(crate) fn recipients_summary(dto: &OutgoingDto) -> String {
    let all: Vec<String> = [&dto.to, &dto.cc, &dto.bcc]
        .iter()
        .flat_map(|f| f.split([',', ';']).map(|s| s.trim().to_string()).collect::<Vec<_>>())
        .filter(|s| !s.is_empty())
        .collect();
    let mut shown: Vec<String> = all.iter().take(3).map(|s| compose::clean_header_value(s)).collect();
    if all.len() > 3 {
        shown.push(format!("i {} więcej", all.len() - 3));
    }
    shown.join(", ")
}

/// Zapisuje wiadomość w kolejce, gdy nie ma sieci. Wywoływane dopiero po zgodzie użytkownika:
/// wiadomość trafia na dysk w postaci niezaszyfrowanej (jak cała lokalna baza).
#[tauri::command]
pub fn queue_message(store: State<'_, Store>, msg: OutgoingDto) -> Result<i64, String> {
    let from = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    let raw = prepare(&msg, &from, true)?;
    let subject: String = compose::clean_header_value(&msg.subject).chars().take(200).collect();
    store.enqueue(
        &from,
        &recipients_summary(&msg),
        &subject,
        msg.thread_id.as_deref().filter(|t| !t.is_empty()),
        msg.draft_id.as_deref().filter(|d| !d.is_empty()),
        &raw,
    )
}

#[tauri::command]
pub fn list_outbox(store: State<'_, Store>) -> Result<Vec<crate::store::OutboxItem>, String> {
    let from = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    store.list_outbox(&from)
}

#[tauri::command]
pub fn retry_outbox(store: State<'_, Store>, id: i64) -> Result<(), String> {
    let from = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    store.retry_outbox(&from, id)
}

#[tauri::command]
pub fn delete_outbox(store: State<'_, Store>, id: i64) -> Result<(), String> {
    let from = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    store.delete_outbox(&from, id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlushResult {
    pub sent: usize,
    pub failed: usize,
}

/// Wysyła czekające wiadomości, od najstarszej. Brak sieci przerywa pętlę (wiadomość wraca do kolejki);
/// inny błąd oznacza wiadomość jako nieudaną i NIE jest ponawiany automatycznie.
#[tauri::command]
pub async fn flush_outbox(state: State<'_, AppState>, store: State<'_, Store>) -> Result<FlushResult, String> {
    let from = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    let mut out = FlushResult { sent: 0, failed: 0 };
    // Górny limit przebiegu chroni przed pętlą, gdyby coś nieoczekiwanie wracało do kolejki.
    for _ in 0..60 {
        let Some(m) = store.claim_next(&from)? else { break };
        let thread = m.thread_id.as_deref();
        let res = match m.draft_id.as_deref() {
            Some(d) => match gmail::save_draft(&state, Some(d), &m.raw, thread).await {
                Ok(_) => gmail::send_draft(&state, d).await.map(|_| ()),
                Err(e) => Err(e),
            },
            None => gmail::send_raw(&state, &m.raw, thread).await.map(|_| ()),
        };
        match res {
            Ok(()) => {
                store.delete_outbox(&from, m.id)?;
                out.sent += 1;
            }
            Err(e) if offline::is_network_error(&e) => {
                store.set_outbox_state(&from, m.id, "queued", None)?;
                break;
            }
            Err(e) => {
                store.set_outbox_state(&from, m.id, "failed", Some(&e))?;
                out.failed += 1;
            }
        }
    }
    Ok(out)
}


// ---------- Filtr spamu ----------

fn my_email() -> Result<String, String> {
    secrets::get(auth::KEY_EMAIL)?.ok_or_else(|| "Nie jesteś zalogowany.".to_string())
}

#[tauri::command]
pub async fn spam_get_config(app: AppHandle) -> Result<spam_io::Config, String> {
    Ok(spam_io::load_config(&app))
}

#[tauri::command]
pub async fn spam_set_config(app: AppHandle, spam: State<'_, SpamState>, config: spam_io::Config) -> Result<spam_io::Config, String> {
    spam_io::save_config(&app, &spam, config)
}

#[tauri::command]
pub async fn spam_status(app: AppHandle, spam: State<'_, SpamState>) -> Result<spam_io::Status, String> {
    Ok(spam_io::status(&app, &spam))
}

/// Ustawia flagę ważności wiadomości (0 = brak, 1 = bardzo ważne, 2 = ważne, 3 = mniej ważne).
#[tauri::command]
pub fn prio_set(store: State<'_, Store>, key: String, level: u8) -> Result<(), String> {
    store.prio_set(&key, level)
}

#[tauri::command]
pub fn prio_get(store: State<'_, Store>, keys: Vec<String>) -> Result<std::collections::HashMap<String, u8>, String> {
    Ok(store.prio_get(&keys)?.into_iter().collect())
}

/// Kopia robocza pisanej wiadomości trafia do zaszyfrowanej bazy. Limit rozmiaru chroni bazę.
#[tauri::command]
pub fn compose_backup_set(store: State<'_, Store>, json: String) -> Result<(), String> {
    if json.len() > 5_000_000 {
        return Err("Kopia robocza jest zbyt duża.".to_string());
    }
    store.backup_set(&json)
}

#[tauri::command]
pub fn compose_backup_get(store: State<'_, Store>) -> Option<String> {
    store.backup_get()
}

#[tauri::command]
pub fn compose_backup_clear(store: State<'_, Store>) -> Result<(), String> {
    store.backup_clear()
}

/// Usuwa dziennik filtra spamu i wytrenowany model (dane lokalne, bez szyfrowania).
#[tauri::command]
pub fn spam_wipe_data(app: AppHandle, spam: State<'_, SpamState>) -> Result<(), String> {
    spam_io::wipe_data(&app, &spam)
}

#[tauri::command]
pub async fn spam_log(app: AppHandle) -> Result<Vec<spam_io::LogEntry>, String> {
    Ok(spam_io::read_log(&app))
}

/// Ręczne sprawdzenie skrzynki odbiorczej (do 100 ostatnich wątków, także przeczytanych).
#[tauri::command]
pub async fn spam_scan_now(app: AppHandle, state: State<'_, AppState>, spam: State<'_, SpamState>) -> Result<spam_io::ScanOutcome, String> {
    let email = my_email()?;
    spam_io::scan(&app, &spam, &state, &email, 5, false).await
}

#[tauri::command]
pub async fn spam_train(
    app: AppHandle,
    spam: State<'_, SpamState>,
    items: Vec<spam_io::TrainItem>,
    is_spam: bool,
) -> Result<usize, String> {
    Ok(spam_io::train_items(&app, &spam, &items, is_spam))
}

#[tauri::command]
pub async fn spam_bootstrap(app: AppHandle, state: State<'_, AppState>, spam: State<'_, SpamState>) -> Result<spam_io::BootstrapResult, String> {
    let email = my_email()?;
    spam_io::bootstrap(&app, &spam, &state, &email).await
}

#[tauri::command]
pub async fn spam_refresh_lists(app: AppHandle, state: State<'_, AppState>, spam: State<'_, SpamState>) -> Result<usize, String> {
    let cfg = spam_io::load_config(&app);
    let set = spam_io::ensure_list(&app, &spam, &state.http, &cfg.provider, true).await?;
    Ok(set.len())
}

#[tauri::command]
pub async fn spam_restore(app: AppHandle, state: State<'_, AppState>, spam: State<'_, SpamState>, item: spam_io::TrainItem) -> Result<(), String> {
    spam_io::restore(&app, &spam, &state, &item).await
}

#[tauri::command]
pub async fn spam_move(app: AppHandle, state: State<'_, AppState>, spam: State<'_, SpamState>, item: spam_io::TrainItem) -> Result<(), String> {
    spam_io::move_to_spam(&app, &spam, &state, &item).await
}

#[derive(serde::Serialize)]
pub struct ContactOut {
    pub email: String,
    pub name: String,
}

fn contact_out(v: Vec<(String, String)>) -> Vec<ContactOut> {
    v.into_iter().map(|(email, name)| ContactOut { email, name }).collect()
}

/// Podpowiedzi adresów z książki adresów dla wpisywanego tekstu.
#[tauri::command]
pub fn contacts_suggest(store: State<'_, Store>, query: String) -> Result<Vec<ContactOut>, String> {
    Ok(contact_out(store.contacts_suggest(&query, 8)?))
}

/// Zapamiętuje adresy z pola (np. „Jan <jan@x.pl>, ala@y.pl”). Niepoprawne wpisy są pomijane.
#[tauri::command]
pub fn contacts_remember(store: State<'_, Store>, list: String) -> Result<usize, String> {
    let addrs = crate::contacts::parse_list(&list);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    store.contacts_remember(&addrs, now)?;
    Ok(addrs.len())
}

#[tauri::command]
pub fn contacts_list(store: State<'_, Store>) -> Result<Vec<ContactOut>, String> {
    Ok(contact_out(store.contacts_list()?))
}

#[tauri::command]
pub fn contacts_delete(store: State<'_, Store>, email: String) -> Result<(), String> {
    store.contacts_delete(&email)
}

#[tauri::command]
pub fn contacts_clear(store: State<'_, Store>) -> Result<(), String> {
    store.contacts_clear()
}

/// Uzupełnia książkę adresów z ostatnich wiadomości skrzynki odbiorczej i wysłanych (Gmail i konta IMAP).
/// Adresy z wysłanych liczą się trzykrotnie (pisałeś do tej osoby), z odebranych jednokrotnie. Nic nie zmienia na serwerach.
#[tauri::command]
pub async fn contacts_harvest(state: State<'_, AppState>, store: State<'_, Store>) -> Result<usize, String> {
    use std::collections::HashMap;
    let mut mine: Vec<String> = imap_io::imap_accounts()?.into_iter().map(|a| a.email.to_lowercase()).collect();
    let gmail_email = secrets::get(auth::KEY_EMAIL)?;
    if let Some(e) = &gmail_email {
        mine.push(e.to_lowercase());
    }
    // (adres → (nazwa, liczba użyć))
    let mut found: HashMap<String, (String, i64)> = HashMap::new();
    let mut add = |sent: bool, h: &(String, String, String)| {
        let w = if sent { 3 } else { 1 };
        // w wysłanych liczą się odbiorcy (Do, DW); w odebranych nadawca, a odbiorcy wiadomości do kilku osób tylko raz
        let fields: [&str; 3] = if sent { [&h.1, &h.2, ""] } else { [&h.0, &h.1, &h.2] };
        for f in fields {
            for a in contacts::parse_list(f) {
                if mine.contains(&a.email) || contacts::is_automatic(&a.email) {
                    continue;
                }
                let e = found.entry(a.email).or_insert((String::new(), 0));
                if e.0.is_empty() {
                    e.0 = a.name;
                }
                e.1 += w;
            }
        }
    };
    if gmail_email.is_some() {
        for (label, sent) in [("INBOX", false), ("SENT", true)] {
            if let Ok(v) = gmail::harvest_headers(&state, label, 150).await {
                for h in &v {
                    add(sent, h);
                }
            }
        }
    }
    for acc in imap_io::imap_accounts()? {
        if let Ok(v) = imap_io::harvest_account(&acc.email, 200).await {
            for (role, h) in &v {
                add(role == "sent", h);
            }
        }
    }
    let items: Vec<(contacts::Addr, i64)> = found.into_iter().map(|(email, (name, n))| (contacts::Addr { email, name }, n)).collect();
    store.contacts_learn(&items)
}

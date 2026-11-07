//! Komendy Tauri udostępniane frontendowi. Każda jest jawnie zarejestrowana w lib.rs;
//! frontend nie ma dostępu do powłoki ani systemu plików.

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::State;

use crate::auth::{self, AppState, ClientCreds};
use crate::{gmail, secrets};

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
    pub email: Option<String>,
}

#[tauri::command]
pub fn auth_status() -> Result<AuthStatus, String> {
    let configured = auth::load_creds()?.is_some();
    let email = if secrets::get(auth::KEY_REFRESH)?.is_some() {
        secrets::get(auth::KEY_EMAIL)?
    } else {
        None
    };
    Ok(AuthStatus { configured, email })
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
    *state.token.lock().await = None;
    Ok(())
}

#[tauri::command]
pub async fn list_threads(
    state: State<'_, AppState>,
    folder: String,
    page_token: Option<String>,
) -> Result<gmail::ThreadPage, String> {
    let email = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    gmail::list_threads(&state, &folder, page_token, &email).await
}

#[tauri::command]
pub async fn get_thread(state: State<'_, AppState>, id: String) -> Result<gmail::ThreadDetail, String> {
    let email = secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?;
    gmail::get_thread(&state, &id, &email).await
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

    #[test]
    fn rejects_malformed_client_id_before_touching_keyring() {
        assert!(save_client("zly-id".to_string(), "sekret".to_string()).is_err());
    }
}

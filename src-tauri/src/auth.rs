//! OAuth 2.0 dla aplikacji desktopowych: przepływ kodu autoryzacji z PKCE
//! i przekierowaniem na lokalny adres 127.0.0.1. Hasło Google nigdy nie jest widoczne dla aplikacji.

use std::time::{Duration, Instant};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::secrets;

/// Etap 2 jest tylko do odczytu, więc prosimy o najmniejszy możliwy zakres.
pub const SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";

pub const KEY_CLIENT: &str = "google_client";
pub const KEY_REFRESH: &str = "refresh_token";
pub const KEY_EMAIL: &str = "account_email";

/// Wspólny stan aplikacji: klient HTTP i token dostępowy trzymany tylko w pamięci.
pub struct AppState {
    pub http: reqwest::Client,
    pub token: tokio::sync::Mutex<Option<(String, Instant)>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .user_agent("RustMail")
                .timeout(Duration::from_secs(60))
                .build()
                .expect("nie udało się utworzyć klienta HTTP"),
            token: tokio::sync::Mutex::new(None),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ClientCreds {
    pub client_id: String,
    pub client_secret: String,
}

fn creds_from(id: Option<&str>, secret: Option<&str>) -> Option<ClientCreds> {
    match (id, secret) {
        (Some(i), Some(s)) if !i.trim().is_empty() && !s.trim().is_empty() => Some(ClientCreds {
            client_id: i.trim().to_string(),
            client_secret: s.trim().to_string(),
        }),
        _ => None,
    }
}

/// Dane klienta Google wbudowane w aplikację podczas budowania
/// (zmienne RUSTMAIL_GOOGLE_CLIENT_ID i RUSTMAIL_GOOGLE_CLIENT_SECRET). Dzięki nim logowanie
/// działa jak w Thunderbirdzie: jedno kliknięcie, bez ręcznego wklejania kodów.
pub fn builtin_creds() -> Option<ClientCreds> {
    creds_from(
        option_env!("RUSTMAIL_GOOGLE_CLIENT_ID"),
        option_env!("RUSTMAIL_GOOGLE_CLIENT_SECRET"),
    )
}

pub fn load_creds() -> Result<Option<ClientCreds>, String> {
    if let Some(c) = builtin_creds() {
        return Ok(Some(c));
    }
    match secrets::get(KEY_CLIENT)? {
        Some(s) => serde_json::from_str(&s).map(Some).map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

pub fn save_creds(c: &ClientCreds) -> Result<(), String> {
    let json = serde_json::to_string(c).map_err(|e| e.to_string())?;
    secrets::set(KEY_CLIENT, &json)
}

fn default_expires() -> u64 {
    3600
}

#[derive(Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default = "default_expires")]
    pub expires_in: u64,
    pub refresh_token: Option<String>,
}

pub fn random_urlsafe(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Wyciąga `code` i `state` z pierwszej linii żądania HTTP przekierowanego przez przeglądarkę.
pub fn parse_callback(request_line: &str) -> Result<(String, String), String> {
    let target = request_line
        .split_whitespace()
        .nth(1)
        .ok_or("Niepoprawne żądanie")?;
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).map_err(|e| e.to_string())?;
    let (mut code, mut state, mut error) = (None, None, None);
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            _ => {}
        }
    }
    if let Some(e) = error {
        return Err(format!("Logowanie przerwane: {e}"));
    }
    match (code, state) {
        (Some(c), Some(s)) => Ok((c, s)),
        _ => Err("W odpowiedzi brakuje kodu autoryzacji".to_string()),
    }
}

fn http_response(status: u16, msg: &str) -> String {
    let reason = if status == 200 { "OK" } else { "Not Found" };
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>RustMail</title>\
         <body style=\"font-family:sans-serif;margin:3rem\"><h2>RustMail</h2><p>{msg}</p></body>"
    );
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn open_browser(url: &str) -> Result<(), String> {
    if !url.starts_with("https://accounts.google.com/") {
        return Err("Niedozwolony adres logowania".to_string());
    }
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(url);
    // Zmienne środowiska AppImage mogłyby zepsuć uruchamianie przeglądarki systemowej.
    for k in ["LD_LIBRARY_PATH", "LD_PRELOAD", "APPDIR", "GDK_PIXBUF_MODULE_FILE"] {
        cmd.env_remove(k);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Nie można otworzyć przeglądarki (xdg-open): {e}"))
}

async fn token_request(http: &reqwest::Client, form: &[(&str, &str)]) -> Result<TokenResponse, String> {
    let resp = http
        .post(TOKEN_URL)
        .form(form)
        .send()
        .await
        .map_err(|e| format!("Brak połączenia z Google: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        if text.contains("invalid_grant") {
            return Err("Sesja Google wygasła lub została cofnięta. Zaloguj się ponownie.".to_string());
        }
        let short: String = text.chars().take(200).collect();
        return Err(format!("Google odrzucił żądanie tokenu ({status}): {short}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("Nieoczekiwana odpowiedź Google: {e}"))
}

/// Pełny przepływ logowania. Czeka maksymalnie 5 minut na zgodę w przeglądarce.
pub async fn sign_in_flow(http: &reqwest::Client, creds: &ClientCreds) -> Result<TokenResponse, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Nie można uruchomić lokalnego odbiornika logowania: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}");
    let verifier = random_urlsafe(64);
    let state = random_urlsafe(24);
    let challenge = pkce_challenge(&verifier);

    let url = reqwest::Url::parse_with_params(
        AUTH_URL,
        &[
            ("client_id", creds.client_id.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", state.as_str()),
            ("access_type", "offline"),
            ("prompt", "consent"),
        ],
    )
    .map_err(|e| e.to_string())?;
    open_browser(url.as_str())?;

    let (code, got_state) = tokio::time::timeout(Duration::from_secs(300), async {
        loop {
            let (mut stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let line = req.lines().next().unwrap_or("").to_string();
            if !line.contains("code=") && !line.contains("error=") {
                let _ = stream.write_all(http_response(404, "Nie znaleziono").as_bytes()).await;
                continue;
            }
            let parsed = parse_callback(&line);
            let msg = if parsed.is_ok() {
                "Zalogowano. Możesz zamknąć tę kartę i wrócić do RustMail."
            } else {
                "Logowanie nie powiodło się. Wróć do RustMail."
            };
            let _ = stream.write_all(http_response(200, msg).as_bytes()).await;
            return parsed;
        }
    })
    .await
    .map_err(|_| "Upłynął czas oczekiwania na logowanie (5 minut).".to_string())??;

    if got_state != state {
        return Err("Niezgodny parametr state – logowanie przerwano ze względów bezpieczeństwa.".to_string());
    }

    let form = [
        ("code", code.as_str()),
        ("client_id", creds.client_id.as_str()),
        ("client_secret", creds.client_secret.as_str()),
        ("code_verifier", verifier.as_str()),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect.as_str()),
    ];
    token_request(http, &form).await
}

async fn refresh_access(http: &reqwest::Client, creds: &ClientCreds, refresh: &str) -> Result<TokenResponse, String> {
    let form = [
        ("client_id", creds.client_id.as_str()),
        ("client_secret", creds.client_secret.as_str()),
        ("refresh_token", refresh),
        ("grant_type", "refresh_token"),
    ];
    token_request(http, &form).await
}

/// Zwraca ważny token dostępowy; odnawia go automatycznie na podstawie tokenu z magazynu haseł.
pub async fn access_token(state: &AppState) -> Result<String, String> {
    let mut guard = state.token.lock().await;
    if let Some((tok, exp)) = guard.as_ref() {
        if *exp > Instant::now() + Duration::from_secs(60) {
            return Ok(tok.clone());
        }
    }
    let creds = load_creds()?.ok_or("Brak danych klienta Google.")?;
    let refresh = secrets::get(KEY_REFRESH)?.ok_or("Nie jesteś zalogowany.")?;
    let tr = refresh_access(&state.http, &creds, &refresh).await?;
    *guard = Some((tr.access_token.clone(), Instant::now() + Duration::from_secs(tr.expires_in)));
    Ok(tr.access_token)
}

pub async fn revoke(http: &reqwest::Client, token: &str) {
    let _ = http.post(REVOKE_URL).form(&[("token", token)]).send().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_creds_require_both_values() {
        assert!(creds_from(None, None).is_none());
        assert!(creds_from(Some("id"), None).is_none());
        assert!(creds_from(Some("  "), Some("sekret")).is_none());
        let c = creds_from(Some(" id.apps.googleusercontent.com "), Some("sekret")).unwrap();
        assert_eq!(c.client_id, "id.apps.googleusercontent.com");
    }

    #[test]
    fn pkce_matches_rfc7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn random_values_are_unique_and_urlsafe() {
        let (a, b) = (random_urlsafe(32), random_urlsafe(32));
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn parses_successful_callback() {
        let r = parse_callback("GET /?code=abc%2F1&state=xyz HTTP/1.1").unwrap();
        assert_eq!(r, ("abc/1".to_string(), "xyz".to_string()));
    }

    #[test]
    fn callback_with_error_is_rejected() {
        let e = parse_callback("GET /?error=access_denied&state=x HTTP/1.1").unwrap_err();
        assert!(e.contains("access_denied"));
    }

    #[test]
    fn callback_without_code_is_rejected() {
        assert!(parse_callback("GET /?state=x HTTP/1.1").is_err());
    }

    #[test]
    fn browser_only_opens_google_urls() {
        assert!(open_browser("https://evil.example.com/").is_err());
        assert!(open_browser("file:///etc/passwd").is_err());
    }
}

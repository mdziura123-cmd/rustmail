//! Operacje „na zewnątrz” inicjowane wyłącznie kliknięciem użytkownika:
//! pobranie obrazu z wiadomości, zapis załącznika na dysku i otwarcie linku w przeglądarce.
//! Wszystkie reguły bezpieczeństwa (które adresy, jakie nazwy plików) są w `safety.rs`.

use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::safety;

const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;

/// Pobiera obraz i zwraca go jako adres `data:`. Dozwolone wyłącznie https, wyłącznie publiczne
/// adresy IP (sprawdzane przy każdym przekierowaniu, a połączenie przypinane do sprawdzonego IP),
/// tylko typy PNG/JPEG/GIF/WebP, maksymalnie 5 MB. Nie wysyłamy ciasteczek ani nagłówka Referer.
pub async fn fetch_image(url: &str) -> Result<String, String> {
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        safety::check_remote_url(&current)?;
        let parsed = reqwest::Url::parse(&current).map_err(|_| "Niepoprawny adres obrazu.".to_string())?;
        let host = parsed.host_str().ok_or("Niepoprawny adres obrazu.")?.trim_matches(['[', ']']).to_string();
        let port = parsed.port_or_known_default().unwrap_or(443);

        let addrs: Vec<SocketAddr> = match host.parse::<IpAddr>() {
            Ok(ip) => vec![SocketAddr::new(ip, port)],
            Err(_) => tokio::net::lookup_host((host.as_str(), port))
                .await
                .map_err(|_| "Nie można znaleźć serwera obrazu.".to_string())?
                .collect(),
        };
        if addrs.is_empty() || addrs.iter().any(|a| !safety::ip_is_public(&a.ip())) {
            return Err("Adres obrazu prowadzi do sieci lokalnej lub jest niedostępny – pominięto.".to_string());
        }

        let mut builder = reqwest::Client::builder()
            .user_agent("Mozilla/5.0")
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none());
        if host.parse::<IpAddr>().is_err() {
            builder = builder.resolve(&host, addrs[0]);
        }
        let client = builder.build().map_err(|e| e.to_string())?;
        let mut resp = client
            .get(parsed.clone())
            .header("Accept", "image/png,image/jpeg,image/gif,image/webp")
            .send()
            .await
            .map_err(|_| "Nie udało się pobrać obrazu.".to_string())?;

        if resp.status().is_redirection() {
            let loc = resp
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or("Przekierowanie bez adresu docelowego.")?;
            current = parsed.join(loc).map_err(|_| "Niepoprawne przekierowanie.".to_string())?.to_string();
            continue;
        }
        if !resp.status().is_success() {
            return Err(format!("Serwer obrazu odpowiedział błędem {}.", resp.status()));
        }
        let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let mime = safety::valid_image_mime(&ct).ok_or("To nie jest obsługiwany obraz (PNG, JPEG, GIF, WebP).")?;
        if resp.content_length().map_or(false, |l| l as usize > MAX_IMAGE_BYTES) {
            return Err("Obraz jest zbyt duży (limit 5 MB).".to_string());
        }
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|_| "Przerwano pobieranie obrazu.".to_string())? {
            if buf.len() + chunk.len() > MAX_IMAGE_BYTES {
                return Err("Obraz jest zbyt duży (limit 5 MB).".to_string());
            }
            buf.extend_from_slice(&chunk);
        }
        return Ok(format!("data:{mime};base64,{}", STANDARD.encode(&buf)));
    }
    Err("Zbyt wiele przekierowań.".to_string())
}

fn downloads_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("Nie znaleziono katalogu domowego.")?;
    Ok(PathBuf::from(home).join("Downloads"))
}

/// Zapisuje załącznik w ~/Downloads pod bezpieczną, unikalną nazwą, bez uprawnień do uruchamiania.
/// Istniejące pliki nigdy nie są nadpisywane.
pub fn save_download(name: &str, data: &[u8]) -> Result<PathBuf, String> {
    let dir = downloads_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Nie można utworzyć katalogu Pobrane: {e}"))?;
    let safe = safety::safe_filename(name);
    for _ in 0..20 {
        let path = safety::unique_path(&dir, &safe, |p| p.exists());
        match std::fs::OpenOptions::new().write(true).create_new(true).mode(0o644).open(&path) {
            Ok(mut f) => {
                f.write_all(data).map_err(|e| format!("Nie można zapisać pliku: {e}"))?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Nie można zapisać pliku: {e}")),
        }
    }
    Err("Nie udało się wybrać wolnej nazwy pliku.".to_string())
}

/// Otwiera adres w przeglądarce systemowej (tylko http, https, mailto).
pub fn open_external(url: &str) -> Result<(), String> {
    safety::check_open_url(url)?;
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(url.trim());
    // Zmienne środowiska AppImage mogłyby zepsuć uruchamianie przeglądarki systemowej.
    for k in ["LD_LIBRARY_PATH", "LD_PRELOAD", "APPDIR", "GDK_PIXBUF_MODULE_FILE"] {
        cmd.env_remove(k);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Nie można otworzyć przeglądarki (xdg-open): {e}"))
}

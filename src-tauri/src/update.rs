//! Samoaktualizacja z wydań GitHub. Aplikacja sprawdza najnowsze wydanie, a pobiera i podmienia
//! plik AppImage dopiero po wyraźnym potwierdzeniu użytkownika. Każdy pobrany plik jest weryfikowany
//! sumą SHA-256 z wydania oraz podpisem Ed25519 (klucz publiczny jest wkompilowany w aplikację);
//! poprzednia wersja zostaje zachowana do przywrócenia.

use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const REPO: &str = "mdziura123-cmd/rustmail";
const ASSET: &str = "RustMail-x86_64.AppImage";
const MAX_SIZE: u64 = 600 * 1024 * 1024;

/// Klucz publiczny Ed25519 wydawcy. Odpowiadający mu klucz prywatny istnieje wyłącznie
/// w sekrecie `UPDATE_SIGNING_KEY` repozytorium i służy do podpisywania wydań w CI.
const UPDATE_PUBKEY: [u8; 32] = [
    0xf7, 0x7f, 0x42, 0xf6, 0x18, 0x0a, 0xf5, 0xd1, 0xa7, 0xb6, 0xe3, 0xf9, 0xf5, 0x7c, 0xe1, 0x8b,
    0xfd, 0x32, 0x22, 0xa5, 0xd5, 0xe7, 0xc7, 0x77, 0x0c, 0xa6, 0x73, 0x3e, 0xef, 0x9f, 0xaa, 0x47,
];

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Serialize)]
pub struct UpdateInfo {
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub notes: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    pub appimage: bool,
    pub has_previous: bool,
}

pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.trim().trim_start_matches('v').split(|c| c == '-' || c == '+').next()?;
    let mut it = core.split('.');
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Plik `.sha256` ma format `suma  nazwa` (jak z `sha256sum`).
pub fn parse_sha(text: &str) -> Option<String> {
    let h = text.split_whitespace().next()?.to_ascii_lowercase();
    if h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(h)
    } else {
        None
    }
}

/// Treść, którą podpisuje CI: prefiks domenowy + SHA-256 pliku AppImage (małe litery hex).
pub fn signed_message(sha_hex: &str) -> Vec<u8> {
    format!("rustmail-appimage-sha256:{sha_hex}").into_bytes()
}

/// Sprawdza podpis Ed25519 (surowe 64 bajty) pod wiadomością podanym kluczem publicznym.
pub fn verify_signature(pubkey: &[u8; 32], msg: &[u8], sig: &[u8]) -> bool {
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, pubkey)
        .verify(msg, sig)
        .is_ok()
}

pub fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn appimage_path() -> Result<PathBuf, String> {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .ok_or_else(|| "Samoaktualizacja działa tylko po uruchomieniu z pliku AppImage.".to_string())
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("RustMail-updater")
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())
}

async fn fetch_release(c: &reqwest::Client) -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = c
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Brak połączenia z GitHub: {e}"))?;
    match resp.status().as_u16() {
        404 => Err("Nie znaleziono opublikowanego wydania. Repozytorium musi być publiczne i mieć co najmniej jedno wydanie.".to_string()),
        403 | 429 => Err("GitHub chwilowo odrzucił zapytanie (limit). Spróbuj później.".to_string()),
        s if !(200..300).contains(&s) => Err(format!("GitHub zwrócił błąd {s}")),
        _ => resp.json::<Release>().await.map_err(|e| format!("Nieoczekiwana odpowiedź GitHub: {e}")),
    }
}

#[tauri::command]
pub async fn check_update() -> Result<UpdateInfo, String> {
    let c = client()?;
    let r = fetch_release(&c).await?;
    let current = env!("CARGO_PKG_VERSION").to_string();
    let has_asset = r.assets.iter().any(|a| a.name == ASSET);
    Ok(UpdateInfo {
        available: !r.draft && !r.prerelease && has_asset && is_newer(&r.tag_name, &current),
        latest: r.tag_name.trim_start_matches('v').to_string(),
        current,
        notes: r.body.unwrap_or_default().chars().take(2000).collect(),
    })
}

/// Pobiera, weryfikuje i podmienia plik AppImage. Wywoływane wyłącznie po kliknięciu użytkownika.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Emitter;
    let appimage = appimage_path()?;
    let c = client()?;
    let r = fetch_release(&c).await?;
    if r.draft || r.prerelease || !is_newer(&r.tag_name, env!("CARGO_PKG_VERSION")) {
        return Err("Masz już najnowszą wersję.".to_string());
    }
    let base = format!("https://github.com/{REPO}/releases/download/");
    let sha_name = format!("{ASSET}.sha256");
    let sig_name = format!("{ASSET}.sig");
    let find = |name: &str| r.assets.iter().find(|a| a.name == name && a.browser_download_url.starts_with(&base));
    let bin = find(ASSET).ok_or("Wydanie nie zawiera pliku AppImage.")?;
    let sha = find(&sha_name).ok_or("Wydanie nie zawiera pliku z sumą SHA-256 – aktualizacja przerwana.")?;

    let sha_text = c
        .get(&sha.browser_download_url)
        .send()
        .await
        .and_then(|x| x.error_for_status())
        .map_err(|e| format!("Nie udało się pobrać sumy kontrolnej: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let expected = parse_sha(&sha_text).ok_or("Plik sumy kontrolnej ma niepoprawny format.")?;

    // Podpis jest wymagany: bez poprawnego podpisu wydawcy aktualizacja nie zostanie zainstalowana.
    let sig_asset = find(&sig_name).ok_or("Wydanie nie jest podpisane (brak pliku .sig) – aktualizacja przerwana.")?;
    let sig_bytes = c
        .get(&sig_asset.browser_download_url)
        .send()
        .await
        .and_then(|x| x.error_for_status())
        .map_err(|e| format!("Nie udało się pobrać podpisu: {e}"))?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    if sig_bytes.len() != 64 {
        return Err("Plik podpisu ma niepoprawny rozmiar – aktualizacja przerwana.".to_string());
    }
    if !verify_signature(&UPDATE_PUBKEY, &signed_message(&expected), &sig_bytes) {
        return Err("Podpis wydania jest niepoprawny – aktualizacja przerwana.".to_string());
    }

    let tmp = with_suffix(&appimage, ".new");
    let cleanup = || {
        let _ = fs::remove_file(&tmp);
    };
    let mut resp = c
        .get(&bin.browser_download_url)
        .send()
        .await
        .and_then(|x| x.error_for_status())
        .map_err(|e| format!("Nie udało się pobrać aktualizacji: {e}"))?;
    let mut file = fs::File::create(&tmp)
        .map_err(|e| format!("Nie można zapisać pliku obok AppImage (brak uprawnień do katalogu?): {e}"))?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let expected_len = resp.content_length().unwrap_or(0);
    let mut last_pct = u64::MAX;
    let _ = app.emit("update-progress", serde_json::json!({ "done": 0u64, "total": expected_len }));
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                total += chunk.len() as u64;
                if total > MAX_SIZE {
                    cleanup();
                    return Err("Plik aktualizacji jest zbyt duży.".to_string());
                }
                hasher.update(&chunk);
                if expected_len > 0 {
                    let pct = total * 100 / expected_len;
                    if pct != last_pct {
                        last_pct = pct;
                        let _ = app.emit("update-progress", serde_json::json!({ "done": total, "total": expected_len }));
                    }
                }
                if let Err(e) = file.write_all(&chunk) {
                    cleanup();
                    return Err(format!("Błąd zapisu aktualizacji: {e}"));
                }
            }
            Ok(None) => break,
            Err(e) => {
                cleanup();
                return Err(format!("Przerwano pobieranie: {e}"));
            }
        }
    }
    if let Err(e) = file.sync_all() {
        cleanup();
        return Err(format!("Błąd zapisu aktualizacji: {e}"));
    }
    drop(file);

    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        cleanup();
        return Err("Suma SHA-256 pobranego pliku nie zgadza się z sumą z wydania. Aktualizacja przerwana.".to_string());
    }

    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    let prev = with_suffix(&appimage, ".previous");
    fs::copy(&appimage, &prev).map_err(|e| format!("Nie udało się zachować poprzedniej wersji: {e}"))?;
    fs::rename(&tmp, &appimage).map_err(|e| format!("Nie udało się podmienić pliku aplikacji: {e}"))?;
    Ok(r.tag_name.trim_start_matches('v').to_string())
}

#[tauri::command]
pub fn rollback_update() -> Result<(), String> {
    let a = appimage_path()?;
    let prev = with_suffix(&a, ".previous");
    if !prev.exists() {
        return Err("Brak zapisanej poprzedniej wersji.".to_string());
    }
    fs::rename(&prev, &a).map_err(|e| format!("Nie udało się przywrócić poprzedniej wersji: {e}"))
}

#[tauri::command]
pub fn update_state() -> UpdateState {
    match appimage_path() {
        Ok(a) => UpdateState { appimage: true, has_previous: with_suffix(&a, ".previous").exists() },
        Err(_) => UpdateState { appimage: false, has_previous: false },
    }
}

/// Uruchamia ponownie aplikację z (już podmienionego) pliku AppImage.
#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) -> Result<(), String> {
    let a = appimage_path()?;
    let mut cmd = std::process::Command::new(&a);
    for k in [
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "APPDIR",
        "APPIMAGE",
        "ARGV0",
        "OWD",
        "GDK_PIXBUF_MODULE_FILE",
        "GSETTINGS_SCHEMA_DIR",
        "GIO_EXTRA_MODULES",
    ] {
        cmd.env_remove(k);
    }
    cmd.spawn().map_err(|e| format!("Nie udało się uruchomić nowej wersji: {e}"))?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_compares_versions() {
        assert_eq!(parse_version("v0.2.10"), Some((0, 2, 10)));
        assert_eq!(parse_version("1.0.0-beta.1"), Some((1, 0, 0)));
        assert_eq!(parse_version("abc"), None);
        assert!(is_newer("v0.2.10", "0.2.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("nonsens", "0.2.0"));
    }

    #[test]
    fn parses_sha256_file() {
        let h = "a".repeat(64);
        assert_eq!(parse_sha(&format!("{h}  RustMail-x86_64.AppImage\n")), Some(h.clone()));
        assert_eq!(parse_sha(&h.to_uppercase()), Some(h));
        assert_eq!(parse_sha("za-krotka"), None);
        assert_eq!(parse_sha(&"g".repeat(64)), None);
    }

    const TEST_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TEST_SIG: &str = "9d534ebc6e59944910c431f0cc501402eb65f1c1979a9da6c1ebc0f024cd85f86cfa8e3cc3bf66784626ad87f0c723d3fd21510e00b0f1af5e55a6a818675a02";

    fn unhex(h: &str) -> Vec<u8> {
        (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn accepts_valid_signature_and_rejects_tampering() {
        let sig = unhex(TEST_SIG);
        assert!(verify_signature(&UPDATE_PUBKEY, &signed_message(TEST_SHA), &sig));
        let other = "b".repeat(64);
        assert!(!verify_signature(&UPDATE_PUBKEY, &signed_message(&other), &sig));
        let mut bad = sig.clone();
        bad[0] ^= 1;
        assert!(!verify_signature(&UPDATE_PUBKEY, &signed_message(TEST_SHA), &bad));
        assert!(!verify_signature(&UPDATE_PUBKEY, &signed_message(TEST_SHA), &sig[..63]));
        let mut key = UPDATE_PUBKEY;
        key[0] ^= 1;
        assert!(!verify_signature(&key, &signed_message(TEST_SHA), &sig));
    }

    #[test]
    fn appends_suffix_to_path() {
        let p = Path::new("/home/u/RustMail-x86_64.AppImage");
        assert_eq!(with_suffix(p, ".previous"), PathBuf::from("/home/u/RustMail-x86_64.AppImage.previous"));
    }
}

//! Proste tłumaczenie angielski → polski. Tekst jest wysyłany do darmowej usługi MyMemory (api.mymemory.translated.net)
//! wyłącznie po kliknięciu użytkownika; nic nie jest tłumaczone automatycznie.

// ---- część czysta (testowana osobno) ----
const MAX_CHUNK: usize = 450;
pub const MAX_CHARS: usize = 5000;

/// Dzieli tekst na kawałki do ~450 znaków (limit usługi). Zwraca kawałki i separator, który wraca po każdym z nich.
pub fn chunk_text(text: &str, max: usize) -> Vec<(String, &'static str)> {
    let mut out: Vec<(String, &'static str)> = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<(String, &'static str)>, sep: &'static str| {
        if !cur.trim().is_empty() {
            out.push((std::mem::take(cur), sep));
        } else {
            cur.clear();
            if let Some(last) = out.last_mut() {
                last.1 = "\n";
            }
        }
    };
    for line in text.lines() {
        let line = line.trim_end();
        if line.chars().count() > max {
            flush(&mut cur, &mut out, "\n");
            let mut piece = String::new();
            for word in line.split_inclusive(' ') {
                if piece.chars().count() + word.chars().count() > max && !piece.is_empty() {
                    out.push((piece.trim_end().to_string(), " "));
                    piece = String::new();
                }
                if word.chars().count() > max {
                    // pojedyncze „słowo” dłuższe niż limit – tniemy twardo
                    let chars: Vec<char> = word.chars().collect();
                    for part in chars.chunks(max) {
                        out.push((part.iter().collect::<String>(), ""));
                    }
                } else {
                    piece.push_str(word);
                }
            }
            if !piece.trim().is_empty() {
                out.push((piece.trim_end().to_string(), "\n"));
            }
            continue;
        }
        let extra = if cur.is_empty() { 0 } else { 1 };
        if !cur.is_empty() && cur.chars().count() + extra + line.chars().count() > max {
            flush(&mut cur, &mut out, "\n");
        }
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(line);
    }
    flush(&mut cur, &mut out, "\n");
    out
}

/// Zamienia podstawowe encje HTML (&amp; &#39; &#x27; …) na znaki.
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(j) = tail.find(';').filter(|j| *j <= 10) {
            let ent = &tail[1..j];
            let ch = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ if ent.starts_with("#x") || ent.starts_with("#X") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
                _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
                _ => None,
            };
            if let Some(c) = ch {
                out.push(c);
                rest = &tail[j + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

/// Dostawcy tłumaczenia.
pub const PROVIDERS: [&str; 4] = ["mymemory", "deepl", "google", "microsoft"];

/// Klucz API: 20–100 znaków (litery, cyfry, myślnik, podkreślenie), DeepL opcjonalnie z końcówką „:fx” (plan darmowy).
pub fn valid_key(provider: &str, k: &str) -> bool {
    let body = if provider == "deepl" { k.strip_suffix(":fx").unwrap_or(k) } else { k };
    (20..=100).contains(&body.len()) && body.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Region zasobu Microsoft (np. „westeurope”, „global”).
pub fn valid_region(r: &str) -> bool {
    (2..=30).contains(&r.len()) && r.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// Adres DeepL zależny od planu (klucze darmowe kończą się na „:fx”). Adresy są stałe – klucz nigdy nie trafia gdzie indziej.
pub fn deepl_url(k: &str) -> &'static str {
    if k.ends_with(":fx") {
        "https://api-free.deepl.com/v2/translate"
    } else {
        "https://api.deepl.com/v2/translate"
    }
}

// ---- część sieciowa ----
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::consent::{self, Consent};
use std::time::Duration;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateOut {
    pub translated: String,
    /// Tekst był zbyt długi – przetłumaczono tylko początek.
    pub truncated: bool,
}

fn key_name(provider: &str) -> String {
    format!("translate_key_{provider}")
}

fn get_key(provider: &str) -> Option<String> {
    crate::secrets::get(&key_name(provider)).ok().flatten()
}

#[derive(Serialize)]
pub struct KeyStatus {
    pub deepl: bool,
    pub google: bool,
    pub microsoft: bool,
}

/// Które klucze są zapisane (samych kluczy nigdy nie zwracamy do interfejsu).
#[tauri::command]
pub fn translate_keys() -> KeyStatus {
    KeyStatus { deepl: get_key("deepl").is_some(), google: get_key("google").is_some(), microsoft: get_key("microsoft").is_some() }
}

#[tauri::command]
pub fn translate_set_key(provider: String, key: String, region: Option<String>) -> Result<(), String> {
    if !PROVIDERS.contains(&provider.as_str()) || provider == "mymemory" {
        return Err("Ten dostawca nie używa klucza.".to_string());
    }
    let k = key.trim();
    if !valid_key(&provider, k) {
        return Err("To nie wygląda na poprawny klucz API (sprawdź, czy skopiowano go w całości, bez spacji).".to_string());
    }
    if provider == "microsoft" {
        let r = region.unwrap_or_default().trim().to_ascii_lowercase();
        let r = if r.is_empty() { "global".to_string() } else { r };
        if !valid_region(&r) {
            return Err("Region wpisz małymi literami, np. westeurope lub global.".to_string());
        }
        crate::secrets::set(&key_name("microsoft_region"), &r)?;
    }
    crate::secrets::set(&key_name(&provider), k)
}

#[tauri::command]
pub fn translate_clear_key(provider: String) -> Result<(), String> {
    if !PROVIDERS.contains(&provider.as_str()) || provider == "mymemory" {
        return Ok(());
    }
    if provider == "microsoft" {
        let _ = crate::secrets::delete(&key_name("microsoft_region"));
    }
    crate::secrets::delete(&key_name(&provider))
}

fn missing_key(name: &str) -> String {
    format!("Wybrano {name}, ale nie wpisano klucza API. Wpisz go w Ustawieniach → Ogólne → Tłumacz albo wybierz inną usługę.")
}

async fn status_error(resp: reqwest::Response, name: &str) -> String {
    match resp.status().as_u16() {
        401 | 403 => format!("{name} odrzucił klucz (lub usługa nie jest włączona dla tego klucza). Sprawdź go w Ustawieniach."),
        400 => format!("{name} odrzucił zapytanie (sprawdź klucz API)."),
        429 => format!("{name}: zbyt wiele zapytań. Spróbuj za chwilę."),
        456 => format!("{name}: wyczerpano limit znaków."),
        s => format!("{name} zwrócił błąd {s}."),
    }
}

async fn deepl_translate(client: &reqwest::Client, body: &str) -> Result<String, String> {
    let key = get_key("deepl").ok_or_else(|| missing_key("DeepL"))?;
    let resp = client
        .post(deepl_url(&key))
        .header("Authorization", format!("DeepL-Auth-Key {key}"))
        .json(&serde_json::json!({ "text": [body], "source_lang": "EN", "target_lang": "PL" }))
        .send()
        .await
        .map_err(|e| format!("Brak połączenia z DeepL: {e}"))?;
    if !resp.status().is_success() {
        return Err(status_error(resp, "DeepL").await);
    }
    let v: serde_json::Value = resp.json().await.map_err(|_| "Nieoczekiwana odpowiedź DeepL.".to_string())?;
    non_empty(v["translations"][0]["text"].as_str(), "DeepL")
}

async fn google_translate(client: &reqwest::Client, body: &str) -> Result<String, String> {
    let key = get_key("google").ok_or_else(|| missing_key("Google"))?;
    let resp = client
        .post("https://translation.googleapis.com/language/translate/v2")
        .header("X-goog-api-key", key)
        .json(&serde_json::json!({ "q": body, "source": "en", "target": "pl", "format": "text" }))
        .send()
        .await
        .map_err(|e| format!("Brak połączenia z Google: {e}"))?;
    if !resp.status().is_success() {
        return Err(status_error(resp, "Google").await);
    }
    let v: serde_json::Value = resp.json().await.map_err(|_| "Nieoczekiwana odpowiedź Google.".to_string())?;
    non_empty(v["data"]["translations"][0]["translatedText"].as_str().map(decode_entities).as_deref(), "Google")
}

async fn microsoft_translate(client: &reqwest::Client, body: &str) -> Result<String, String> {
    let key = get_key("microsoft").ok_or_else(|| missing_key("Microsoft"))?;
    let region = crate::secrets::get(&key_name("microsoft_region")).ok().flatten().unwrap_or_else(|| "global".to_string());
    let resp = client
        .post("https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&from=en&to=pl")
        .header("Ocp-Apim-Subscription-Key", key)
        .header("Ocp-Apim-Subscription-Region", region)
        .json(&serde_json::json!([{ "Text": body }]))
        .send()
        .await
        .map_err(|e| format!("Brak połączenia z Microsoft: {e}"))?;
    if !resp.status().is_success() {
        return Err(status_error(resp, "Microsoft").await);
    }
    let v: serde_json::Value = resp.json().await.map_err(|_| "Nieoczekiwana odpowiedź Microsoft.".to_string())?;
    non_empty(v[0]["translations"][0]["text"].as_str(), "Microsoft")
}

fn non_empty(t: Option<&str>, name: &str) -> Result<String, String> {
    match t {
        Some(t) if !t.is_empty() => Ok(t.to_string()),
        _ => Err(format!("{name} nie zwrócił tłumaczenia.")),
    }
}

async fn mymemory_translate(client: &reqwest::Client, body: &str) -> Result<String, String> {
    let mut result = String::new();
    for (chunk, sep) in chunk_text(body, MAX_CHUNK) {
        let resp = client
            .get("https://api.mymemory.translated.net/get")
            .query(&[("q", chunk.as_str()), ("langpair", "en|pl")])
            .send()
            .await
            .map_err(|e| format!("Brak połączenia z usługą tłumaczenia: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("Usługa tłumaczenia zwróciła błąd {}.", resp.status().as_u16()));
        }
        let raw = resp.text().await.map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&raw).map_err(|_| "Nieoczekiwana odpowiedź usługi tłumaczenia.".to_string())?;
        let code = v["responseStatus"].as_i64().or_else(|| v["responseStatus"].as_str().and_then(|s| s.parse().ok())).unwrap_or(200);
        let translated = v["responseData"]["translatedText"].as_str().unwrap_or("");
        if translated.to_ascii_uppercase().contains("MYMEMORY WARNING") || code == 429 {
            return Err("Dzienny limit darmowego tłumaczenia został wyczerpany. Spróbuj jutro albo wybierz inną usługę w Ustawieniach.".to_string());
        }
        if code != 200 || translated.is_empty() {
            let why = v["responseDetails"].as_str().unwrap_or("").chars().take(120).collect::<String>();
            return Err(format!("Usługa tłumaczenia odrzuciła zapytanie (kod {code}). {why}").trim().to_string());
        }
        result.push_str(&decode_entities(translated));
        result.push_str(sep);
    }
    Ok(result.trim_end().to_string())
}

/// Tłumaczy tekst wybraną usługą (`mymemory`, `deepl`, `google`, `microsoft`). Usługa jest wybierana w Ustawieniach.
#[tauri::command]
pub async fn translate_text(app: AppHandle, text: String, provider: String) -> Result<TranslateOut, String> {
    if !PROVIDERS.contains(&provider.as_str()) {
        return Err("Nieznana usługa tłumaczenia.".to_string());
    }
    // Backend sam pilnuje zgody: bez włączonej funkcji i zgody na tę usługę nic nie jest wysyłane.
    consent::check(&load_consent(&app), &provider)?;
    let cleaned: String = text.chars().map(|c| if c.is_control() && c != '\n' { ' ' } else { c }).collect();
    let truncated = cleaned.chars().count() > MAX_CHARS;
    let body: String = cleaned.chars().take(MAX_CHARS).collect();
    if body.trim().is_empty() {
        return Err("Brak tekstu do przetłumaczenia.".to_string());
    }
    let client = reqwest::Client::builder()
        .user_agent("RustMail-translate")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| e.to_string())?;
    let translated = match provider.as_str() {
        "deepl" => deepl_translate(&client, &body).await?,
        "google" => google_translate(&client, &body).await?,
        "microsoft" => microsoft_translate(&client, &body).await?,
        _ => mymemory_translate(&client, &body).await?,
    };
    Ok(TranslateOut { translated, truncated })
}

fn consent_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join("translate_consent.txt"))
}

fn load_consent(app: &AppHandle) -> Consent {
    consent_path(app)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| consent::parse(&t, &PROVIDERS))
        .unwrap_or_default()
}

fn save_consent(app: &AppHandle, c: &Consent) -> Result<(), String> {
    crate::privfs::write_private(&consent_path(app)?, consent::render(c).as_bytes())
}

#[derive(Serialize)]
pub struct ConsentOut {
    pub enabled: bool,
    pub granted: Vec<String>,
}

fn out(c: Consent) -> ConsentOut {
    ConsentOut { enabled: c.enabled, granted: c.granted }
}

#[tauri::command]
pub fn translate_consent(app: AppHandle) -> ConsentOut {
    out(load_consent(&app))
}

/// Całkowite włączenie/wyłączenie tłumaczenia online (po wyłączeniu przycisk znika, a backend odmawia).
#[tauri::command]
pub fn translate_set_enabled(app: AppHandle, enabled: bool) -> Result<ConsentOut, String> {
    let mut c = load_consent(&app);
    c.enabled = enabled;
    save_consent(&app, &c)?;
    Ok(out(c))
}

/// Zgoda na wysyłanie treści do wskazanej usługi (wywoływana po kliknięciu „Rozumiem” w interfejsie).
#[tauri::command]
pub fn translate_grant(app: AppHandle, provider: String) -> Result<ConsentOut, String> {
    let mut c = load_consent(&app);
    consent::grant(&mut c, &provider, &PROVIDERS)?;
    save_consent(&app, &c)?;
    Ok(out(c))
}

/// Cofnięcie wszystkich zgód (przy następnym tłumaczeniu aplikacja zapyta ponownie).
#[tauri::command]
pub fn translate_revoke(app: AppHandle) -> Result<ConsentOut, String> {
    let mut c = load_consent(&app);
    c.granted.clear();
    save_consent(&app, &c)?;
    Ok(out(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_respect_limit_and_keep_text() {
        let text = "Hello there.\nThis is a second line.\n\nNew paragraph here.";
        let c = chunk_text(text, 450);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].0, text);
        let long = "word ".repeat(300);
        let parts = chunk_text(&long, 450);
        assert!(parts.len() >= 4);
        assert!(parts.iter().all(|(t, _)| t.chars().count() <= 450));
        let giant = "x".repeat(1000);
        assert!(chunk_text(&giant, 450).iter().all(|(t, _)| t.chars().count() <= 450));
        assert!(chunk_text("   \n ", 450).is_empty());
    }

    #[test]
    fn validates_keys() {
        assert!(valid_key("deepl", "279a2e9d-83b3-c416-7e2d-f721593e42a0:fx"));
        assert!(valid_key("deepl", "279a2e9d-83b3-c416-7e2d-f721593e42a0"));
        assert!(valid_key("google", "AIzaSyA1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q"));
        assert!(!valid_key("google", "AIzaSyA1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q:fx"));
        assert!(!valid_key("deepl", "za-krotki"));
        assert!(!valid_key("deepl", "279a2e9d-83b3-c416-7e2d-f721593e42a0 \n"));
        assert!(!valid_key("microsoft", "279a2e9d83b3c4167e2df721593e42a0;rm -rf"));
        assert!(valid_region("westeurope") && valid_region("global"));
        assert!(!valid_region("West Europe") && !valid_region("x"));
        assert_eq!(deepl_url("abc:fx"), "https://api-free.deepl.com/v2/translate");
        assert_eq!(deepl_url("abc"), "https://api.deepl.com/v2/translate");
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(decode_entities("Tom &amp; Jerry &#39;x&#39; &quot;y&quot; &lt;b&gt;"), "Tom & Jerry 'x' \"y\" <b>");
        assert_eq!(decode_entities("&#x41;&#322; a & b &nieznane; &"), "Ał a & b &nieznane; &");
    }
}

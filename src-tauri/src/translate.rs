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

// ---- część sieciowa ----
use serde::Serialize;
use std::time::Duration;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateOut {
    pub translated: String,
    /// Tekst był zbyt długi – przetłumaczono tylko początek.
    pub truncated: bool,
}

#[tauri::command]
pub async fn translate_text(text: String) -> Result<TranslateOut, String> {
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
    let mut result = String::new();
    for (chunk, sep) in chunk_text(&body, MAX_CHUNK) {
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
            return Err("Dzienny limit darmowego tłumaczenia został wyczerpany. Spróbuj jutro.".to_string());
        }
        if code != 200 || translated.is_empty() {
            let why = v["responseDetails"].as_str().unwrap_or("").chars().take(120).collect::<String>();
            return Err(format!("Usługa tłumaczenia odrzuciła zapytanie (kod {code}). {why}").trim().to_string());
        }
        result.push_str(&decode_entities(translated));
        result.push_str(sep);
    }
    Ok(TranslateOut { translated: result.trim_end().to_string(), truncated })
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
    fn decodes_entities() {
        assert_eq!(decode_entities("Tom &amp; Jerry &#39;x&#39; &quot;y&quot; &lt;b&gt;"), "Tom & Jerry 'x' \"y\" <b>");
        assert_eq!(decode_entities("&#x41;&#322; a & b &nieznane; &"), "Ał a & b &nieznane; &");
    }
}

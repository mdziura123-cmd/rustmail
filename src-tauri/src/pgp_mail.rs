//! Komendy OpenPGP dla interfejsu: klucze, sprawdzanie odbiorców, odszyfrowanie i weryfikacja podpisów.
//!
//! Odszyfrowanie odbywa się tylko na żądanie (przycisk „Odszyfruj”) i tylko w pamięci: odszyfrowana treść
//! nie trafia do lokalnej pamięci podręcznej ani na dysk (chyba że użytkownik zapisze załącznik).
//! GnuPG działa w osobnym wątku, żeby nie blokować aplikacji.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use mailparse::{DispositionType, MailHeaderMap, ParsedMail};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::auth::{self, AppState};
use crate::pgp::{self, Key, SigInfo};
use crate::{gmail, imap_io, secrets};

const MAX_DECRYPTED_ATTACHMENTS: usize = 40 * 1024 * 1024;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| format!("Błąd wewnętrzny: {e}"))?
}

fn clip(s: &str, n: usize) -> String {
    s.chars().filter(|c| !c.is_control() || *c == '\n').take(n).collect()
}

// ---------- Stan i klucze ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PgpStatus {
    pub available: bool,
    pub version: String,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyView {
    pub fingerprint: String,
    pub key_id: String,
    pub uids: Vec<String>,
    pub emails: Vec<String>,
    pub algo: String,
    pub created_ms: i64,
    pub expires_ms: i64,
    pub revoked: bool,
    pub expired: bool,
    pub can_encrypt: bool,
    pub can_sign: bool,
    pub secret: bool,
}

impl From<Key> for KeyView {
    fn from(k: Key) -> Self {
        KeyView {
            fingerprint: k.fingerprint,
            key_id: k.key_id,
            uids: k.uids,
            emails: k.emails,
            algo: k.algo,
            created_ms: k.created * 1000,
            expires_ms: k.expires * 1000,
            revoked: k.revoked,
            expired: k.expired || k.disabled,
            can_encrypt: k.can_encrypt,
            can_sign: k.can_sign,
            secret: k.secret,
        }
    }
}

#[tauri::command]
pub async fn pgp_status() -> Result<PgpStatus, String> {
    blocking(|| {
        Ok(match pgp::gpg().and_then(|g| g.version()) {
            Ok(v) if !v.is_empty() => PgpStatus { available: true, version: v, error: None },
            Ok(_) => PgpStatus { available: false, version: String::new(), error: Some("GnuPG nie podał wersji.".to_string()) },
            Err(e) => PgpStatus { available: false, version: String::new(), error: Some(e) },
        })
    })
    .await
}

#[tauri::command]
pub async fn pgp_keys() -> Result<Vec<KeyView>, String> {
    blocking(|| {
        let mut keys = pgp::gpg()?.list_keys()?;
        keys.sort_by(|a, b| b.secret.cmp(&a.secret).then_with(|| a.uids.first().cmp(&b.uids.first())));
        Ok(keys.into_iter().map(KeyView::from).collect())
    })
    .await
}

#[tauri::command]
pub async fn pgp_generate(name: String, email: String, passphrase: String) -> Result<String, String> {
    blocking(move || pgp::gpg()?.generate(&name, &email, &passphrase)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportView {
    pub fingerprints: Vec<String>,
    pub new_keys: u32,
    pub unchanged: u32,
    pub secret: u32,
}

fn import_bytes(data: Vec<u8>) -> Result<ImportView, String> {
    let r = pgp::gpg()?.import(&data)?;
    Ok(ImportView { fingerprints: r.fingerprints, new_keys: r.new_keys, unchanged: r.unchanged, secret: r.secret })
}

/// Import klucza z wklejonego tekstu albo z pliku (base64).
#[tauri::command]
pub async fn pgp_import(text: Option<String>, data_base64: Option<String>) -> Result<ImportView, String> {
    let data = match (text, data_base64) {
        (Some(t), _) if !t.trim().is_empty() => t.into_bytes(),
        (_, Some(b)) => STANDARD.decode(b.trim()).map_err(|_| "Nie można odczytać pliku klucza.".to_string())?,
        _ => return Err("Wklej klucz albo wybierz plik.".to_string()),
    };
    blocking(move || import_bytes(data)).await
}

/// Import klucza z załącznika wiadomości Gmail.
#[tauri::command]
pub async fn pgp_import_gmail_att(state: State<'_, AppState>, message_id: String, attachment_id: String) -> Result<ImportView, String> {
    let data = gmail::download_attachment(&state, &message_id, &attachment_id).await?;
    blocking(move || import_bytes(data)).await
}

/// Import klucza z załącznika wiadomości IMAP (numer jak przy pobieraniu załącznika).
#[tauri::command]
pub async fn pgp_import_imap_att(account: String, folder: String, uid: u32, index: u32) -> Result<ImportView, String> {
    let data = imap_io::attachment_bytes(&account, &folder, uid, index).await?;
    blocking(move || import_bytes(data)).await
}

#[tauri::command]
pub async fn pgp_export_public(fingerprint: String) -> Result<String, String> {
    blocking(move || pgp::gpg()?.export_public(&fingerprint)).await
}

/// Zapisuje klucz publiczny albo kopię klucza prywatnego do Pobranych.
#[tauri::command]
pub async fn pgp_save_key(app: AppHandle, fingerprint: String, secret: bool, passphrase: Option<String>) -> Result<crate::commands::SavedFile, String> {
    let fpr = fingerprint.clone();
    let text = blocking(move || {
        let g = pgp::gpg()?;
        if secret { g.export_secret(&fpr, passphrase.as_deref()) } else { g.export_public(&fpr) }
    })
    .await?;
    let short = fingerprint.get(fingerprint.len().saturating_sub(16)..).unwrap_or("");
    let name = if secret { format!("OpenPGP_0x{short}_PRYWATNY.asc") } else { format!("OpenPGP_0x{short}.asc") };
    crate::commands::save_scanned(&app, &name, text.as_bytes()).await
}

#[tauri::command]
pub async fn pgp_delete(fingerprint: String) -> Result<(), String> {
    blocking(move || pgp::gpg()?.delete(&fingerprint)).await
}

#[tauri::command]
pub async fn pgp_forget() -> Result<(), String> {
    blocking(|| {
        pgp::gpg()?.forget_passphrases();
        Ok(())
    })
    .await
}

// ---------- Okno nowej wiadomości ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipientCheck {
    pub available: bool,
    /// Odbiorcy bez klucza do szyfrowania.
    pub missing: Vec<String>,
    /// Czy nadawca ma własny klucz (podpis i odczyt w Wysłanych).
    pub own_key: bool,
}

fn sender_address(account: Option<&str>) -> Result<String, String> {
    match account {
        Some(a) if !a.is_empty() => Ok(a.trim().to_ascii_lowercase()),
        _ => Ok(secrets::get(auth::KEY_EMAIL)?.ok_or("Nie jesteś zalogowany.")?),
    }
}

#[tauri::command]
pub async fn pgp_check_recipients(account: Option<String>, to: String, cc: String, bcc: String) -> Result<RecipientCheck, String> {
    let from = sender_address(account.as_deref())?;
    blocking(move || {
        let g = match pgp::gpg() {
            Ok(g) if g.version().is_ok() => g,
            _ => return Ok(RecipientCheck { available: false, missing: vec![], own_key: false }),
        };
        let keys = g.list_keys()?;
        // Niepoprawne adresy zgłosi dopiero wysyłka – tu sprawdzamy tylko te, które da się odczytać.
        let mut addrs: Vec<String> = Vec::new();
        for field in [&to, &cc, &bcc] {
            if let Ok(list) = crate::compose::parse_recipients(field) {
                addrs.extend(list);
            }
        }
        Ok(RecipientCheck {
            available: true,
            missing: crate::pgp_send::missing_keys(&keys, &addrs),
            own_key: crate::pgp_send::signing_key(&keys, &from).is_ok() && pgp::key_for(&keys, &from, true, true).is_some(),
        })
    })
    .await
}

// ---------- Odczyt ----------

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SigView {
    /// good | bad | expired_key | revoked_key | expired_sig | no_key | error
    pub status: String,
    pub fingerprint: String,
    pub key_id: String,
    pub signer: String,
    /// Czy klucz podpisu należy do adresu z pola „Od”.
    pub from_matches: bool,
    pub created_ms: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedAtt {
    pub name: String,
    pub mime: String,
    pub size: usize,
    pub data_base64: String,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    /// encrypted | signed | none
    pub kind: String,
    pub decrypted: bool,
    pub text: String,
    pub html: String,
    pub attachments: Vec<OpenedAtt>,
    pub signature: Option<SigView>,
}

fn sig_view(s: SigInfo, keys: &[Key], from_email: &str) -> SigView {
    let key = keys.iter().find(|k| {
        (!s.fingerprint.is_empty() && k.fingerprint.eq_ignore_ascii_case(&s.fingerprint))
            || (!s.key_id.is_empty() && k.fingerprint.to_ascii_uppercase().ends_with(&s.key_id.to_ascii_uppercase()))
    });
    let from = from_email.trim().to_ascii_lowercase();
    let signer = key.and_then(|k| k.uids.first().cloned()).unwrap_or_else(|| clip(&s.signer, 200));
    SigView {
        from_matches: !from.is_empty() && key.map(|k| k.emails.contains(&from)).unwrap_or(false),
        fingerprint: key.map(|k| k.fingerprint.clone()).unwrap_or(s.fingerprint),
        key_id: s.key_id,
        signer,
        created_ms: s.created * 1000,
        status: s.status,
    }
}

fn att_name(part: &ParsedMail) -> Option<String> {
    let cd = part.get_content_disposition();
    let fname = cd.params.get("filename").or_else(|| part.ctype.params.get("name")).cloned();
    if cd.disposition == DispositionType::Attachment || fname.is_some() {
        Some(fname.unwrap_or_else(|| "załącznik".to_string()))
    } else {
        None
    }
}

/// Zbiera treść i załączniki odszyfrowanej części MIME.
fn walk(part: &ParsedMail, out: &mut Opened, total: &mut usize) {
    if !part.subparts.is_empty() {
        for s in &part.subparts {
            walk(s, out, total);
        }
        return;
    }
    let mime = part.ctype.mimetype.to_ascii_lowercase();
    if let Some(name) = att_name(part) {
        // Podpis wewnątrz zaszyfrowanej wiadomości nie jest „załącznikiem” dla użytkownika.
        if mime == "application/pgp-signature" {
            return;
        }
        let data = part.get_body_raw().unwrap_or_default();
        *total += data.len();
        if *total > MAX_DECRYPTED_ATTACHMENTS {
            return;
        }
        out.attachments.push(OpenedAtt { name: clip(&name, 150), mime: clip(&mime, 100), size: data.len(), data_base64: STANDARD.encode(&data) });
    } else if mime == "text/plain" {
        if out.text.is_empty() {
            out.text = part.get_body().unwrap_or_default();
        }
    } else if mime == "text/html" && out.html.is_empty() {
        out.html = part.get_body().unwrap_or_default();
    }
}

fn mime_of(m: &ParsedMail) -> String {
    m.ctype.mimetype.to_ascii_lowercase()
}

/// Sprawdza `multipart/signed` na najwyższym poziomie `raw` (wiadomość albo odszyfrowana część).
fn verify_signed(g: &pgp::Gpg, raw: &[u8], mail: &ParsedMail) -> Result<Option<SigInfo>, String> {
    let boundary = mail.ctype.params.get("boundary").cloned().unwrap_or_default();
    let start = pgp::body_start(raw).ok_or("Uszkodzona wiadomość podpisana.")?;
    let signed = pgp::signed_part(&raw[start..], &boundary).ok_or("Nie udało się wydzielić podpisanej części wiadomości.")?;
    let sig_part = mail.subparts.iter().find(|p| mime_of(p) == "application/pgp-signature").ok_or("W wiadomości brakuje podpisu.")?;
    let sig = sig_part.get_body_raw().map_err(|e| e.to_string())?;
    g.verify_detached(&signed, &sig).map(Some)
}

/// Odczytuje wiadomość OpenPGP: odszyfrowuje (PGP/MIME albo w treści) i sprawdza podpis.
pub fn open_raw(raw: &[u8], passphrase: Option<&str>) -> Result<Opened, String> {
    let mail = mailparse::parse_mail(raw).map_err(|e| format!("Nie udało się odczytać wiadomości: {e}"))?;
    let from = mail.headers.get_first_value("From").unwrap_or_default();
    let from_email = crate::contacts::parse_list(&from).into_iter().next().map(|a| a.email).unwrap_or_default();
    let g = pgp::gpg()?;
    let mut out = Opened { kind: "none".to_string(), ..Default::default() };
    let mut sig: Option<SigInfo> = None;
    let top = mime_of(&mail);

    if top == "multipart/encrypted" {
        out.kind = "encrypted".to_string();
        let part = mail
            .subparts
            .iter()
            .find(|p| mime_of(p) == "application/octet-stream")
            .or_else(|| mail.subparts.get(1))
            .ok_or("Uszkodzona wiadomość zaszyfrowana.")?;
        let ct = part.get_body_raw().map_err(|e| e.to_string())?;
        let d = g.decrypt(&ct, passphrase)?;
        sig = d.signature;
        let inner_raw = pgp::to_crlf(&d.plain);
        let inner = mailparse::parse_mail(&inner_raw).map_err(|e| format!("Nie udało się odczytać odszyfrowanej treści: {e}"))?;
        // Najpierw podpisana, potem zaszyfrowana (osobny podpis wewnątrz).
        if mime_of(&inner) == "multipart/signed" && sig.is_none() {
            sig = verify_signed(&g, &inner_raw, &inner)?;
        }
        let mut total = 0usize;
        walk(&inner, &mut out, &mut total);
        out.decrypted = true;
    } else if top == "multipart/signed" {
        out.kind = "signed".to_string();
        sig = verify_signed(&g, raw, &mail)?;
    } else {
        // Wiadomość w starym stylu: blok OpenPGP w treści.
        let mut probe = Opened::default();
        let mut total = 0usize;
        walk(&mail, &mut probe, &mut total);
        let text = if probe.text.is_empty() { crate::gmail::strip_html(&probe.html) } else { probe.text };
        if let Some(block) = pgp::extract_armor(&text, "PGP MESSAGE") {
            out.kind = "encrypted".to_string();
            let d = g.decrypt(block.as_bytes(), passphrase)?;
            sig = d.signature;
            out.text = String::from_utf8_lossy(&d.plain).into_owned();
            out.decrypted = true;
        } else if let Some(block) = pgp::extract_armor(&text, "PGP SIGNED MESSAGE") {
            out.kind = "signed".to_string();
            sig = g.decrypt(block.as_bytes(), None)?.signature;
        }
    }
    if let Some(s) = sig {
        let keys = g.list_keys().unwrap_or_default();
        out.signature = Some(sig_view(s, &keys, &from_email));
    }
    out.text = out.text.chars().take(500_000).collect();
    out.html = out.html.chars().take(1_500_000).collect();
    Ok(out)
}

#[tauri::command]
pub async fn pgp_open_gmail(state: State<'_, AppState>, message_id: String, passphrase: Option<String>) -> Result<Opened, String> {
    let raw = gmail::message_source(&state, &message_id).await?;
    blocking(move || open_raw(&raw, passphrase.as_deref())).await
}

#[tauri::command]
pub async fn pgp_open_imap(account: String, folder: String, uid: u32, passphrase: Option<String>) -> Result<Opened, String> {
    let raw = imap_io::raw_message(&account, &folder, uid).await?;
    blocking(move || open_raw(&raw, passphrase.as_deref())).await
}

/// Zapisuje odszyfrowany załącznik w Pobranych (z tymi samymi kontrolami co zwykłe załączniki).
#[tauri::command]
pub async fn pgp_save_attachment(app: AppHandle, name: String, data_base64: String) -> Result<crate::commands::SavedFile, String> {
    let bytes = STANDARD.decode(data_base64.trim()).map_err(|_| "Nie można odczytać załącznika.".to_string())?;
    crate::commands::save_scanned(&app, &name, &bytes).await
}

//! Blokada aplikacji kodem PIN (lub hasłem). Kod nigdy nie jest zapisywany wprost: w systemowym
//! magazynie haseł leży tylko sól i skrót PBKDF2-HMAC-SHA256. Po kilku błędnych próbach kolejne
//! są wstrzymywane na coraz dłużej.
//!
//! To ochrona przed podejrzeniem poczty przez kogoś przy odblokowanym komputerze – nie zastępuje
//! blokady ekranu systemu ani szyfrowania dysku.

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};

use crate::secrets;

const KEY: &str = "app_lock_pin";
const ITERATIONS: u32 = 210_000;
const SALT_LEN: usize = 16;
const HASH_LEN: usize = 32;
const FREE_TRIES: u32 = 5;

/// PIN lub hasło: 4–64 znaki, bez znaków sterujących i bez spacji na brzegach.
pub fn valid_pin(pin: &str) -> Result<(), String> {
    let n = pin.chars().count();
    if n < 4 {
        return Err("Kod musi mieć co najmniej 4 znaki.".to_string());
    }
    if n > 64 {
        return Err("Kod może mieć najwyżej 64 znaki.".to_string());
    }
    if pin.chars().any(|c| c.is_control()) || pin.trim() != pin {
        return Err("Kod nie może zawierać znaków sterujących ani spacji na początku i końcu.".to_string());
    }
    Ok(())
}

/// Czas wstrzymania po `fails` kolejnych błędach: pierwsze 5 prób bez czekania,
/// potem 30 s, 60 s, 120 s… najwyżej 15 minut.
pub fn backoff(fails: u32) -> Duration {
    if fails < FREE_TRIES {
        return Duration::ZERO;
    }
    let exp = (fails - FREE_TRIES).min(5);
    Duration::from_secs((30u64 << exp).min(900))
}

/// Zapis: `v1$iteracje$sól$skrót` (base64).
pub fn encode(iterations: u32, salt: &[u8], hash: &[u8]) -> String {
    format!("v1${iterations}${}${}", STANDARD.encode(salt), STANDARD.encode(hash))
}

pub fn decode(s: &str) -> Option<(u32, Vec<u8>, Vec<u8>)> {
    let mut it = s.split('$');
    if it.next()? != "v1" {
        return None;
    }
    let iters: u32 = it.next()?.parse().ok()?;
    let salt = STANDARD.decode(it.next()?).ok()?;
    let hash = STANDARD.decode(it.next()?).ok()?;
    if it.next().is_some() || iters == 0 || salt.len() < 8 || hash.len() != HASH_LEN {
        return None;
    }
    Some((iters, salt, hash))
}

pub fn derive(pin: &str, salt: &[u8], iterations: u32) -> [u8; HASH_LEN] {
    let mut out = [0u8; HASH_LEN];
    let it = NonZeroU32::new(iterations).unwrap_or(NonZeroU32::MIN);
    pbkdf2::derive(pbkdf2::PBKDF2_HMAC_SHA256, it, salt, pin.as_bytes(), &mut out);
    out
}

/// Porównanie w stałym czasie (ring).
pub fn matches(pin: &str, stored: &str) -> bool {
    let Some((iters, salt, hash)) = decode(stored) else { return false };
    let Some(it) = NonZeroU32::new(iters) else { return false };
    pbkdf2::verify(pbkdf2::PBKDF2_HMAC_SHA256, it, &salt, pin.as_bytes(), &hash).is_ok()
}

fn make(pin: &str) -> Result<String, String> {
    let mut salt = [0u8; SALT_LEN];
    SystemRandom::new().fill(&mut salt).map_err(|_| "Brak bezpiecznego źródła losowości.".to_string())?;
    Ok(encode(ITERATIONS, &salt, &derive(pin, &salt, ITERATIONS)))
}

/// Licznik błędnych prób (tylko w pamięci; ponowne uruchomienie programu go nie omija,
/// bo i tak trzeba wpisać kod, a każda próba kosztuje ~0,2 s obliczeń).
struct Attempts {
    fails: u32,
    until: Option<Instant>,
}

static ATTEMPTS: Mutex<Attempts> = Mutex::new(Attempts { fails: 0, until: None });

/// Wspólny stan blokady wszystkich okien (poczta i kalendarz). Zmiana jest ogłaszana zdarzeniem `app-lock`.
static LOCKED: AtomicBool = AtomicBool::new(false);

fn announce(app: &tauri::AppHandle, locked: bool) {
    use tauri::Emitter;
    LOCKED.store(locked, Ordering::SeqCst);
    let _ = app.emit("app-lock", locked);
}

fn throttle_check() -> Result<(), String> {
    let a = ATTEMPTS.lock().map_err(|_| "Błąd blokady.".to_string())?;
    if let Some(u) = a.until {
        let now = Instant::now();
        if u > now {
            let secs = (u - now).as_secs() + 1;
            return Err(format!("Zbyt wiele błędnych prób. Spróbuj ponownie za {secs} s."));
        }
    }
    Ok(())
}

fn record(ok: bool) {
    if let Ok(mut a) = ATTEMPTS.lock() {
        if ok {
            a.fails = 0;
            a.until = None;
        } else {
            a.fails = a.fails.saturating_add(1);
            let d = backoff(a.fails);
            a.until = if d.is_zero() { None } else { Some(Instant::now() + d) };
        }
    }
}

fn check(pin: &str) -> Result<bool, String> {
    throttle_check()?;
    let Some(stored) = secrets::get(KEY)? else { return Ok(true) };
    let ok = matches(pin, &stored);
    record(ok);
    Ok(ok)
}

/// Czy ustawiono kod blokady.
#[tauri::command]
pub fn lock_status() -> Result<bool, String> {
    Ok(secrets::get(KEY)?.is_some())
}

/// Ustawia lub zmienia kod. Zmiana wymaga podania obecnego kodu.
#[tauri::command]
pub async fn lock_set(current: Option<String>, pin: String) -> Result<(), String> {
    valid_pin(&pin)?;
    tauri::async_runtime::spawn_blocking(move || {
        if secrets::get(KEY)?.is_some() && !check(current.as_deref().unwrap_or(""))? {
            return Err("Obecny kod jest nieprawidłowy.".to_string());
        }
        secrets::set(KEY, &make(&pin)?)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Usuwa kod (wymaga podania obecnego kodu).
#[tauri::command]
pub async fn lock_clear(current: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !check(&current)? {
            return Err("Kod jest nieprawidłowy.".to_string());
        }
        LOCKED.store(false, Ordering::SeqCst);
        secrets::delete(KEY)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Sprawdza kod przy odblokowaniu. Poprawny kod odblokowuje wszystkie okna.
/// Błąd oznacza wstrzymanie po zbyt wielu próbach.
#[tauri::command]
pub async fn lock_verify(app: tauri::AppHandle, pin: String) -> Result<bool, String> {
    let ok = tauri::async_runtime::spawn_blocking(move || check(&pin)).await.map_err(|e| e.to_string())??;
    if ok {
        announce(&app, false);
    }
    Ok(ok)
}

/// Blokuje wszystkie okna (tylko gdy ustawiono kod).
#[tauri::command]
pub fn lock_engage(app: tauri::AppHandle) -> Result<bool, String> {
    if secrets::get(KEY)?.is_none() {
        return Ok(false);
    }
    announce(&app, true);
    Ok(true)
}

/// Czy aplikacja jest teraz zablokowana (np. dla okna kalendarza otwieranego w czasie blokady).
#[tauri::command]
pub fn lock_state() -> bool {
    LOCKED.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_rules() {
        assert!(valid_pin("1234").is_ok());
        assert!(valid_pin("hasło z spacją w środku").is_ok());
        assert!(valid_pin("123").is_err());
        assert!(valid_pin(" 1234").is_err());
        assert!(valid_pin("12\n34").is_err());
        assert!(valid_pin(&"x".repeat(65)).is_err());
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff(0), Duration::ZERO);
        assert_eq!(backoff(4), Duration::ZERO);
        assert_eq!(backoff(5), Duration::from_secs(30));
        assert_eq!(backoff(6), Duration::from_secs(60));
        assert_eq!(backoff(9), Duration::from_secs(480));
        assert_eq!(backoff(10), Duration::from_secs(900));
        assert_eq!(backoff(100), Duration::from_secs(900));
    }

    #[test]
    fn hash_roundtrip() {
        let salt = [7u8; 16];
        let stored = encode(1000, &salt, &derive("4821", &salt, 1000));
        assert!(matches("4821", &stored));
        assert!(!matches("4822", &stored));
        assert!(!matches("4821", "v1$1000$bad$bad"));
        assert!(!matches("4821", ""));
        assert!(decode("v2$1$AAAAAAAAAAA=$AAAA").is_none());
    }
}

//! Systemowy magazyn sekretów (KDE Wallet / GNOME Keyring przez Secret Service).
//! Tokeny OAuth nigdy nie trafiają do plików ani do bazy danych.

use keyring::Entry;

const SERVICE: &str = "pl.rustmail.client";

fn friendly(e: keyring::Error) -> String {
    format!("Systemowy magazyn haseł (KDE Wallet / GNOME Keyring) jest niedostępny: {e}")
}

fn entry(key: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, key).map_err(friendly)
}

pub fn set(key: &str, value: &str) -> Result<(), String> {
    entry(key)?.set_password(value).map_err(friendly)
}

pub fn get(key: &str) -> Result<Option<String>, String> {
    match entry(key)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(friendly(e)),
    }
}

pub fn delete(key: &str) -> Result<(), String> {
    match entry(key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(friendly(e)),
    }
}

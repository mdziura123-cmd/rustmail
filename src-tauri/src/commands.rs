//! Komendy Tauri udostępniane frontendowi. Każda komenda jest jawnie
//! zarejestrowana w lib.rs; frontend nie ma dostępu do powłoki ani systemu plików.

use serde::Serialize;

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
}

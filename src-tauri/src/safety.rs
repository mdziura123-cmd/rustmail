//! Czyste reguły bezpieczeństwa (bez sieci i bez zewnętrznych bibliotek), w pełni testowalne:
//! - które adresy IP wolno pobierać (ochrona sieci lokalnej przed żądaniami z wiadomości),
//! - które adresy wolno otworzyć w przeglądarce,
//! - bezpieczne nazwy i ścieżki zapisywanych załączników.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};

/// Czy adres IP jest publiczny. Odrzuca pętlę zwrotną, sieci prywatne, link-local, CGNAT,
/// multicast, dokumentacyjne i adresy IPv6 odwzorowane na prywatne IPv4.
pub fn ip_is_public(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_public(v4),
        IpAddr::V6(v6) => v6_public(v6),
    }
}

fn v4_public(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || ip.is_documentation()
        || ip.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (64..=127).contains(&o[1]))
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
        || o[0] >= 240)
}

fn v6_public(ip: &Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_public(&v4);
    }
    let s = ip.segments();
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00 // unikatowe lokalne fc00::/7
        || (s[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
        || (s[0] == 0x2001 && s[1] == 0x0db8)) // dokumentacyjne
}

/// Dozwolone typy obrazów do pobrania (bez SVG, który może zawierać aktywną treść).
pub fn valid_image_mime(ct: &str) -> Option<&'static str> {
    let base = ct.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    match base.as_str() {
        "image/png" => Some("image/png"),
        "image/jpeg" | "image/jpg" => Some("image/jpeg"),
        "image/gif" => Some("image/gif"),
        "image/webp" => Some("image/webp"),
        _ => None,
    }
}

/// Adres do pobrania obrazu: tylko https, bez danych logowania, bez znaków sterujących.
pub fn check_remote_url(url: &str) -> Result<(), String> {
    let u = url;
    if u.len() > 2000 || u.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("Niepoprawny adres obrazu.".to_string());
    }
    let Some(rest) = u.strip_prefix("https://") else {
        return Err("Pobieranie obrazów jest dozwolone tylko przez https.".to_string());
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return Err("Niepoprawny adres obrazu.".to_string());
    }
    Ok(())
}

/// Adres otwierany w przeglądarce systemowej: http, https lub mailto.
pub fn check_open_url(url: &str) -> Result<(), String> {
    let u = url;
    if u.len() > 2000 || u.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("Niepoprawny adres.".to_string());
    }
    let lower = u.to_ascii_lowercase();
    let ok = lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:");
    if !ok || u.starts_with('-') {
        return Err("Można otwierać tylko adresy http, https i mailto.".to_string());
    }
    Ok(())
}

/// Nazwa pliku załącznika: bez ścieżek, bez znaków sterujących, bez ukrytych plików, bez ".." .
pub fn safe_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_control() || "<>:\"|?*".contains(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim().to_string();
    let cleaned: String = cleaned.chars().take(150).collect();
    if cleaned.is_empty() { "zalacznik".to_string() } else { cleaned }
}

/// Rozszerzenia, które po uruchomieniu mogą wykonać kod lub zmienić system.
pub fn risky_extension(name: &str) -> bool {
    const RISKY: &[&str] = &[
        "exe", "msi", "bat", "cmd", "com", "scr", "pif", "vbs", "vbe", "js", "jse", "wsf", "wsh", "ps1", "psm1",
        "jar", "sh", "bash", "run", "bin", "appimage", "desktop", "app", "dmg", "pkg", "deb", "rpm", "apk",
        "lnk", "reg", "hta", "cpl", "dll", "iso", "img", "docm", "xlsm", "pptm", "dotm", "xlam", "py", "pl", "rb",
    ];
    let lower = name.to_ascii_lowercase();
    // sprawdzamy każdy człon po kropce, aby wychwycić "faktura.pdf.exe"
    lower.split('.').skip(1).any(|ext| RISKY.contains(&ext.trim()))
}

/// Wolna nazwa w katalogu: "plik.pdf", potem "plik (1).pdf" itd. Nigdy nie nadpisuje istniejącego pliku.
pub fn unique_path(dir: &Path, name: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let first = dir.join(name);
    if !exists(&first) {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 1..10_000 {
        let p = dir.join(format!("{stem} ({n}){ext}"));
        if !exists(&p) {
            return p;
        }
    }
    dir.join(format!("{stem} (kopia){ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn private_and_special_addresses_are_rejected() {
        for a in [
            "127.0.0.1", "10.1.2.3", "172.16.0.1", "172.31.255.255", "192.168.1.1", "169.254.169.254", "0.0.0.0",
            "100.64.0.1", "224.0.0.1", "255.255.255.255", "192.0.2.1", "198.18.0.1", "::1", "::", "fe80::1", "fc00::1",
            "fd12:3456::1", "ff02::1", "::ffff:192.168.0.1", "::ffff:127.0.0.1", "2001:db8::1",
        ] {
            assert!(!ip_is_public(&ip(a)), "powinno być odrzucone: {a}");
        }
    }

    #[test]
    fn public_addresses_are_accepted() {
        for a in ["8.8.8.8", "1.1.1.1", "172.32.0.1", "93.184.216.34", "2606:4700:4700::1111", "::ffff:8.8.8.8"] {
            assert!(ip_is_public(&ip(a)), "powinno być dozwolone: {a}");
        }
    }

    #[test]
    fn image_mime_whitelist() {
        assert_eq!(valid_image_mime("image/PNG; charset=x"), Some("image/png"));
        assert_eq!(valid_image_mime("image/jpg"), Some("image/jpeg"));
        assert_eq!(valid_image_mime("image/svg+xml"), None);
        assert_eq!(valid_image_mime("text/html"), None);
        assert_eq!(valid_image_mime(""), None);
    }

    #[test]
    fn remote_urls_must_be_plain_https() {
        assert!(check_remote_url("https://cdn.example.com/a.png?x=1").is_ok());
        for bad in [
            "http://example.com/a.png", "file:///etc/passwd", "https://user:pw@example.com/a", "https://",
            "https://a.com/x y", "data:image/png;base64,AAA", "https://a.com/\n", "ftp://a.com/a",
        ] {
            assert!(check_remote_url(bad).is_err(), "powinno być odrzucone: {bad:?}");
        }
    }

    #[test]
    fn open_urls_are_limited_to_safe_schemes() {
        assert!(check_open_url("https://example.com/a?b=c").is_ok());
        assert!(check_open_url("HTTP://example.com").is_ok());
        assert!(check_open_url("mailto:a@b.pl").is_ok());
        for bad in ["javascript:alert(1)", "file:///etc/passwd", "-o /bin/sh", "https://a b", "data:text/html,x", "", "smb://x/y"] {
            assert!(check_open_url(bad).is_err(), "powinno być odrzucone: {bad:?}");
        }
    }

    #[test]
    fn filenames_are_sanitized() {
        assert_eq!(safe_filename("../../etc/passwd"), "passwd");
        assert_eq!(safe_filename("C:\\Users\\a\\faktura.pdf"), "faktura.pdf");
        assert_eq!(safe_filename(".bashrc"), "bashrc");
        assert_eq!(safe_filename("a\nb<c>.txt"), "a_b_c_.txt");
        assert_eq!(safe_filename(".."), "zalacznik");
        assert_eq!(safe_filename(""), "zalacznik");
        assert!(safe_filename(&"x".repeat(500)).chars().count() <= 150);
    }

    #[test]
    fn risky_extensions_include_double_extensions() {
        assert!(risky_extension("faktura.pdf.exe"));
        assert!(risky_extension("SKRYPT.SH"));
        assert!(risky_extension("raport.docm"));
        assert!(risky_extension("RustMail.AppImage"));
        assert!(!risky_extension("zdjecie.jpg"));
        assert!(!risky_extension("umowa.pdf"));
        assert!(!risky_extension("bezrozszerzenia"));
    }

    #[test]
    fn unique_path_never_overwrites() {
        let dir = Path::new("/tmp/x");
        let taken = [dir.join("a.pdf"), dir.join("a (1).pdf")];
        let exists = |p: &Path| taken.iter().any(|t| t == p);
        assert_eq!(unique_path(dir, "b.pdf", exists), dir.join("b.pdf"));
        assert_eq!(unique_path(dir, "a.pdf", exists), dir.join("a (2).pdf"));
        let only = [dir.join("noext")];
        assert_eq!(unique_path(dir, "noext", |p| only.iter().any(|t| t == p)), dir.join("noext (1)"));
    }
}

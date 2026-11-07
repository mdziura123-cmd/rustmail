//! Rozpoznawanie rzeczywistego typu pliku po jego zawartości (nagłówku), niezależnie od nazwy.
//! Wyłącznie biblioteka standardowa. Wykrywa m.in. program wykonywalny ukryty pod nazwą „faktura.pdf”.

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Kind {
    Exe,
    Elf,
    MachO,
    Script,
    Html,
    Zip,
    Pdf,
    Png,
    Jpeg,
    Gif,
    Rar,
    SevenZ,
    Gzip,
    Ole,
    Other,
}

pub fn sniff(b: &[u8]) -> Kind {
    let starts = |p: &[u8]| b.len() >= p.len() && &b[..p.len()] == p;
    if starts(b"MZ") {
        return Kind::Exe;
    }
    if starts(&[0x7F, b'E', b'L', b'F']) {
        return Kind::Elf;
    }
    if starts(&[0xCA, 0xFE, 0xBA, 0xBE])
        || starts(&[0xFE, 0xED, 0xFA, 0xCE])
        || starts(&[0xFE, 0xED, 0xFA, 0xCF])
        || starts(&[0xCF, 0xFA, 0xED, 0xFE])
        || starts(&[0xCE, 0xFA, 0xED, 0xFE])
    {
        return Kind::MachO;
    }
    if starts(b"#!") {
        return Kind::Script;
    }
    if starts(b"%PDF-") {
        return Kind::Pdf;
    }
    if starts(&[0x89, b'P', b'N', b'G']) {
        return Kind::Png;
    }
    if starts(&[0xFF, 0xD8, 0xFF]) {
        return Kind::Jpeg;
    }
    if starts(b"GIF8") {
        return Kind::Gif;
    }
    if starts(b"PK\x03\x04") || starts(b"PK\x05\x06") {
        return Kind::Zip;
    }
    if starts(b"Rar!") {
        return Kind::Rar;
    }
    if starts(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Kind::SevenZ;
    }
    if starts(&[0x1F, 0x8B]) {
        return Kind::Gzip;
    }
    if starts(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return Kind::Ole;
    }
    let head: String = b.iter().take(64).map(|c| (*c as char).to_ascii_lowercase()).collect();
    let head = head.trim_start();
    if head.starts_with("<!doctype html") || head.starts_with("<html") || head.starts_with("<script") || head.starts_with("<svg") {
        return Kind::Html;
    }
    Kind::Other
}

pub fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i + 1 < name.len() => name[i + 1..].trim().to_ascii_lowercase(),
        _ => String::new(),
    }
}

const EXEC_EXT: &[&str] = &[
    "exe", "dll", "scr", "com", "sys", "so", "bin", "run", "appimage", "sh", "bash", "py", "pl", "rb", "elf", "o", "dylib",
    "app", "class", "msi", "ocx", "cpl",
];
const DOC_EXT: &[&str] = &[
    "pdf", "jpg", "jpeg", "png", "gif", "bmp", "webp", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "txt", "rtf",
    "zip", "csv", "mp3", "mp4", "avi", "mov",
];
const MEDIA_EXT: &[(&str, Kind)] = &[("pdf", Kind::Pdf), ("jpg", Kind::Jpeg), ("jpeg", Kind::Jpeg), ("png", Kind::Png), ("gif", Kind::Gif)];

/// Zwraca opis problemu, jeśli zawartość pliku nie zgadza się z jego nazwą w sposób, który może być niebezpieczny
/// albo mylący. `None` oznacza brak zastrzeżeń.
pub fn mismatch(name: &str, bytes: &[u8]) -> Option<String> {
    let ext = ext_of(name);
    if ext.is_empty() {
        return None;
    }
    let kind = sniff(bytes);
    let is_exec_content = matches!(kind, Kind::Exe | Kind::Elf | Kind::MachO);
    if is_exec_content && !EXEC_EXT.contains(&ext.as_str()) {
        return Some(format!("Plik ma rozszerzenie „.{ext}”, ale w środku jest program wykonywalny – to typowa sztuczka złośliwych załączników."));
    }
    if matches!(kind, Kind::Script) && DOC_EXT.contains(&ext.as_str()) && ext != "txt" {
        return Some(format!("Plik ma rozszerzenie „.{ext}”, ale w środku jest skrypt."));
    }
    if matches!(kind, Kind::Html) && DOC_EXT.contains(&ext.as_str()) && ext != "txt" {
        return Some(format!("Plik ma rozszerzenie „.{ext}”, ale w środku jest strona HTML lub skrypt."));
    }
    if let Some((_, expected)) = MEDIA_EXT.iter().find(|(e, _)| *e == ext) {
        let known_other = !matches!(kind, Kind::Other | Kind::Script | Kind::Html | Kind::Exe | Kind::Elf | Kind::MachO) && kind != *expected;
        if known_other {
            return Some(format!("Typ zawartości pliku nie zgadza się z rozszerzeniem „.{ext}”."));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_common_formats() {
        assert_eq!(sniff(b"MZ\x90\x00"), Kind::Exe);
        assert_eq!(sniff(b"\x7fELF\x02"), Kind::Elf);
        assert_eq!(sniff(b"#!/bin/sh\necho"), Kind::Script);
        assert_eq!(sniff(b"%PDF-1.7"), Kind::Pdf);
        assert_eq!(sniff(b"\x89PNG\r\n"), Kind::Png);
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Kind::Jpeg);
        assert_eq!(sniff(b"GIF89a"), Kind::Gif);
        assert_eq!(sniff(b"PK\x03\x04abc"), Kind::Zip);
        assert_eq!(sniff(b"  <!DOCTYPE HTML><html>"), Kind::Html);
        assert_eq!(sniff(b"<svg xmlns"), Kind::Html);
        assert_eq!(sniff(b"zwykly tekst"), Kind::Other);
        assert_eq!(sniff(b""), Kind::Other);
    }

    #[test]
    fn disguised_executables_are_flagged() {
        assert!(mismatch("faktura.pdf", b"MZ\x90\x00\x03").is_some());
        assert!(mismatch("zdjecie.jpg", b"\x7fELF\x02\x01").is_some());
        assert!(mismatch("umowa.docx", b"\xCA\xFE\xBA\xBE").is_some());
        assert!(mismatch("raport.pdf", b"#!/bin/bash\nrm -rf").is_some());
        assert!(mismatch("obrazek.png", b"<html><script>x</script>").is_some());
    }

    #[test]
    fn legitimate_files_pass() {
        assert!(mismatch("setup.exe", b"MZ\x90\x00").is_none());
        assert!(mismatch("skrypt.sh", b"#!/bin/sh\n").is_none());
        assert!(mismatch("notatki.txt", b"#!/bin/sh\n").is_none());
        assert!(mismatch("faktura.pdf", b"%PDF-1.4").is_none());
        assert!(mismatch("plik.docx", b"PK\x03\x04").is_none());
        assert!(mismatch("strona.html", b"<html>").is_none());
        assert!(mismatch("bezrozszerzenia", b"MZ").is_none());
        assert!(mismatch("dane.bin", b"\x7fELF").is_none());
    }

    #[test]
    fn renamed_image_is_a_mild_mismatch() {
        let m = mismatch("zrzut.jpg", b"\x89PNG\r\n").unwrap();
        assert!(m.contains("nie zgadza"));
        assert!(mismatch("a.pdf", b"PK\x03\x04").is_some());
        assert!(mismatch("a.jpg", b"losowe bajty").is_none());
    }

    #[test]
    fn extension_parsing() {
        assert_eq!(ext_of("A.PDF"), "pdf");
        assert_eq!(ext_of("a.tar.gz"), "gz");
        assert_eq!(ext_of("noext"), "");
        assert_eq!(ext_of("trailing."), "");
    }
}

//! Powiadomienia pulpitowe o nowej poczcie. Logika (co jest nowe, jaki tekst) jest czysta i testowana;
//! samo wyświetlenie robi systemowe `notify-send` (libnotify) uruchamiane bez powłoki.

use std::collections::HashSet;

/// Co już widzieliśmy. `None` = pierwsze sprawdzenie (zapisujemy stan, ale niczego nie ogłaszamy).
#[derive(Default)]
pub struct Seen {
    pub ids: Option<HashSet<String>>,
    /// (nieprzeczytane, wszystkie) w skrzynce przy ostatnim pełnym sprawdzeniu.
    pub counts: Option<(u32, u32)>,
}

/// Stan współdzielony między wywołaniami (zarządzany przez Tauri).
#[derive(Default)]
pub struct Notifier(pub std::sync::Mutex<Seen>);

/// Zwraca identyfikatory wątków, których wcześniej nie było, i zapamiętuje bieżący stan.
pub fn diff_new(seen: &mut Option<HashSet<String>>, current: &[String]) -> Vec<String> {
    let now: HashSet<String> = current.iter().cloned().collect();
    let new = match seen.as_ref() {
        None => Vec::new(),
        Some(old) => current.iter().filter(|id| !old.contains(*id)).cloned().collect(),
    };
    *seen = Some(now);
    new
}

/// Tekst bez znaków sterujących i znaczników, przycięty do `max` znaków.
pub fn clean_text(s: &str, max: usize) -> String {
    let mut t: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > max {
        let cut: String = t.chars().take(max.saturating_sub(1)).collect();
        return format!("{cut}…");
    }
    t
}

pub struct Item<'a> {
    pub sender: &'a str,
    pub subject: &'a str,
    /// Wiadomość z ostrzeżeniem – jej treści nigdy nie pokazujemy w powiadomieniu.
    pub risky: bool,
}

/// Tytuł i treść powiadomienia. Bez `show_content` pokazuje tylko liczbę nowych wiadomości.
pub fn build_text(items: &[Item<'_>], show_content: bool) -> Option<(String, String)> {
    if items.is_empty() {
        return None;
    }
    let n = items.len();
    let title = if n == 1 { "Nowa wiadomość".to_string() } else { format!("Nowe wiadomości: {n}") };
    if !show_content {
        return Some((title, "Otwórz RustMail, aby je przeczytać.".to_string()));
    }
    let lines: Vec<String> = items
        .iter()
        .take(3)
        .map(|i| {
            if i.risky {
                "Wiadomość z ostrzeżeniem – otwórz RustMail".to_string()
            } else {
                format!("{}: {}", clean_text(i.sender, 40), clean_text(i.subject, 70))
            }
        })
        .collect();
    let mut body = lines.join("\n");
    if n > 3 {
        body.push_str(&format!("\n… i {} więcej", n - 3));
    }
    Some((title, body))
}

/// Pokazuje powiadomienie przez `notify-send`. Brak programu lub błąd nie przerywają pracy aplikacji.
pub fn send(title: &str, body: &str) -> Result<(), String> {
    let mut cmd = std::process::Command::new("notify-send");
    // `--` kończy opcje, więc tekst z wiadomości nigdy nie zostanie wzięty za opcję.
    cmd.arg("--app-name=RustMail").arg("--").arg(clean_text(title, 80)).arg(clean_text(body, 400));
    for k in ["LD_LIBRARY_PATH", "LD_PRELOAD", "APPDIR", "GDK_PIXBUF_MODULE_FILE"] {
        cmd.env_remove(k);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    match cmd.status() {
        Ok(st) if st.success() => Ok(()),
        Ok(_) => Err("notify-send zakończył się błędem.".to_string()),
        Err(e) => Err(format!("Nie można pokazać powiadomienia (notify-send): {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn first_check_only_seeds() {
        let mut seen = None;
        assert!(diff_new(&mut seen, &v(&["a", "b"])).is_empty());
        assert_eq!(seen.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn later_checks_report_only_new() {
        let mut seen = None;
        diff_new(&mut seen, &v(&["a", "b"]));
        assert_eq!(diff_new(&mut seen, &v(&["c", "a", "b"])), v(&["c"]));
        assert!(diff_new(&mut seen, &v(&["c", "a"])).is_empty());
        assert_eq!(diff_new(&mut seen, &v(&["b", "c"])), v(&["b"]));
    }

    #[test]
    fn text_is_cleaned_and_cut() {
        assert_eq!(clean_text("a\nb\t c\u{7}", 20), "a b c");
        let t = clean_text(&"x".repeat(100), 10);
        assert_eq!(t.chars().count(), 10);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn notification_text_respects_privacy() {
        assert!(build_text(&[], true).is_none());
        let items = [Item { sender: "Ala", subject: "Faktura", risky: false }];
        let (t, b) = build_text(&items, false).unwrap();
        assert_eq!(t, "Nowa wiadomość");
        assert!(!b.contains("Ala") && !b.contains("Faktura"));
        let (_, b) = build_text(&items, true).unwrap();
        assert_eq!(b, "Ala: Faktura");
        let risky = [Item { sender: "Bank", subject: "Pilne!", risky: true }];
        let (_, b) = build_text(&risky, true).unwrap();
        assert!(!b.contains("Bank") && !b.contains("Pilne"));
        let many: Vec<Item> = (0..5).map(|_| Item { sender: "A", subject: "B", risky: false }).collect();
        let (t, b) = build_text(&many, true).unwrap();
        assert_eq!(t, "Nowe wiadomości: 5");
        assert!(b.ends_with("… i 2 więcej"));
    }
}

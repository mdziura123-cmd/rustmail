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

/// Co już widzieliśmy w skrzynkach IMAP (osobno dla każdego konta; `None` = pierwsze sprawdzenie).
#[derive(Default)]
pub struct ImapSeen(pub std::sync::Mutex<std::collections::HashMap<String, Option<HashSet<String>>>>);

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
    if !show_content {
        let title = if n == 1 { "Nowa wiadomość".to_string() } else { format!("Nowe wiadomości: {n}") };
        return Some((title, "Otwórz RustMail, aby je przeczytać.".to_string()));
    }
    // Jedna wiadomość: nadawca jako tytuł (pogrubiony w dymku), temat pod nim – do dwóch linii.
    if n == 1 {
        let i = &items[0];
        if i.risky {
            return Some(("Nowa wiadomość".to_string(), "Wiadomość z ostrzeżeniem – otwórz RustMail".to_string()));
        }
        let sender = clean_text(i.sender, 60);
        let subject = clean_text(i.subject, 140);
        return Some((
            if sender.is_empty() { "Nowa wiadomość".to_string() } else { sender },
            if subject.is_empty() { "(bez tematu)".to_string() } else { subject },
        ));
    }
    let lines: Vec<String> = items
        .iter()
        .take(3)
        .map(|i| {
            if i.risky {
                "Wiadomość z ostrzeżeniem – otwórz RustMail".to_string()
            } else {
                format!("{} — {}", clean_text(i.sender, 30), clean_text(i.subject, 80))
            }
        })
        .collect();
    let mut body = lines.join("\n");
    if n > 3 {
        body.push_str(&format!("\n… i {} więcej", n - 3));
    }
    Some((format!("Nowe wiadomości: {n}"), body))
}

fn command(title: &str, body: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new("notify-send");
    cmd.arg("--app-name=RustMail")
        .arg("--icon=pl.rustmail.app")
        .arg("--hint=string:desktop-entry:pl.rustmail.app")
        .arg("--expire-time=8000");
    cmd.env_remove("LD_LIBRARY_PATH");
    for k in ["LD_PRELOAD", "APPDIR", "GDK_PIXBUF_MODULE_FILE"] {
        cmd.env_remove(k);
    }
    // `--` kończy opcje, więc tekst z wiadomości nigdy nie zostanie wzięty za opcję (opcje dodane później
    // muszą trafić przed `--`, dlatego `finish` dokłada go na końcu).
    let _ = (title, body);
    cmd
}

fn finish(mut cmd: std::process::Command, title: &str, body: &str) -> std::process::Command {
    cmd.arg("--").arg(clean_text(title, 80)).arg(clean_text(body, 400));
    cmd.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    cmd
}

/// Pokazuje powiadomienie przez `notify-send`. Brak programu lub błąd nie przerywają pracy aplikacji.
pub fn send(title: &str, body: &str) -> Result<(), String> {
    let mut cmd = finish(command(title, body), title, body);
    cmd.stdout(std::process::Stdio::null());
    match cmd.status() {
        Ok(st) if st.success() => Ok(()),
        Ok(_) => Err("notify-send zakończył się błędem.".to_string()),
        Err(e) => Err(format!("Nie można pokazać powiadomienia (notify-send): {e}")),
    }
}

/// Powiadomienie, którego kliknięcie pokazuje okno aplikacji i wysyła do interfejsu zdarzenie `open-mail`
/// z opisem wiadomości (`g:identyfikator` dla Gmaila, `i:konto` dla IMAP). Czekanie na kliknięcie odbywa się
/// w osobnym wątku; starsze `notify-send` bez obsługi akcji dostaje zwykłe powiadomienie.
pub fn send_click(app: &tauri::AppHandle, title: &str, body: &str, target: String) {
    use tauri::Emitter;
    let (app, t, b) = (app.clone(), title.to_string(), body.to_string());
    std::thread::spawn(move || {
        let mut cmd = command(&t, &b);
        cmd.arg("--action=default=Otwórz").arg("--wait");
        let mut cmd = finish(cmd, &t, &b);
        cmd.stdout(std::process::Stdio::piped());
        match cmd.output() {
            Ok(out) if out.status.success() => {
                if String::from_utf8_lossy(&out.stdout).trim() == "default" {
                    crate::background::show_main(&app);
                    let _ = app.emit("open-mail", target);
                }
            }
            _ => {
                let _ = send(&t, &b);
            }
        }
    });
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
        let (t, b) = build_text(&items, true).unwrap();
        assert_eq!((t.as_str(), b.as_str()), ("Ala", "Faktura"));
        let risky = [Item { sender: "Bank", subject: "Pilne!", risky: true }];
        let (t, b) = build_text(&risky, true).unwrap();
        assert!(!t.contains("Bank") && !b.contains("Bank") && !b.contains("Pilne"));
        let empty = [Item { sender: "", subject: "", risky: false }];
        let (t, b) = build_text(&empty, true).unwrap();
        assert_eq!((t.as_str(), b.as_str()), ("Nowa wiadomość", "(bez tematu)"));
        let many: Vec<Item> = (0..5).map(|_| Item { sender: "A", subject: "B", risky: false }).collect();
        let (t, b) = build_text(&many, true).unwrap();
        assert_eq!(t, "Nowe wiadomości: 5");
        assert!(b.ends_with("… i 2 więcej"));
        assert!(b.starts_with("A — B\n"));
    }
}

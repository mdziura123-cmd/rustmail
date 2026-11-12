//! Działanie w tle: ikona w zasobniku systemowym, jedna kopia aplikacji, ukrywanie okna przy zamknięciu
//! oraz skrót w menu aplikacji (stały, żeby przypięcie do paska przetrwało aktualizacje).

use std::{
    io::{Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    time::Duration,
};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use crate::{badge, launcher};

/// Ikona w zasobniku powstała (bez niej nigdy nie chowamy okna – inaczej nie dałoby się go przywrócić).
static TRAY_OK: AtomicBool = AtomicBool::new(false);
/// Ustawienie użytkownika: zamknięcie okna zostawia aplikację w tle.
static KEEP_BG: AtomicBool = AtomicBool::new(true);
/// Ustawienie użytkownika: zamknięcie okna je minimalizuje do paska zadań (zamiast chować do zasobnika).
static MINIMIZE: AtomicBool = AtomicBool::new(false);

/// Ostatnio ustawiona liczba nieprzeczytanych (żeby nie odświeżać ikon bez zmiany).
static LAST_UNREAD: AtomicU32 = AtomicU32::new(u32::MAX);

/// Ustawia znacznik nowych wiadomości: kropkę na ikonie zasobnika, podpowiedź i licznik na ikonie w pasku.
#[tauri::command]
pub fn set_unread(app: AppHandle, count: u32) {
    if LAST_UNREAD.swap(count, Ordering::SeqCst) == count {
        return;
    }
    if let Some(tray) = app.tray_by_id("rustmail") {
        let _ = tray.set_tooltip(Some(badge::tooltip(count)));
        if let Some(base) = app.default_window_icon() {
            let (w, h) = (base.width(), base.height());
            let mut rgba = base.rgba().to_vec();
            if count > 0 {
                badge::draw_dot(&mut rgba, w as usize, h as usize);
            }
            let _ = tray.set_icon(Some(tauri::image::Image::new_owned(rgba, w, h)));
        }
    }
    // Licznik na ikonie aplikacji w pasku (KDE Plasma i inne środowiska z interfejsem Unity Launcher).
    // Bez programu gdbus po prostu nic się nie dzieje.
    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new("gdbus");
        cmd.args(badge::unity_args(launcher::DESKTOP_ID, count));
        for k in ["LD_LIBRARY_PATH", "LD_PRELOAD", "APPDIR", "GDK_PIXBUF_MODULE_FILE", "GIO_EXTRA_MODULES", "GSETTINGS_SCHEMA_DIR"] {
            cmd.env_remove(k);
        }
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        let _ = cmd.status();
    });
}

/// Co zrobić z oknem po kliknięciu „zamknij”: schować do zasobnika, zminimalizować do paska zadań albo zamknąć program.
#[derive(Debug, PartialEq, Eq)]
pub enum CloseAction {
    Hide,
    Minimize,
    Quit,
}

pub fn close_action() -> CloseAction {
    if !KEEP_BG.load(Ordering::SeqCst) {
        CloseAction::Quit
    } else if MINIMIZE.load(Ordering::SeqCst) {
        CloseAction::Minimize
    } else if TRAY_OK.load(Ordering::SeqCst) {
        CloseAction::Hide
    } else {
        CloseAction::Quit
    }
}

/// Tryb zamykania okna: `true` = minimalizuj do paska zadań (jedno kliknięcie ikony w pasku przywraca okno).
#[tauri::command]
pub fn set_close_minimize(minimize: bool) {
    MINIMIZE.store(minimize, Ordering::SeqCst);
}

#[tauri::command]
pub fn set_background(enabled: bool) {
    KEEP_BG.store(enabled, Ordering::SeqCst);
}

/// Czy ikona w zasobniku działa (interfejs pokazuje wtedy ustawienie działania w tle).
#[tauri::command]
pub fn background_state() -> bool {
    TRAY_OK.load(Ordering::SeqCst)
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

// ---------- ikona w zasobniku ----------

fn build_tray(app: &AppHandle) -> Result<(), String> {
    let show = MenuItem::with_id(app, "show", "Pokaż RustMail", true, None::<&str>).map_err(|e| e.to_string())?;
    let quit = MenuItem::with_id(app, "quit", "Zakończ RustMail", true, None::<&str>).map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app, &[&show, &quit]).map_err(|e| e.to_string())?;
    let icon = app.default_window_icon().cloned().ok_or("Brak ikony aplikacji.")?;
    TrayIconBuilder::with_id("rustmail")
        .icon(icon)
        .tooltip("RustMail")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Tworzy ikonę w zasobniku. Biblioteka zasobnika może zapaść się (panic), gdy w systemie brakuje
/// obsługi – wtedy aplikacja działa dalej bez zasobnika, a zamknięcie okna zamyka program jak dotąd.
pub fn init_tray(app: &AppHandle) {
    let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build_tray(app))).map(|r| r.is_ok()).unwrap_or(false);
    TRAY_OK.store(ok, Ordering::SeqCst);
}

// ---------- jedna kopia aplikacji ----------

fn socket_path() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
    if !dir.is_absolute() || !dir.is_dir() {
        return None;
    }
    Some(dir.join(format!("{}.sock", launcher::DESKTOP_ID)))
}

/// Zwraca gniazdo nasłuchujące, jeśli to pierwsza kopia. Jeśli już działa inna kopia, prosi ją o pokazanie
/// okna i kończy ten proces. Po restarcie po aktualizacji (`RUSTMAIL_RESTART`) przejmuje gniazdo bez pytania.
pub fn single_instance() -> Option<UnixListener> {
    let path = socket_path()?;
    let restarting = std::env::var_os("RUSTMAIL_RESTART").is_some();
    std::env::remove_var("RUSTMAIL_RESTART");
    if !restarting {
        if let Ok(mut s) = UnixStream::connect(&path) {
            let _ = s.set_read_timeout(Some(Duration::from_millis(1500)));
            let _ = s.write_all(b"show\n");
            let mut buf = [0u8; 8];
            // Kończymy tylko wtedy, gdy działająca kopia potwierdziła; zawieszona kopia nie blokuje startu.
            if s.read(&mut buf).map(|n| buf[..n].starts_with(b"ok")).unwrap_or(false) {
                std::process::exit(0);
            }
        }
    }
    let _ = std::fs::remove_file(&path);
    let l = UnixListener::bind(&path).ok()?;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    Some(l)
}

/// Obsługuje prośby innych uruchomień: pokazuje okno i potwierdza.
pub fn serve(app: AppHandle, listener: UnixListener) {
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut s) = conn else { continue };
            let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
            let mut buf = [0u8; 16];
            let n = s.read(&mut buf).unwrap_or(0);
            if buf[..n].starts_with(b"show") {
                show_main(&app);
                let _ = s.write_all(b"ok\n");
            }
        }
    });
}

// ---------- skrót w menu aplikacji ----------

fn write_if_changed(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    if std::fs::read(path).map(|old| old == data).unwrap_or(false) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

/// Zapisuje (lub odświeża) skrót i ikonę w katalogu danych użytkownika. Działa tylko z AppImage.
/// Błędy są ignorowane – to udogodnienie, nie warunek działania aplikacji.
pub fn install_launcher(app: &AppHandle) {
    let Some(appimage) = std::env::var_os("APPIMAGE").and_then(|p| p.into_string().ok()) else { return };
    let Some(entry) = launcher::desktop_entry(&appimage) else { return };
    let Ok(data) = app.path().data_dir() else { return };
    let id = launcher::DESKTOP_ID;
    let _ = write_if_changed(&data.join("applications").join(format!("{id}.desktop")), entry.as_bytes());
    let _ = write_if_changed(&data.join("icons/hicolor/128x128/apps").join(format!("{id}.png")), include_bytes!("../icons/128x128.png"));
    let _ = write_if_changed(&data.join("icons/hicolor/256x256/apps").join(format!("{id}.png")), include_bytes!("../icons/128x128@2x.png"));
}

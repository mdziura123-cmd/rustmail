//! Własna ikona w zasobniku (protokół StatusNotifierItem przez D-Bus). Biblioteka, której używa Tauri,
//! w Linuksie nie przekazuje programowi zwykłego kliknięcia (pokazuje tylko menu), a tu lewy przycisk
//! myszy od razu otwiera okno. Jeśli rejestracja się nie uda (brak zasobnika), program używa zwykłej ikony Tauri.

use std::{
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use dbus::{
    arg::{Arg, IterAppend, Variant},
    blocking::SyncConnection,
    channel::{MatchingReceiver, Sender},
    message::{MatchRule, Message, MessageType},
    strings::{BusName, ErrorName},
    Path,
};
use tauri::AppHandle;

const ITEM_PATH: &str = "/StatusNotifierItem";
const ITEM_IFACE: &str = "org.kde.StatusNotifierItem";
const PROPS_IFACE: &str = "org.freedesktop.DBus.Properties";

type Pixmap = Vec<(i32, i32, Vec<u8>)>;

struct Shared {
    conn: Arc<SyncConnection>,
    icon: Mutex<Pixmap>,
    tooltip: Mutex<String>,
}

static SHARED: OnceLock<Shared> = OnceLock::new();

enum Prop {
    S(String),
    B(bool),
    U(u32),
    O(&'static str),
    Px(Pixmap),
    Tip(String, Pixmap, String, String),
}

fn props(sh: &Shared) -> Vec<(&'static str, Prop)> {
    let icon = sh.icon.lock().map(|g| g.clone()).unwrap_or_default();
    let tip = sh.tooltip.lock().map(|g| g.clone()).unwrap_or_default();
    vec![
        ("Category", Prop::S("ApplicationStatus".to_string())),
        ("Id", Prop::S("rustmail".to_string())),
        ("Title", Prop::S("RustMail".to_string())),
        ("Status", Prop::S("Active".to_string())),
        ("WindowId", Prop::U(0)),
        ("IconName", Prop::S(String::new())),
        ("IconPixmap", Prop::Px(icon)),
        ("OverlayIconName", Prop::S(String::new())),
        ("AttentionIconName", Prop::S(String::new())),
        ("ToolTip", Prop::Tip(String::new(), Vec::new(), "RustMail".to_string(), tip)),
        ("ItemIsMenu", Prop::B(false)),
        ("Menu", Prop::O("/NO_DMENU")),
    ]
}

/// Dopisuje wartość właściwości jako wariant (`v`).
fn append_prop(ia: &mut IterAppend, p: &Prop) {
    match p {
        Prop::S(s) => {
            let s = s.clone();
            ia.append_variant(&<String as Arg>::signature(), move |v| v.append(s));
        }
        Prop::B(b) => {
            let b = *b;
            ia.append_variant(&<bool as Arg>::signature(), move |v| v.append(b));
        }
        Prop::U(n) => {
            let n = *n;
            ia.append_variant(&<u32 as Arg>::signature(), move |v| v.append(n));
        }
        Prop::O(path) => {
            if let Ok(path) = Path::new(*path) {
                ia.append_variant(&<Path as Arg>::signature(), move |v| v.append(path));
            }
        }
        Prop::Px(px) => {
            let px = px.clone();
            ia.append_variant(&<Pixmap as Arg>::signature(), move |v| v.append(px));
        }
        Prop::Tip(a, px, b, c) => {
            let t = (a.clone(), px.clone(), b.clone(), c.clone());
            ia.append_variant(&<(String, Pixmap, String, String) as Arg>::signature(), move |v| v.append(t));
        }
    }
}

fn error_reply(msg: &Message, name: &str, text: &str) -> Option<Message> {
    let n = ErrorName::new(name).ok()?;
    let c = std::ffi::CString::new(text).ok()?;
    Some(msg.error(&n, &c))
}

fn handle(app: &AppHandle, conn: &SyncConnection, msg: Message) {
    if msg.msg_type() != MessageType::MethodCall {
        return;
    }
    let Some(sh) = SHARED.get() else { return };
    let (Some(path), Some(iface), Some(member)) = (msg.path(), msg.interface(), msg.member()) else {
        if let Some(e) = error_reply(&msg, "org.freedesktop.DBus.Error.UnknownMethod", "Nieznana metoda") {
            let _ = conn.send(e);
        }
        return;
    };
    let (path, iface, member) = (path.to_string(), iface.to_string(), member.to_string());
    let reply: Option<Message> = if path != ITEM_PATH {
        error_reply(&msg, "org.freedesktop.DBus.Error.UnknownObject", "Nieznany obiekt")
    } else if iface == ITEM_IFACE {
        match member.as_str() {
            // lewy przycisk, środkowy przycisk i (bez własnego menu) prawy przycisk: otwórz okno
            "Activate" | "SecondaryActivate" | "ContextMenu" => {
                crate::background::show_main(app);
                Some(msg.method_return())
            }
            "Scroll" => Some(msg.method_return()),
            _ => error_reply(&msg, "org.freedesktop.DBus.Error.UnknownMethod", "Nieznana metoda"),
        }
    } else if iface == PROPS_IFACE {
        match member.as_str() {
            "GetAll" => {
                let mut r = msg.method_return();
                {
                    let mut ia = IterAppend::new(&mut r);
                    ia.append_dict(&<String as Arg>::signature(), &<Variant<u8> as Arg>::signature(), |d| {
                        for (k, p) in props(sh) {
                            d.append_dict_entry(|e| {
                                e.append(k);
                                append_prop(e, &p);
                            });
                        }
                    });
                }
                Some(r)
            }
            "Get" => {
                let (_, name) = msg.get2::<&str, &str>();
                match name.and_then(|n| props(sh).into_iter().find(|(k, _)| *k == n)) {
                    Some((_, p)) => {
                        let mut r = msg.method_return();
                        {
                            let mut ia = IterAppend::new(&mut r);
                            append_prop(&mut ia, &p);
                        }
                        Some(r)
                    }
                    None => error_reply(&msg, "org.freedesktop.DBus.Error.InvalidArgs", "Nieznana właściwość"),
                }
            }
            _ => error_reply(&msg, "org.freedesktop.DBus.Error.AccessDenied", "Tylko do odczytu"),
        }
    } else {
        error_reply(&msg, "org.freedesktop.DBus.Error.UnknownInterface", "Nieznany interfejs")
    };
    if let Some(r) = reply {
        let _ = conn.send(r);
    }
}

fn emit(name: &str) {
    if let Some(sh) = SHARED.get() {
        if let Ok(sig) = Message::new_signal(ITEM_PATH, ITEM_IFACE, name) {
            let _ = sh.conn.send(sig);
        }
    }
}

/// Aktualizuje ikonę (RGBA) i podpowiedź; powiadamia zasobnik sygnałami.
pub fn update(rgba: &[u8], w: u32, h: u32, tooltip: &str) {
    let Some(sh) = SHARED.get() else { return };
    let px = crate::badge::argb_pixmap(rgba, w, h);
    if !px.is_empty() {
        if let Ok(mut g) = sh.icon.lock() {
            *g = px;
        }
    }
    if let Ok(mut g) = sh.tooltip.lock() {
        *g = tooltip.to_string();
    }
    emit("NewIcon");
    emit("NewToolTip");
}

/// Rejestruje ikonę w zasobniku. Zwraca `false`, gdy się nie udało (wtedy działa zwykła ikona Tauri).
pub fn start(app: &AppHandle, rgba: &[u8], w: u32, h: u32) -> bool {
    let px = crate::badge::argb_pixmap(rgba, w, h);
    if px.is_empty() || SHARED.get().is_some() {
        return false;
    }
    let Ok(conn) = SyncConnection::new_session() else { return false };
    let conn = Arc::new(conn);
    let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
    let Ok(bus_name) = BusName::new(name.clone()) else { return false };
    if conn.request_name(bus_name, false, true, false).is_err() {
        return false;
    }
    let app2 = app.clone();
    conn.start_receive(
        MatchRule::new_method_call(),
        Box::new(move |msg, c| {
            handle(&app2, c, msg);
            true
        }),
    );
    let shared = Shared { conn: conn.clone(), icon: Mutex::new(px), tooltip: Mutex::new("RustMail".to_string()) };
    if SHARED.set(shared).is_err() {
        return false;
    }
    // Rejestracja w obserwatorze zasobnika (dopóki trwa, nie przetwarzamy innych wiadomości – kolejkują się).
    let registered = conn
        .with_proxy("org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher", Duration::from_secs(3))
        .method_call::<(), _, _, _>("org.kde.StatusNotifierWatcher", "RegisterStatusNotifierItem", (name.as_str(),))
        .is_ok();
    if !registered {
        return false;
    }
    std::thread::spawn(move || {
        let mut errors = 0u32;
        loop {
            match conn.process(Duration::from_millis(500)) {
                Ok(_) => errors = 0,
                Err(_) => {
                    errors += 1;
                    if errors > 30 {
                        break;
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
    });
    true
}

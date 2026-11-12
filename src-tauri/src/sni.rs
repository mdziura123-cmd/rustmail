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
    strings::{BusName, ErrorName, Signature},
    Path,
};
use tauri::AppHandle;

const ITEM_PATH: &str = "/StatusNotifierItem";
const ITEM_IFACE: &str = "org.kde.StatusNotifierItem";
const PROPS_IFACE: &str = "org.freedesktop.DBus.Properties";
const MENU_PATH: &str = "/MenuBar";
const MENU_IFACE: &str = "com.canonical.dbusmenu";

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
    Strs(Vec<String>),
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
        ("Menu", Prop::O(MENU_PATH)),
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
        Prop::Strs(list) => {
            let list = list.clone();
            ia.append_variant(&<Vec<String> as Arg>::signature(), move |v| v.append(list));
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

fn menu_props() -> Vec<(&'static str, Prop)> {
    vec![
        ("Version", Prop::U(3)),
        ("TextDirection", Prop::S("ltr".to_string())),
        ("Status", Prop::S("normal".to_string())),
        ("IconThemePath", Prop::Strs(Vec::new())),
    ]
}

enum Mv {
    S(&'static str),
    B(bool),
}

const MENU_IDS: [i32; 3] = [1, 2, 3];

/// Pozycje menu zasobnika: 1 = pokaż okno, 2 = separator, 3 = zakończ program.
fn item_props(id: i32) -> Vec<(&'static str, Mv)> {
    match id {
        0 => vec![("children-display", Mv::S("submenu"))],
        1 => vec![("label", Mv::S("Pokaż RustMail")), ("enabled", Mv::B(true)), ("visible", Mv::B(true))],
        2 => vec![("type", Mv::S("separator")), ("visible", Mv::B(true))],
        _ => vec![("label", Mv::S("Zakończ całkowicie")), ("enabled", Mv::B(true)), ("visible", Mv::B(true))],
    }
}

fn append_mv(ia: &mut IterAppend, v: &Mv) {
    match v {
        Mv::S(s) => {
            let s = s.to_string();
            ia.append_variant(&<String as Arg>::signature(), move |x| x.append(s));
        }
        Mv::B(b) => {
            let b = *b;
            ia.append_variant(&<bool as Arg>::signature(), move |x| x.append(b));
        }
    }
}

fn append_item_props(ia: &mut IterAppend, id: i32) {
    ia.append_dict(&<String as Arg>::signature(), &<Variant<u8> as Arg>::signature(), |d| {
        for (k, v) in item_props(id) {
            d.append_dict_entry(|e| {
                e.append(k);
                append_mv(e, &v);
            });
        }
    });
}

/// Pozycja menu jako struktura (id, właściwości, dzieci).
fn append_item(ia: &mut IterAppend, id: i32, with_children: bool) {
    ia.append_struct(|s| {
        s.append(id);
        append_item_props(s, id);
        s.append_array(&<Variant<u8> as Arg>::signature(), |a| {
            if with_children {
                if let Ok(sig) = Signature::new("(ia{sv}av)") {
                    for cid in MENU_IDS {
                        a.append_variant(&sig, |v| append_item(v, cid, false));
                    }
                }
            }
        });
    });
}

fn menu_call(app: &AppHandle, msg: &Message, member: &str) -> Option<Message> {
    match member {
        "GetLayout" => {
            let (pid, _) = msg.get2::<i32, i32>();
            let pid = pid.filter(|p| MENU_IDS.contains(p)).unwrap_or(0);
            let mut r = msg.method_return();
            {
                let mut ia = IterAppend::new(&mut r);
                ia.append(1u32);
                append_item(&mut ia, pid, pid == 0);
            }
            Some(r)
        }
        "GetGroupProperties" => {
            let mut ids: Vec<i32> = msg.get1::<Vec<i32>>().unwrap_or_default().into_iter().filter(|i| *i == 0 || MENU_IDS.contains(i)).collect();
            if ids.is_empty() {
                ids = vec![0, 1, 2, 3];
            }
            let sig = Signature::new("(ia{sv})").ok()?;
            let mut r = msg.method_return();
            {
                let mut ia = IterAppend::new(&mut r);
                ia.append_array(&sig, |a| {
                    for id in ids {
                        a.append_struct(|s| {
                            s.append(id);
                            append_item_props(s, id);
                        });
                    }
                });
            }
            Some(r)
        }
        "Event" => {
            let (id, ev) = msg.get2::<i32, &str>();
            if ev == Some("clicked") {
                match id {
                    Some(1) => crate::background::show_main(app),
                    Some(3) => app.exit(0),
                    _ => {}
                }
            }
            Some(msg.method_return())
        }
        "AboutToShow" => Some(msg.method_return().append1(false)),
        "AboutToShowGroup" => Some(msg.method_return().append2(Vec::<i32>::new(), Vec::<i32>::new())),
        "EventGroup" => Some(msg.method_return().append1(Vec::<i32>::new())),
        _ => error_reply(msg, "org.freedesktop.DBus.Error.UnknownMethod", "Nieznana metoda"),
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
    let reply: Option<Message> = if path != ITEM_PATH && path != MENU_PATH {
        error_reply(&msg, "org.freedesktop.DBus.Error.UnknownObject", "Nieznany obiekt")
    } else if iface == PROPS_IFACE {
        let which = msg.get1::<&str>().unwrap_or("");
        let list = if path == ITEM_PATH && which == ITEM_IFACE {
            props(sh)
        } else if path == MENU_PATH && which == MENU_IFACE {
            menu_props()
        } else {
            Vec::new()
        };
        match member.as_str() {
            "GetAll" => {
                let mut r = msg.method_return();
                {
                    let mut ia = IterAppend::new(&mut r);
                    ia.append_dict(&<String as Arg>::signature(), &<Variant<u8> as Arg>::signature(), |d| {
                        for (k, p) in &list {
                            d.append_dict_entry(|e| {
                                e.append(*k);
                                append_prop(e, p);
                            });
                        }
                    });
                }
                Some(r)
            }
            "Get" => {
                let (_, name) = msg.get2::<&str, &str>();
                match name.and_then(|n| list.iter().find(|(k, _)| *k == n)) {
                    Some((_, p)) => {
                        let mut r = msg.method_return();
                        {
                            let mut ia = IterAppend::new(&mut r);
                            append_prop(&mut ia, p);
                        }
                        Some(r)
                    }
                    None => error_reply(&msg, "org.freedesktop.DBus.Error.InvalidArgs", "Nieznana właściwość"),
                }
            }
            _ => error_reply(&msg, "org.freedesktop.DBus.Error.AccessDenied", "Tylko do odczytu"),
        }
    } else if path == ITEM_PATH && iface == ITEM_IFACE {
        match member.as_str() {
            // lewy i środkowy przycisk: otwórz okno (prawy pokazuje menu z obiektu /MenuBar)
            "Activate" | "SecondaryActivate" | "ContextMenu" => {
                crate::background::show_main(app);
                Some(msg.method_return())
            }
            "Scroll" => Some(msg.method_return()),
            _ => error_reply(&msg, "org.freedesktop.DBus.Error.UnknownMethod", "Nieznana metoda"),
        }
    } else if path == MENU_PATH && iface == MENU_IFACE {
        menu_call(app, &msg, &member)
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

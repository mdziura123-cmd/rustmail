//! Kalendarz w osobnym oknie (żeby podglądać go obok poczty). Okno ma etykietę „calendar”; interfejs rozpoznaje
//! ją i pokazuje tylko kalendarz. Dane wstępne (np. z wiadomości) czekają w pamięci i są odbierane przez okno.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "calendar";
const MAX_PREFILL: usize = 4000;

#[derive(Default)]
pub struct CalPrefill(Mutex<Option<String>>);

fn clean(p: Option<String>) -> Result<Option<String>, String> {
    match p {
        Some(s) if s.len() > MAX_PREFILL => Err("Dane dla kalendarza są zbyt duże.".to_string()),
        Some(s) if s.chars().any(|c| c.is_control() && c != '\n') => Err("Niepoprawne dane dla kalendarza.".to_string()),
        Some(s) if s.trim().is_empty() => Ok(None),
        other => Ok(other),
    }
}

/// Otwiera okno kalendarza (lub wysuwa istniejące). `prefill` to JSON z danymi nowego wydarzenia.
#[tauri::command]
pub fn open_calendar_window(app: AppHandle, state: State<'_, CalPrefill>, prefill: Option<String>) -> Result<(), String> {
    let prefill = clean(prefill)?;
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        if let Some(p) = prefill {
            let _ = app.emit_to(LABEL, "cal-prefill", p);
        }
        return Ok(());
    }
    if let Ok(mut g) = state.0.lock() {
        *g = prefill;
    }
    WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Kalendarz – RustMail")
        .inner_size(900.0, 780.0)
        .min_inner_size(560.0, 520.0)
        .build()
        .map(|_| ())
        .map_err(|e| format!("Nie można otworzyć okna kalendarza: {e}"))
}

/// Odbiera dane wstępne zapisane przy otwieraniu okna (jednorazowo).
#[tauri::command]
pub fn cal_take_prefill(state: State<'_, CalPrefill>) -> Option<String> {
    state.0.lock().ok().and_then(|mut g| g.take())
}

#[tauri::command]
pub fn close_calendar_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.close();
    }
}

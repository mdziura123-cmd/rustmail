mod auth;
mod commands;
mod gmail;
mod secrets;
mod update;

pub fn run() {
    // Obejście znanych problemów renderowania WebKitGTK (biała/pusta ramka)
    // na części sterowników GPU. Można nadpisać: WEBKIT_DISABLE_DMABUF_RENDERER=0.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    tauri::Builder::default()
        .manage(auth::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::auth_status,
            commands::save_client,
            commands::clear_client,
            commands::sign_in,
            commands::sign_out,
            commands::list_threads,
            commands::get_thread,
            update::check_update,
            update::install_update,
            update::rollback_update,
            update::update_state,
            update::restart_app,
        ])
        .run(tauri::generate_context!())
        .expect("nie udało się uruchomić RustMail");
}

mod auth;
mod blocklist;
mod commands;
mod compose;
mod dismissed;
mod filetype;
mod gmail;
mod labels;
mod notify;
mod offline;
mod phishing;
mod remote;
mod safety;
mod scan;
mod secrets;
mod signature;
mod store;
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
        .manage(notify::Notifier::default())
        .setup(|app| {
            use tauri::Manager;
            // Baza lokalna: jeśli nie da się jej otworzyć, aplikacja działa dalej tylko online.
            let store = match app.path().app_data_dir() {
                Ok(dir) => store::Store::open(&dir.join("cache")),
                Err(_) => store::Store::disabled(),
            };
            app.manage(store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::auth_status,
            commands::save_client,
            commands::clear_client,
            commands::sign_in,
            commands::sign_out,
            commands::list_threads,
            commands::get_thread,
            commands::search_threads,
            commands::sync_now,
            commands::queue_message,
            commands::list_outbox,
            commands::retry_outbox,
            commands::delete_outbox,
            commands::flush_outbox,
            commands::poll_mail,
            commands::create_label,
            commands::test_notification,
            commands::set_thread_label,
            commands::cache_status,
            commands::set_offline_enabled,
            commands::clear_cache,
            commands::set_thread_safe,
            commands::get_sender_lists,
            commands::add_sender_rule,
            commands::remove_sender_rule,
            commands::send_message,
            commands::save_draft,
            commands::delete_draft,
            commands::open_draft,
            commands::thread_action,
            commands::fetch_image,
            commands::open_url,
            commands::download_attachment,
            commands::get_signature,
            commands::set_signature,
            update::check_update,
            update::install_update,
            update::rollback_update,
            update::update_state,
            update::restart_app,
        ])
        .run(tauri::generate_context!())
        .expect("nie udało się uruchomić RustMail");
}

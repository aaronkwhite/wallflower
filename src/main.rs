#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config;
mod database;
mod freedium;

use std::sync::Arc;
use tokio::sync::RwLock;

use commands::AppState;
use config::AppConfig;
use database::Database;
use freedium::FreediumClient;

fn main() {
    // Load configuration
    let config = AppConfig::load();

    // Create the Freedium client with configured endpoints
    let client = FreediumClient::new(config.endpoints.clone());

    // Initialize the database
    let database = Database::new().expect("Failed to initialize database");

    // Create shared application state with tokio's async-safe RwLock
    let state = AppState {
        client: Arc::new(RwLock::new(client)),
        config: Arc::new(RwLock::new(config)),
        database: Arc::new(database),
    };

    // Build and run the Tauri application
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::fetch_article,
            commands::get_cached_article,
            commands::get_config,
            commands::save_config,
            commands::check_endpoints,
            commands::validate_url,
            commands::get_history,
            commands::get_favorites,
            commands::toggle_favorite,
            commands::delete_from_history,
            commands::clear_cache,
            commands::save_markdown_file,
            commands::export_database,
            commands::export_as_markdown,
        ])
        .setup(|app| {
            // Enable back/forward swipe gestures on macOS
            #[cfg(target_os = "macos")]
            {
                use tauri::Manager;
                if let Some(webview) = app.get_webview_window("main") {
                    let _ = webview.with_webview(|wv| {
                        use objc2::rc::Retained;
                        use objc2::runtime::Bool;
                        use objc2::msg_send;
                        use objc2_foundation::NSObject;

                        let wk_webview: Retained<NSObject> = unsafe {
                            Retained::retain(wv.inner().cast()).unwrap()
                        };
                        let _: () = unsafe {
                            msg_send![&wk_webview, setAllowsBackForwardNavigationGestures: Bool::YES]
                        };
                    });
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

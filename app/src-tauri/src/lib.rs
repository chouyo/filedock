mod commands;
mod config;
mod file_entry;
mod matcher;
mod scan;
mod settings;
mod tray;
mod util;
mod watch;
mod webview;
mod window_state;

use std::sync::Mutex;
use tauri::{Emitter, Manager};

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
                let _ = app.emit("window-visibility", true);
            }
        }))
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            scan_locks: Mutex::new(std::collections::HashMap::new()),
            config: Mutex::new(config::load_config_file()),
            settings: Mutex::new(settings::load_settings_file()),
            file_watch: tokio::sync::Mutex::new(None),
        })
        .setup(|app| {
            config::init_config_file()?;
            settings::init_settings_file()?;

            let window = app.get_webview_window("main").unwrap();
            let settings_val = settings::load_settings_file();

            webview::disable_browser_accelerator_keys(&window);

            if let Some(ref state) = settings_val.window_state {
                window_state::restore_window_state(&window, state);
            } else {
                let _ = window.center();
            }

            let _ = window.show();

            window_state::install_window_state_listener(app.handle());

            tray::setup_tray(app)?;

            let app_handle = app.handle().clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let settings_val = settings::load_settings_file();
                    match settings_val.close_action.as_str() {
                        "close" => {}
                        "minimize" => {
                            api.prevent_close();
                            if let Some(w) = app_handle.get_webview_window("main") {
                                let _ = w.hide();
                                let _ = app_handle.emit("window-visibility", false);
                            }
                        }
                        _ => {
                            api.prevent_close();
                            let _ = app_handle.emit("close-requested", ());
                        }
                    }
                }
            });

            #[cfg(debug_assertions)]
            {
                if let Some(w) = app.get_webview_window("main") {
                    w.open_devtools();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![

            commands::load_config,
            commands::load_settings,
            commands::save_window_state,
            commands::list_categories,
            commands::save_category,
            commands::delete_category,
            commands::reorder_categories,
            commands::list_files,
            commands::save_session,
            commands::save_settings,
            commands::open_in_explorer,
            commands::open_devtools,
            commands::hide_main_window,
            commands::quit_app,
            commands::get_config_dir,
            commands::is_valid_directory,
            commands::inspect_paths,
            commands::open_config_directory,
            commands::minimize_main_window,
            commands::toggle_maximize_main_window,
            commands::close_main_window,
            watch::start_file_watch,
            watch::stop_file_watch,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // On macOS, clicking the Dock icon while the window is hidden/closed
            // fires Reopen instead of a window event; restore it manually.
            if let tauri::RunEvent::Reopen {
                has_visible_windows,
                ..
            } = event
            {
                if !has_visible_windows {
                    tray::show_main_window(app_handle);
                }
            }
        });
}

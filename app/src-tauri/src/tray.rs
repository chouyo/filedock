use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

use crate::settings;

pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let settings_val = settings::load_settings_file();
    let is_zh = settings_val.language == "zh-CN";

    let open_label = if is_zh { "打开" } else { "Open" };
    let settings_label = if is_zh { "设置" } else { "Settings" };
    let quit_label = if is_zh { "退出" } else { "Exit" };

    let open_i = MenuItem::with_id(app, "tray_open", open_label, true, None::<&str>)?;
    let settings_i = MenuItem::with_id(app, "tray_settings", settings_label, true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit_i = MenuItem::with_id(app, "tray_quit", quit_label, true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open_i, &settings_i, &sep, &quit_i])?;

    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("FileDock")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            handle_tray_menu(app, event.id.as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                show_main_window(app);
            }
        })
        .build(app)?;

    Ok(())
}

fn handle_tray_menu(app: &tauri::AppHandle, id: &str) {
    match id {
        "tray_open" => show_main_window(app),
        "tray_settings" => {
            show_main_window(app);
            let _ = app.emit("open-settings", ());
        }
        "tray_quit" => {
            if let Some(window) = app.get_webview_window("main") {
                if !window.is_visible().unwrap_or(false) {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            let _ = app.emit("request-quit", ());
        }
        _ => {}
    }
}

pub fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if !window.is_visible().unwrap_or(false) {
            let _ = window.show();
        }
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = app.emit("window-visibility", true);
    }
}

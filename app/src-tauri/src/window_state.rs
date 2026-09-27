use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{LogicalPosition, LogicalSize, Manager, WebviewWindow};

use crate::settings::{self, MonitorPos, MonitorSize, WindowState};

pub fn collect_window_state(window: &WebviewWindow) -> Option<WindowState> {
    let monitor = window.current_monitor().ok()??;
    let pos = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    let maximized = window.is_maximized().unwrap_or(false);

    let m_pos = monitor.position();
    let m_size = monitor.size();
    let scale = monitor.scale_factor();

    Some(WindowState {
        monitor_name: monitor.name().map(|s| s.to_string()),
        monitor_position: MonitorPos {
            x: m_pos.x,
            y: m_pos.y,
        },
        monitor_size: MonitorSize {
            width: m_size.width,
            height: m_size.height,
        },
        window_x: (pos.x - m_pos.x) as f64 / scale,
        window_y: (pos.y - m_pos.y) as f64 / scale,
        window_width: size.width as f64 / scale,
        window_height: size.height as f64 / scale,
        maximized,
    })
}

pub fn restore_window_state(window: &WebviewWindow, state: &WindowState) {
    if state.maximized {
        let _ = window.maximize();
        return;
    }

    let monitors = window.available_monitors().unwrap_or_default();

    let target_monitor = monitors.iter().find(|m| {
        if let (Some(name), Some(m_name)) = (&state.monitor_name, m.name()) {
            if name == &m_name.to_string() {
                return true;
            }
        }
        let m_pos = m.position();
        if m_pos.x == state.monitor_position.x && m_pos.y == state.monitor_position.y {
            return true;
        }
        let m_size = m.size();
        if m_size.width == state.monitor_size.width && m_size.height == state.monitor_size.height
        {
            return true;
        }
        false
    });

    if target_monitor.is_some() {
        let _ = window.set_size(LogicalSize::new(
            state.window_width.max(400.0),
            state.window_height.max(300.0),
        ));
        let _ = window.set_position(LogicalPosition::new(state.window_x, state.window_y));
    } else {
        let _ = window.center();
    }
}

pub fn collect_and_save_window_state(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("main window not found".to_string())?;
    if let Some(state) = collect_window_state(&window) {
        let mut settings = settings::load_settings_file();
        settings.window_state = Some(state);
        settings::save_settings_file(&settings)?;
    }
    Ok(())
}

pub fn install_window_state_listener(app: &tauri::AppHandle) {
    let window = match app.get_webview_window("main") {
        Some(w) => w,
        None => return,
    };
    let app_handle = app.clone();
    // Moved/Resized fire on every frame of a live drag. Only the task spawned by
    // the latest event saves, so the window queries (which hop to the main thread
    // on macOS) and the disk write happen once, after the drag has settled.
    let generation = Arc::new(AtomicU64::new(0));

    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) = event {
            let handle = app_handle.clone();
            let generation = generation.clone();
            let current = generation.fetch_add(1, Ordering::Relaxed) + 1;
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if generation.load(Ordering::Relaxed) == current {
                    let _ = collect_and_save_window_state(&handle);
                }
            });
        }
    });
}

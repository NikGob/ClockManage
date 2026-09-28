//! Window management: main window, the always-on-top mini timer, the alarm overlay, tray.

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WindowEvent};

use crate::app::{Shared, TrayItems};

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_main(app);
        }
    }
}

pub fn toggle_mini(app: &AppHandle) {
    let Some(w) = app.get_webview_window("mini") else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        let _ = app.emit("mini", false);
        return;
    }
    // First show: top-right corner of the current monitor.
    if w.outer_position().map(|p| p.x <= 0 && p.y <= 0).unwrap_or(true) {
        if let (Ok(Some(m)), Ok(size)) = (w.current_monitor(), w.outer_size()) {
            let scale = m.scale_factor();
            let margin = (24.0 * scale) as i32;
            let x = m.position().x + m.size().width as i32 - size.width as i32 - margin;
            let y = m.position().y + (48.0 * scale) as i32;
            let _ = w.set_position(PhysicalPosition::new(x, y));
        }
    }
    let _ = w.show();
    let _ = w.set_always_on_top(true);
    let _ = app.emit("mini", true);
}

/// Closing a window only hides it; the app lives in the tray.
pub fn install_close_handlers(app: &AppHandle) {
    for label in ["main", "mini", "overlay"] {
        if let Some(w) = app.get_webview_window(label) {
            let w2 = w.clone();
            let app2 = app.clone();
            w.on_window_event(move |e| {
                if let WindowEvent::CloseRequested { api, .. } = e {
                    api.prevent_close();
                    let _ = w2.hide();
                    if w2.label() == "mini" {
                        let _ = app2.emit("mini", false);
                    }
                }
            });
        }
    }
}

pub fn try_quit(app: &AppHandle, shared: &Arc<Shared>) {
    let snap = shared.snapshot();
    if snap.view.lock.base {
        use tauri_plugin_notification::NotificationExt;
        let _ = app
            .notification()
            .builder()
            .title("Выйти нельзя")
            .body("Идёт учебный день. Блокировка снимется после всех блоков или в конце дня.")
            .show();
        show_main(app);
        return;
    }
    shared.blocker.lock().unwrap_or_else(|e| e.into_inner()).sync(None, true);
    shared.mcp.stop();
    app.exit(0);
}

pub fn build_tray(app: &AppHandle, shared: Arc<Shared>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Открыть", true, None::<&str>)?;
    let mini = MenuItem::with_id(app, "mini", "Мини-таймер", true, None::<&str>)?;
    let action = MenuItem::with_id(app, "action", "Начать день", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&action, &sep1, &open, &mini, &sep2, &quit])?;
    *shared.tray.lock().unwrap() = Some(TrayItems { action, quit });

    let s2 = shared.clone();
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("ClockManage")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "mini" => toggle_mini(app),
            "action" => {
                if let Err(err) = crate::app::primary_action(&s2) {
                    eprintln!("tray action: {err}");
                }
            }
            "quit" => try_quit(app, &s2),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                toggle_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

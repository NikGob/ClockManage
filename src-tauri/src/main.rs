#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod blocker;
mod mcp_server;
mod sound;
mod store;
mod system;
mod windows;

use std::sync::{Arc, Mutex};

use clockmanage_core::clock;
use tauri::{Emitter, Manager};

fn main() {
    let background = std::env::args().any(|a| a == "--background");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // The watchdog task relaunches us every few minutes with --background: ignore it.
            if !args.iter().any(|a| a == "--background") {
                windows::show_main(app);
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .setup(move |tauri_app| {
            let handle = tauri_app.handle().clone();
            let dir = handle.path().app_data_dir().expect("app data dir");
            let store = store::Store::new(dir.clone());
            let mut cfg = store.load_config();
            if cfg.mcp_port == 0 {
                cfg.mcp_port = mcp_server::random_port();
            }
            store.save_config(&cfg);
            let now = clock::now_ts();
            let day = app::load_today(&store, &cfg, now);
            store.save_day(&day);
            let autostart = cfg.autostart;
            let (mcp_enabled, mcp_port) = (cfg.mcp_enabled, cfg.mcp_port);

            let shared = Arc::new(app::Shared {
                inner: Mutex::new(app::Inner {
                    cfg,
                    day,
                    captcha: None,
                    force_emit: true,
                    last_emit_sec: 0,
                    last_save: now,
                    last_kill_note: 0,
                }),
                store,
                blocker: Mutex::new(blocker::Blocker::new(dir.join("blocker.json"))),
                mcp: mcp_server::McpServer::default(),
                app: handle.clone(),
                admin: system::is_admin(),
                overlay: Mutex::new(None),
                tray: Mutex::new(None),
            });
            tauri_app.manage(shared.clone());

            let port = shared.mcp.start(shared.clone(), mcp_enabled, mcp_port);
            if port != mcp_port {
                let mut g = shared.lock();
                g.cfg.mcp_port = port;
                shared.store.save_config(&g.cfg);
            }

            windows::build_tray(&handle, shared.clone())?;
            windows::install_close_handlers(&handle);
            std::thread::spawn(move || {
                sound::warm_up();
                if let Err(e) = system::set_autostart(autostart) {
                    eprintln!("{e}");
                }
            });
            app::spawn_ticker(shared.clone());

            if !background {
                windows::show_main(&handle);
            }
            let _ = handle.emit("state", shared.snapshot());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app::get_state,
            app::get_config,
            app::save_config,
            app::regenerate_port,
            app::start_day,
            app::pause,
            app::resume,
            app::start_next,
            app::primary,
            app::start_lunch,
            app::start_single,
            app::stop_single,
            app::set_plan,
            app::emergency,
            app::end_access,
            app::captcha_new,
            app::captcha_submit,
            app::list_days,
            app::day_stats,
            app::export_log,
            app::open_data_dir,
            app::test_sound,
            app::preview_overlay,
            app::get_overlay,
            app::hide_overlay,
            app::show_main,
            app::toggle_mini,
            app::style_titlebar,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ClockManage")
        .run(|_app, event| {
            // Keep running in the tray when every window is hidden.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

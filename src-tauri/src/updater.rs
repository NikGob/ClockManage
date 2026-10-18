//! Self-update from GitHub Releases, no server of our own: the app reads `latest.json` of the
//! latest release (CI uploads it next to the signed installer), downloads the new installer in
//! the background and runs it when no study day or lock is running. The installer closes the
//! app, installs over it and starts it again: our NSIS hook does that (`installer-hooks.nsh`),
//! because the plugin's own restart runs it without admin rights and so asks for UAC.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use clockmanage_core::clock::{self, Ts};
use clockmanage_core::i18n::tr;
use clockmanage_core::tr;
use serde::Serialize;
use tauri::{Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::app::Shared;

/// First check this long after the start (the app has settled, the network is up).
const FIRST_CHECK: Duration = Duration::from_secs(60);
const CHECK_EVERY_MS: Ts = 6 * 3600 * 1000;
/// An automatic install waits for this long without keyboard or mouse input.
const AWAY_MS: u64 = 5 * 60 * 1000;

#[derive(Serialize, Clone, Default)]
pub struct UpdateStatus {
    /// idle | checking | downloading | ready | latest | error
    pub state: &'static str,
    /// The new version (downloading / ready).
    pub version: Option<String>,
    pub checked_at: Option<Ts>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Updates {
    pub status: Mutex<UpdateStatus>,
    /// The downloaded installer, verified against the public key in tauri.conf.json.
    ready: Mutex<Option<(Update, Vec<u8>)>>,
    busy: Mutex<bool>,
}

impl Updates {
    pub fn status(&self) -> UpdateStatus {
        let s = self.status.lock().unwrap_or_else(|e| e.into_inner()).clone();
        UpdateStatus { state: if s.state.is_empty() { "idle" } else { s.state }, ..s }
    }

    fn set(&self, shared: &Shared, f: impl FnOnce(&mut UpdateStatus)) {
        f(&mut self.status.lock().unwrap_or_else(|e| e.into_inner()));
        let _ = shared.app.emit("state", shared.snapshot());
    }
}

/// Restarting now loses nothing: no day or single timer runs and nothing is blocked.
fn safe_to_restart(shared: &Shared) -> bool {
    let v = shared.snapshot().view;
    !v.lock.base && !v.lock.blocked && matches!(v.phase.kind.as_str(), "idle" | "done")
}

fn visible(shared: &Shared, label: &str) -> bool {
    shared
        .app
        .get_webview_window(label)
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
}

/// Nobody is in the middle of something: the window and the cards are closed, no end-of-block
/// line is waiting, and the user has been away from the keyboard for a while.
fn user_away(shared: &Shared) -> bool {
    !visible(shared, "main")
        && !visible(shared, "overlay")
        && shared.snapshot().view.pending_note.is_none()
        && crate::system::idle_ms() >= AWAY_MS
}

pub fn spawn(shared: Arc<Shared>) {
    std::thread::Builder::new()
        .name("updater".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK);
            let mut last_check: Ts = 0;
            loop {
                let now = clock::now_ts();
                let auto = shared.lock().cfg.auto_update;
                if auto && now - last_check >= CHECK_EVERY_MS {
                    last_check = now;
                    check(&shared, false);
                }
                // Like Discord: the update goes in by itself, but never under the user's hands
                // and never during a study day (a restart would pause the running timer).
                if installs_by_itself(&shared) && shared.updates.ready.lock().unwrap_or_else(|e| e.into_inner()).is_some()
                    && safe_to_restart(&shared)
                    && user_away(&shared)
                {
                    let _ = install(&shared);
                }
                std::thread::sleep(Duration::from_secs(60));
            }
        })
        .expect("spawn updater");
}

/// Look for a newer release and download it. `manual`: the user pressed "Check now".
pub fn check(shared: &Arc<Shared>, manual: bool) {
    {
        let mut busy = shared.updates.busy.lock().unwrap_or_else(|e| e.into_inner());
        if *busy || shared.updates.ready.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            return;
        }
        *busy = true;
    }
    shared.updates.set(shared, |s| {
        s.state = "checking";
        s.error = None;
    });
    let res = tauri::async_runtime::block_on(fetch(shared));
    let now = clock::now_ts();
    match res {
        Ok(Some((update, bytes))) => {
            let version = update.version.clone();
            *shared.updates.ready.lock().unwrap_or_else(|e| e.into_inner()) = Some((update, bytes));
            shared.updates.set(shared, |s| {
                s.state = "ready";
                s.version = Some(version.clone());
                s.checked_at = Some(now);
            });
            if !safe_to_restart(shared) || manual || !installs_by_itself(shared) {
                shared.notice(
                    &tr!("updater_clockmanage_is_ready", version = version),
                    if installs_by_itself(shared) { tr("updater_it_installs_by_itself_when_no") } else { tr("updater_press_restart_and_update_in_settings") },
                );
            }
        }
        Ok(None) => shared.updates.set(shared, |s| {
            s.state = "latest";
            s.version = None;
            s.checked_at = Some(now);
        }),
        Err(e) => {
            eprintln!("update check: {e}");
            shared.updates.set(shared, |s| {
                s.state = "error";
                s.error = Some(e);
                s.checked_at = Some(now);
            });
        }
    }
    *shared.updates.busy.lock().unwrap_or_else(|e| e.into_inner()) = false;
}

async fn fetch(shared: &Arc<Shared>) -> Result<Option<(Update, Vec<u8>)>, String> {
    // The installer starts after a hard exit: remove the tray icon first, or a ghost stays.
    let app = shared.app.clone();
    let updater = shared
        .app
        .updater_builder()
        .on_before_exit(move || app.cleanup_before_exit())
        .restart_after_install(false)
        .build()
        .map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let version = update.version.clone();
    shared.updates.set(shared, |s| {
        s.state = "downloading";
        s.version = Some(version);
    });
    let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    Ok(Some((update, bytes)))
}

/// Run the downloaded installer: the app exits here and the installer starts it again.
pub fn install(shared: &Arc<Shared>) -> Result<(), String> {
    if !safe_to_restart(shared) {
        return Err(tr("updater_the_update_installs_after_the_study").into());
    }
    let ready = shared.updates.ready.lock().unwrap_or_else(|e| e.into_inner()).take();
    let Some((update, bytes)) = ready else {
        return Err(tr("updater_no_downloaded_update").into());
    };
    // Windows: exits the process on success. Linux: the new AppImage or package is in place.
    let res = update.install(&bytes);
    if let Err(e) = &res {
        eprintln!("update install: {e}");
    }
    // A failed install starts over clean: the tray icon is gone and the windows are hidden by now
    // (on_before_exit); the next check downloads the update again. Linux starts the new version.
    if res.is_err() || cfg!(target_os = "linux") {
        shared.app.restart();
    }
    Ok(())
}

/// Windows installs by itself while nobody is at the PC. On Linux a .deb asks for the password
/// (pkexec dpkg), so the user installs with "Restart and update".
fn installs_by_itself(shared: &Shared) -> bool {
    shared.lock().cfg.auto_update && cfg!(windows)
}

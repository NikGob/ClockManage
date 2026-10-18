//! Self-update from GitHub Releases, no server of our own: the app reads `latest.json` of the
//! latest release (CI uploads it next to the signed installer), downloads the new installer in
//! the background and runs it when no study day or lock is running. On Linux the check only finds
//! the version: the package downloads when the user presses "Restart and update". The installer
//! closes the app, installs over it and starts it again: our NSIS hook does that
//! (`installer-hooks.nsh`), because the plugin's own restart runs it without admin rights and so
//! asks for UAC.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use clockmanage_core::clock::{self, Ts};
use clockmanage_core::i18n::tr;
use clockmanage_core::tr;
use serde::Serialize;
use tauri::{Emitter, Manager};
use tauri_plugin_updater::{Update, Updater, UpdaterExt};

use crate::app::Shared;

/// First check this long after the start (the app has settled, the network is up).
const FIRST_CHECK: Duration = Duration::from_secs(60);
const CHECK_EVERY_MS: Ts = 6 * 3600 * 1000;
/// An automatic install waits for this long without keyboard or mouse input.
const AWAY_MS: u64 = 5 * 60 * 1000;

#[derive(Serialize, Clone, Default)]
pub struct UpdateStatus {
    /// idle | checking | available | downloading | ready | latest | error
    pub state: &'static str,
    /// The new version (available / downloading / ready).
    pub version: Option<String>,
    pub checked_at: Option<Ts>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Updates {
    pub status: Mutex<UpdateStatus>,
    /// The update found, with the installer downloaded and verified against the public key in
    /// tauri.conf.json. Linux: without it, until `install` (an AppImage is about 80 MB).
    ready: Mutex<Option<(Update, Option<Vec<u8>>)>>,
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
            let state = if bytes.is_some() { "ready" } else { "available" };
            *shared.updates.ready.lock().unwrap_or_else(|e| e.into_inner()) = Some((update, bytes));
            shared.updates.set(shared, |s| {
                s.state = state;
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

async fn fetch(shared: &Arc<Shared>) -> Result<Option<(Update, Option<Vec<u8>>)>, String> {
    // The installer starts after a hard exit: remove the tray icon first, or a ghost stays.
    let app = shared.app.clone();
    let updater = shared
        .app
        .updater_builder()
        .on_before_exit(move || app.cleanup_before_exit())
        .restart_after_install(false)
        .build()
        .map_err(|e| e.to_string())?;
    find(&updater, |version| downloading(shared, version)).await
}

/// Windows downloads the installer with the check. Linux installs only on "Restart and update",
/// so it notes the version and doesn't hold the package in memory until then.
async fn find(updater: &Updater, on_download: impl FnOnce(&str)) -> Result<Option<(Update, Option<Vec<u8>>)>, String> {
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    if !cfg!(windows) {
        return Ok(Some((update, None)));
    }
    on_download(&update.version);
    let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    Ok(Some((update, Some(bytes))))
}

fn downloading(shared: &Shared, version: &str) {
    shared.updates.set(shared, |s| {
        s.state = "downloading";
        s.version = Some(version.to_owned());
    });
}

/// Run the installer: the app exits here and the installer starts it again.
pub fn install(shared: &Arc<Shared>) -> Result<(), String> {
    if !safe_to_restart(shared) {
        return Err(tr("updater_the_update_installs_after_the_study").into());
    }
    let ready = shared.updates.ready.lock().unwrap_or_else(|e| e.into_inner()).take();
    let Some((update, bytes)) = ready else {
        return Err(tr("updater_no_downloaded_update").into());
    };
    let bytes = match bytes {
        Some(bytes) => bytes,
        // Linux: the check found the version only, so the package downloads now. `busy` keeps
        // the 6-hourly check out meanwhile.
        None => {
            *shared.updates.busy.lock().unwrap_or_else(|e| e.into_inner()) = true;
            downloading(shared, &update.version);
            let res = tauri::async_runtime::block_on(update.download(|_, _| {}, || {}));
            *shared.updates.busy.lock().unwrap_or_else(|e| e.into_inner()) = false;
            match res {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("update download: {e}");
                    return Err(keep(shared, update, e.to_string()));
                }
            }
        }
    };
    // Windows: exits the process on success. Linux: the new AppImage or package is in place.
    match update.install(&bytes) {
        Ok(()) => shared.app.restart(),
        Err(e) => {
            eprintln!("update install: {e}");
            // Windows: the tray icon is gone and the windows are hidden by now (on_before_exit):
            // start over clean; the next check downloads the update again.
            if cfg!(windows) {
                shared.app.restart();
            }
            // Linux (the password prompt cancelled, a folder not writable): keep the update.
            Err(keep(shared, update, e.to_string()))
        }
    }
}

/// Linux: keep the update found after a failed download or install, but not its bytes: the next
/// "Restart and update" downloads it again.
fn keep(shared: &Shared, update: Update, error: String) -> String {
    *shared.updates.ready.lock().unwrap_or_else(|e| e.into_inner()) = Some((update, None));
    shared.updates.set(shared, |s| s.state = "available");
    tr!("settings_update_failed", error = error)
}

/// Windows installs by itself while nobody is at the PC. On Linux a .deb asks for the password
/// (pkexec dpkg), so the user installs with "Restart and update".
fn installs_by_itself(shared: &Shared) -> bool {
    shared.lock().cfg.auto_update && cfg!(windows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Only Windows downloads the package with the check: on Linux it would sit in memory (an
    /// AppImage is about 80 MB) until the user presses "Restart and update".
    #[test]
    fn only_windows_downloads_with_the_check() {
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let base = format!("http://{}", server.server_addr());
        let downloads = Arc::new(AtomicUsize::new(0));
        let (srv, count, url) = (server.clone(), downloads.clone(), base.clone());
        std::thread::spawn(move || {
            for req in srv.incoming_requests() {
                let body = if req.url() == "/latest.json" {
                    let package = serde_json::json!({ "url": format!("{url}/package"), "signature": "unsigned" });
                    let platforms: serde_json::Map<_, _> =
                        ["linux", "windows"].iter().map(|os| (format!("{os}-{}", std::env::consts::ARCH), package.clone())).collect();
                    serde_json::json!({ "version": "99.0.0", "platforms": platforms }).to_string()
                } else {
                    count.fetch_add(1, Ordering::SeqCst);
                    "package".into()
                };
                let _ = req.respond(tiny_http::Response::from_string(body));
            }
        });
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": "unused", "endpoints": [format!("{base}/latest.json")], "dangerousInsecureTransportProtocol": true }),
        );
        let app = tauri::test::mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(context).unwrap();
        let updater = app.updater_builder().build().unwrap();

        let mut announced = None;
        let found = tauri::async_runtime::block_on(find(&updater, |version| announced = Some(version.to_owned())));
        if cfg!(windows) {
            assert_eq!(downloads.load(Ordering::SeqCst), 1);
            assert_eq!(announced.as_deref(), Some("99.0.0"));
            assert!(found.is_err(), "an unsigned package passed");
        } else {
            let (update, bytes) = found.unwrap().expect("no update found");
            assert_eq!(update.version, "99.0.0");
            assert!(bytes.is_none() && announced.is_none());
            assert_eq!(downloads.load(Ordering::SeqCst), 0, "the check downloaded the package");
            // "Restart and update" downloads it (and refuses it: it isn't signed).
            assert!(tauri::async_runtime::block_on(update.download(|_, _| {}, || {})).is_err());
            assert_eq!(downloads.load(Ordering::SeqCst), 1);
        }
    }
}

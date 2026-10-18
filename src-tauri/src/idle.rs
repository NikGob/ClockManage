//! How long nobody has touched the keyboard or mouse on Linux, where the desktop tells: GNOME
//! and Cinnamon over D-Bus, an X11 session through the X server's IDLETIME counter, and Wayland
//! compositors with ext-idle-notify (KDE Plasma 5.27+, Sway, Hyprland, labwc, COSMIC). KDE's own
//! D-Bus method answers only on X11, which the counter already covers.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::{self, ExtIdleNotificationV1};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;

/// Milliseconds since the last input, or None when the desktop doesn't tell.
pub fn idle_ms() -> Option<u64> {
    mutter().or_else(x11).or_else(wayland)
}

/// GNOME (also Pantheon, Budgie up to 10.9) and Cinnamon: milliseconds, in X11 and Wayland.
fn mutter() -> Option<u64> {
    // A new connection each time: a kept one stays broken once the session bus restarts.
    let bus = zbus::blocking::connection::Builder::session().ok()?.method_timeout(Duration::from_secs(2)).build().ok()?;
    let names = zbus::blocking::fdo::DBusProxy::new(&bus).ok()?;
    let get = |name: &str, path: &str| {
        // Only a running service: calling a name nobody owns may start one (D-Bus activation).
        names.name_has_owner(name.try_into().ok()?).ok()?.then_some(())?;
        bus.call_method(Some(name), path, Some(name), "GetIdletime", &()).ok()?.body().deserialize::<u64>().ok()
    };
    get("org.gnome.Mutter.IdleMonitor", "/org/gnome/Mutter/IdleMonitor/Core")
        .or_else(|| get("org.cinnamon.Muffin.IdleMonitor", "/org/cinnamon/Muffin/IdleMonitor/Core"))
}

fn wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland")
}

/// The X server's IDLETIME counter (SYNC extension, libXext), in milliseconds. Not under
/// Xwayland: it sees only the input that goes to X11 windows.
fn x11() -> Option<u64> {
    use x11_dl::sync::{XSyncValue, Xext};
    use x11_dl::xlib::Xlib;
    if wayland_session() {
        return None;
    }
    let (xlib, xext) = (Xlib::open().ok()?, Xext::open().ok()?);
    unsafe {
        let display = (xlib.XOpenDisplay)(std::ptr::null());
        if display.is_null() {
            return None;
        }
        let (mut a, mut b) = (0, 0);
        let mut idle = None;
        if (xext.XSyncQueryExtension)(display, &mut a, &mut b) != 0 && (xext.XSyncInitialize)(display, &mut a, &mut b) != 0 {
            let mut n = 0;
            let list = (xext.XSyncListSystemCounters)(display, &mut n);
            if !list.is_null() {
                let counters = std::slice::from_raw_parts(list, n.max(0) as usize);
                if let Some(c) = counters.iter().find(|c| std::ffi::CStr::from_ptr(c.name).to_bytes() == b"IDLETIME") {
                    let mut value = XSyncValue { hi: 0, lo: 0 };
                    if (xext.XSyncQueryCounter)(display, c.counter, &mut value) != 0 {
                        idle = Some((u64::from(value.hi as u32) << 32) | u64::from(value.lo));
                    }
                }
                (xext.XSyncFreeSystemCounterList)(list);
            }
        }
        (xlib.XCloseDisplay)(display);
        idle
    }
}

/// ext-idle-notify only says "nobody has touched anything for NOTIFY_AFTER" and "someone did":
/// until the first notice the idle time counts as 0, after it as NOTIFY_AFTER plus the time since.
const NOTIFY_AFTER: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
enum Watch {
    Off,
    Starting,
    Active,
    Idle(Instant),
}

static WATCH: Mutex<Watch> = Mutex::new(Watch::Off);

fn set(watch: Watch) {
    *WATCH.lock().unwrap_or_else(|e| e.into_inner()) = watch;
}

/// A thread keeps a Wayland connection and follows the notices; the first call starts it and
/// knows nothing yet. If the compositor lacks the protocol, the next call tries again.
fn wayland() -> Option<u64> {
    std::env::var_os("WAYLAND_DISPLAY")?;
    let mut watch = WATCH.lock().unwrap_or_else(|e| e.into_inner());
    match *watch {
        Watch::Off => {
            *watch = Watch::Starting;
            let spawned = std::thread::Builder::new().name("idle-notify".into()).spawn(|| {
                let _ = follow();
                set(Watch::Off);
            });
            if spawned.is_err() {
                *watch = Watch::Off;
            }
            None
        }
        Watch::Starting => None,
        Watch::Active => Some(0),
        Watch::Idle(since) => Some((since.elapsed() + NOTIFY_AFTER).as_millis() as u64),
    }
}

/// Runs until the connection fails.
fn follow() -> Option<()> {
    let conn = Connection::connect_to_env().ok()?;
    let (globals, mut queue) = registry_queue_init::<Notices>(&conn).ok()?;
    let qh = queue.handle();
    let seat: WlSeat = globals.bind(&qh, 1..=1, ()).ok()?;
    let notifier: ExtIdleNotifierV1 = globals.bind(&qh, 1..=2, ()).ok()?;
    let after = NOTIFY_AFTER.as_millis() as u32;
    // Version 2 counts input only, like Windows: a playing video doesn't keep the PC busy.
    let _notification = if notifier.version() >= 2 {
        notifier.get_input_idle_notification(after, &seat, &qh, ())
    } else {
        notifier.get_idle_notification(after, &seat, &qh, ())
    };
    set(Watch::Active);
    loop {
        queue.blocking_dispatch(&mut Notices).ok()?;
    }
}

struct Notices;

impl Dispatch<ExtIdleNotificationV1, ()> for Notices {
    fn event(_: &mut Self, _: &ExtIdleNotificationV1, event: ext_idle_notification_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            ext_idle_notification_v1::Event::Idled => set(Watch::Idle(Instant::now())),
            ext_idle_notification_v1::Event::Resumed => set(Watch::Active),
            _ => {}
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Notices {
    fn event(_: &mut Self, _: &WlRegistry, _: wayland_client::protocol::wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

delegate_noop!(Notices: ignore WlSeat);
delegate_noop!(Notices: ExtIdleNotifierV1);

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a desktop session: run it under GNOME (or mutter --headless), X11 (or Xvfb -noreset)
    /// and a compositor with ext-idle-notify (sway on the headless backend, kwin_wayland --virtual).
    /// With IDLE_TEST_INPUT_AFTER=<s> it then waits s more seconds, while the script sends some
    /// input, and the idle time must have dropped.
    #[test]
    #[ignore = "needs a desktop session: cargo test -p clockmanage idle -- --ignored"]
    fn reads_the_idle_time() {
        // Wayland tells nothing before NOTIFY_AFTER without input.
        let deadline = Instant::now() + NOTIFY_AFTER + Duration::from_secs(10);
        let first = loop {
            match idle_ms() {
                Some(ms) if ms >= 1000 => break ms,
                other if Instant::now() > deadline => panic!("no idle time from this session: {other:?}"),
                _ => std::thread::sleep(Duration::from_millis(200)),
            }
        };
        std::thread::sleep(Duration::from_secs(3));
        let later = idle_ms().expect("the idle time went away");
        eprintln!("idle: {first} ms, 3 s later {later} ms");
        assert!(later >= first + 2500, "{first} -> {later}");
        if let Some(after) = std::env::var("IDLE_TEST_INPUT_AFTER").ok().and_then(|s| s.parse::<u64>().ok()) {
            std::thread::sleep(Duration::from_secs(after));
            let now = idle_ms().expect("the idle time went away");
            eprintln!("after the input: {now} ms");
            assert!(now < 2000, "the input didn't count: {now} ms");
        }
    }
}

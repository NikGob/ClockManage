<div align="center">

<img src="docs/media/cover.png" alt="ClockManage in dark and light theme" width="820">

# ClockManage

**A deep-work timer for Windows and Linux that blocks distractions for real — and lets Claude run your day.**

[![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?logo=windows&logoColor=white)](#install)
[![Linux](https://img.shields.io/badge/Linux-deb%20%7C%20AppImage-FCC624?logo=linux&logoColor=black)](#linux)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-core-B7410E?logo=rust&logoColor=white)](core)
[![MCP](https://img.shields.io/badge/MCP-21%20tools-6E56CF)](#claude-runs-your-day-mcp)
[![Android](https://img.shields.io/badge/Android-companion-3DDC84?logo=android&logoColor=white)](#android-companion)
[![Languages](https://img.shields.io/badge/UI-English%20%7C%20Russian-555)](#good-to-know)

[Install](#install) · [Features](#features) · [MCP](#claude-runs-your-day-mcp) · [Android](#android-companion) · [Development](#development)

</div>

---

Plan your day as study blocks, press **Start day**, and ClockManage does the rest: it runs each block in focused parts with breaks, rings when a break is over, and keeps Telegram, Discord, Shorts and whatever else you listed **closed until the work is done**. No browser extension to switch off, no "just five minutes": blocked sites are cut off in `hosts` and browser policies, and blocked apps simply can't start (on Linux they are closed within a second).

And because it speaks [MCP](https://modelcontextprotocol.io), Claude can see where you are, rearrange the rest of the day, add a site to the block list or lock everything for two hours — when you ask it to.

<div align="center">
  <img src="docs/media/timer.gif" alt="The timer: a wavy progress ring, pause and resume" width="860">
  <br><sub>The ring breathes while you work. Pause stops the clock honestly — and shows how long you've been away.</sub>
</div>

## Features

### A day, not a stopwatch

Every weekday gets a type — **Full**, **Light** or **Day off** — each with its own plan template and lock setting. A fresh install starts with a week of days off: pick your study days in *Settings → Week*. Today starts from its template; edit it on the **Plan** screen or let Claude do it. A block of 90 minutes runs as 45 + 45 with a 10-minute break, blocks are 20 minutes apart, and breaks never eat into study time. The day end (22:00 by default) is the hard stop: the lock lifts then even if the plan isn't done, and you can move it for today only — every shift goes to the log.

<div align="center">
  <img src="docs/media/plan.png" alt="Plan screen" width="49%">
  <img src="docs/media/today-light.png" alt="A break, light theme" width="49%">
</div>

### Breaks that wait for you

When a break ends the timer **doesn't** quietly carry on: a hand-drawn alarm pops up over everything and keeps ringing every minute until you press **Start part 2**. The lock stays on while it waits. When a block closes you get a little trophy and one line to answer — *what was boring, what distracted you?* — that lands in your log.

<div align="center">
  <img src="docs/media/alarm.gif" alt="Break over alarm" width="45%">
  <img src="docs/media/block.gif" alt="Block closed, with a note field" width="45%">
</div>

### Blocking that bites

| What | How |
|---|---|
| Domains — `x.com`, `twitch.tv`, `web.telegram.org` | a section in `hosts` + `URLBlocklist` policies of Chrome, Edge, Brave, Yandex and Firefox |
| Paths — `youtube.com/shorts` | browser policies only, so the rest of YouTube keeps working |
| Apps — `Telegram.exe`, `Discord.exe` | can't start (a tiny guard runs instead and says *Nope*); already running ones are closed within a second. On Linux they are closed within a second after they start |

During a study day the block list **can only grow**, the app can't be quit from the tray, and if the process is killed, `hosts` and policies stay in force while Task Scheduler (on Linux, a systemd user service) brings it back within 5 minutes. Want the break between blocks free? **Blocking → Access in breaks** opens everything while that break's timer runs; a pause and the short breaks between the parts of a block stay locked. Need out for real? **Emergency access** opens everything for 10 minutes after you type a long phrase by hand (pasting is disabled), and it goes to the log.

<div align="center">
  <img src="docs/media/blocking.png" alt="Blocking screen" width="62%">
  <img src="docs/media/nope.gif" alt="Nope: a wagging finger" width="30%">
</div>

### Lunch, naps and walks

**Segments** are non-study time with a countdown — or untimed, as a stopwatch. Put them in the plan ("lunch after Math") and they start by themselves when the block before them closes. A **nap** waits while you get ready, counts from the moment you press **In bed**, and ends with an alarm that won't stop until you press **I'm up** — even with the sound off. Segments never count as study, but the forecast "does the rest of the plan still fit?" takes them into account.

<div align="center">
  <img src="docs/media/nap.png" alt="Getting ready for a nap" width="56%">
  <img src="docs/media/wake.gif" alt="Nap alarm" width="36%">
</div>

### A journal that writes itself

Every day is logged: actual time per block, pauses, segments, emergency accesses, your end-of-block lines. The **Week** view adds it up into journal hours per subject (each block rounded down to 0.25 h) — one click copies it as a table for a spreadsheet. JSON and CSV export are one click too.

<div align="center">
  <img src="docs/media/log.png" alt="Log of a day" width="49%">
  <img src="docs/media/week.png" alt="Journal hours of a week" width="49%">
</div>

### Looks good doing it

Material 3 with your own colour: pick a swatch and the whole scheme is rebuilt from it, spreading across the window from where you clicked. Light, dark or follow the system. Hand-drawn illustrations "boil" like a cartoon while they move. Springy navigation, a cascade of content on every screen.

<div align="center">
  <img src="docs/media/theme.gif" alt="Changing the colour and the theme" width="49%">
  <img src="docs/media/tour.gif" alt="Moving between screens" width="49%">
</div>

### A mini timer that stays on top

A small always-on-top pill with the countdown and one button that does the right thing — pause, resume, start the next part, *In bed*. Drag it anywhere; there's a high-contrast variant for busy wallpapers.

<div align="center">
  <img src="docs/media/mini-dark.png" alt="Mini timer, dark" width="380">
  <img src="docs/media/mini-light.png" alt="Mini timer, light" width="380">
</div>

### Focus lock

"Block everything for 2 hours" — any day, even on a day off or after the study day ended. It can only be extended, never lifted early (except by your own emergency access), and survives a restart.

### Autolock before the day starts

Most time gets lost *before* the first block: one more video, one more chat. So on Full and Light days the block list goes on by itself at **13:00** if you haven't pressed **Start day** yet — sites, apps and the paired phone, as during a block. Five minutes before, a notice warns you. It holds until **Start day** (then the day's usual rules take over) or the day end, which meanwhile only moves later; only emergency access opens it early. Start before 13:00 and nothing happens. The day log records when it went on and how many minutes later the day started — the *start delay*. A restart or a power cut doesn't drop it. The time, or off, is set per profile in *Settings → Study day → Autolock before the start*, or ask Claude (`set_autolock`); a day off never has one.

### Food that fits your diet

Out of the box the diet aims at the brain, after the MIND diet: leafy greens, berries and nuts every day, oily fish, slow carbs, no sugar spikes or heavy lunches. Two more ready diets keep the brain rules and add more protein or fewer calories. Pick one, or write your own — or paste yours to Claude and say *"save this as my diet"*. Add the delivery address, how far away food may come from (3 km by default) and your meal times (13:00 and 19:00). Half an hour before each meal ClockManage asks an agent — Claude Code by default — to look through the Wolt restaurants and grocery stores that deliver to you and pick up to five restaurants with dishes that fit (never drinks), plus up to five stores with products to buy for the next days (berries, nuts, fish, greens…). The picks land on the **Meals** screen: **Open in Wolt** for each restaurant, and every dish is a link straight to it on Wolt, where you add it to the basket and order. ClockManage never fills the basket or orders anything. **Pick now** asks for a pick right away.

Pair an iPhone and the picks come to it as notifications: **Meals → Pair an iPhone** shows a QR code; open it with the camera, add the page to the Home Screen (Share → Add to Home Screen, in Safari or Chrome on iOS 16.4+) and open it from there — it connects by itself (or type the code). The iPhone app is a web app on GitHub Pages: **Breakfast**, **Lunch** or **Dinner** there asks the PC for a pick right away (the PC must be on), and the diet can be edited or switched there too. The PC and the iPhone talk through [ntfy.sh](https://ntfy.sh) with end-to-end encryption, and the PC sends the pushes itself: no account, no keys, no server of your own. The agent is Claude Code by default: ClockManage finds it by itself and runs it on its own login (run `claude` once and `/login`), without your user settings and hooks, allowed only the meal tools. **Meals → Agent → Custom command** runs Codex, Gemini CLI or any other agent with MCP instead: the task comes on stdin, `{mcp_url}` and `{mcp_config}` are filled in.

Wolt has no public API for customers: ClockManage reads the same pages wolt.com does, without logging in. Wolt's terms don't allow automated access, and the pages can change — then picking stops working until ClockManage is updated. Menus carry no nutrition data, so the agent judges dishes by their names and descriptions.

## Claude runs your day (MCP)

ClockManage runs a local MCP server (`127.0.0.1` only). The exact address and a ready-to-paste command are in **Settings → MCP for Claude**:

```sh
claude mcp add --transport http clockmanage http://127.0.0.1:<port>/mcp
```

Then just talk:

> - *"I'm behind on the thesis — move lunch after it and cut Spanish to one hour."*
> - *"How many hours did I study this week? Give me the table for the journal."*
> - *"Block Reddit and Steam and lock everything until 18:00."*
> - *"Close the current block, I'm done with math for today."*
> - *"I get up at 7 — make it night until 7 and hold for 3 seconds."*
> - *"On Light days lock everything at 11:30 if I haven't started yet."*
> - *"Here's my diet from the nutritionist — save it. Then find me lunch on Wolt."*

| Tool | What it does |
|---|---|
| `get_session_state` | what's running now: block, part, break or segment, time left, whether the lock is on |
| `get_today_stats` | actual time per block, journal hours, pauses, segments, notes, the autolock and the start delay — today or any `date` |
| `get_week_stats` | journal hours for a week by subject and day, with a ready TSV |
| `get_plan` | today's plan, all templates (with their autolock time), the weekday schedule, the next 7 days and today's autolock |
| `set_plan` | replace today's plan or set one for a future date; reorder anything, close a started block "as is" |
| `set_template` · `set_week_schedule` | edit the Full / Light / Day off templates and which weekday gets which |
| `set_day_end` | move today's day end once, with a reason for the log |
| `set_night_start` | the night start guard: until when it's night, whether after the day end too, how long to hold — or off |
| `set_autolock` | the autolock of the Full or Light profile: a time like 13:00, or off |
| `set_block_note` | save your line about a block |
| `finish_block` | close the current block at the time worked — two steps, needs your explicit "yes" |
| `get_blocklist` · `add_to_blocklist` · `remove_from_blocklist` | read and edit the block list (removing only outside the lock) |
| `start_focus_lock` | block the whole list right now for N minutes; can only be extended |
| `get_diet` · `set_diet` | read and replace your diet, with the address status, the distance limit and the meal times |
| `wolt_search` · `wolt_menu` | Wolt dishes and store products (by a query) or the restaurants and stores that deliver to you within the distance limit, and a venue's menu |
| `set_meal_options` | offer up to 5 restaurants with dishes and up to 5 stores with products: shown on the Meals screen and pushed to the paired iPhone |

The rules hold for the agent too: it can't erase worked time, can't shrink the block list during the lock, and can't close a block without asking you. Every change it makes shows up in the app as *"The agent changed …"*, so you always know it wasn't you.

## Android companion

A native app under a megabyte. Pair it on the same Wi-Fi: **Settings → Phone → Pair a phone** on the PC, **Find PC** and the PIN on the phone. It shows the timer and the plan, pauses and starts parts, keeps counting when offline, rings at the end of a break, has a home-screen widget — and blocks the chosen apps and the PC's sites on the phone through Accessibility. When you update the desktop app, the phone offers the update itself over Wi-Fi.

## Install

1. Download `ClockManage_<version>_x64-setup.exe` from [Releases](https://github.com/NikGob/ClockManage/releases) or from the latest run in [Actions → build](https://github.com/NikGob/ClockManage/actions/workflows/build.yml) (artifact `ClockManage-<version>-setup`; the Android APK is next to it).
2. Run it. A new version installs over the old one — settings and the log are kept. From 0.5.0 on the app updates itself from Releases (see [Good to know](#good-to-know)).
3. ClockManage asks for **administrator rights**: without them it can't write `hosts` and browser policies or close blocked apps. On first start it registers itself in Task Scheduler, so it starts at sign-in without a UAC prompt.

Uninstalling from *Apps & features* rolls back everything ClockManage wrote to the system.

### Linux

1. Download `ClockManage_<version>_amd64.deb` (Ubuntu 22.04, Debian 12, Mint 21 or newer) or `ClockManage_<version>_amd64.AppImage` (other distributions with glibc 2.35 or newer) from [Releases](https://github.com/NikGob/ClockManage/releases), or from the artifact `ClockManage-<version>-linux` of the latest run in [Actions → build](https://github.com/NikGob/ClockManage/actions/workflows/build.yml).
2. Install the .deb with `sudo apt install ./ClockManage_<version>_amd64.deb` and start ClockManage from the app menu. For the AppImage: `chmod +x` it and run it.
3. The app runs as you. Only `hosts` and the browser policies belong to root: for them the .deb installs a polkit rule that lets the signed-in user run one root step (`clockmanage --block-sites`, through `pkexec`) without a password. **The AppImage has no install step, so it closes blocked apps but doesn't block sites.**

ClockManage starts when you sign in (an entry in `~/.config/autostart`), and a systemd user service (`~/.config/systemd/user/clockmanage.service`) brings it back within 5 minutes if it is killed. `sudo apt remove clock-manage` takes the `hosts` section and the browser policies out.

What works differently from Windows:

- Blocked apps go by their process name: `Telegram.exe` in the list matches the process `Telegram` (Telegram from telegram.org). Ubuntu's own Telegram package runs as `telegram-desktop`: add that name. Apps under Wine match by their `.exe` name.
- A blocked app can start and is closed within a second: Linux has no way to stop it before it starts. A browser tab showing a blocked site gets no *Nope* (other apps' window titles can't be read on Wayland).
- Sites: `hosts`, a policy file of Chrome, Chromium, Edge, Brave and Yandex in `/etc/…/policies/managed/` (read at once) and rules in `/etc/firefox/policies/policies.json`. Flatpak browsers read neither.
- The tray icon opens its menu on a click; there is no click-to-open. The mini timer and the alarm stay on top on X11; under Wayland the compositor may not keep them on top or where they were.
- Updates: the AppImage downloads and installs a new version by itself, as on Windows, if it can write to its folder and the desktop reports idle time: GNOME, Cinnamon, Wayland compositors with ext-idle-notify (KDE Plasma 5.27 or newer, Sway, Hyprland, labwc, COSMIC) and any X11 session. Elsewhere, and always with a .deb (it asks for your password), the app only finds the update, and **Restart and update** installs it.

### Quick start

1. **Plan** — add today's blocks (or edit the Full / Light templates).
2. **Blocking** — check the sites and apps.
3. **Today** — press **Start day**.

## Good to know

- **Updates** come by themselves, like in Discord: every 6 hours ClockManage looks at the latest GitHub release, downloads a new version in the background and installs it when no study day or lock is running, the window is closed and nobody has touched the PC for 5 minutes. The app comes back on its own, in the tray. On Linux the AppImage does the same where the desktop reports idle time, and a .deb only finds the update (see [Linux](#linux)). *Settings → About ClockManage → Automatic updates* turns this off; **Check** looks right now, and **Restart and update** installs the new version at once (outside a study day). The installers are signed, and the app installs nothing else.
- **Language**: English and Russian, in *Settings → System → Language* — *Auto* follows the system language. The default plan names, segment types, emergency phrase and diet switch along until you change them. The time zone is there too (a new install takes the system's).
- Firefox only reads policies at start, so ClockManage restarts it when the lock turns on (tabs are restored). You can turn that off.
- Browsers check the block list when a page loads: a page that was already open when the lock started, or Shorts opened by a click inside YouTube, may show until the next navigation or refresh.
- **No start by accident at night**: before 05:00, or after the day end, **Start day** works only in the app window and takes a 2-second hold. The tray, the mini timer and the phone don't start a day then. The times and the hold are in *Settings → Study day → Start at night*, or ask Claude (`set_night_start`).
- **Autolock**: on Full and Light days the block list goes on at 13:00 when the day hasn't started — see [Autolock before the day starts](#autolock-before-the-day-starts). Once it is on today, changing the setting only affects the days ahead. A phone that has lost the PC learns about it on the next sync.
- An administrator can undo anything by hand. The point is to make distraction inconvenient, not impossible.
- Your data stays on your computer: `%APPDATA%\com.nikgob.clockmanage` on Windows, `~/.local/share/com.nikgob.clockmanage` on Linux (`config.json` and one JSON file per day). The day is saved every few seconds and survives a power cut: after a restart it comes back paused, and the time the PC was off is not counted. The Android phone talks to the PC over your local network only. Requests to the internet: the update check on GitHub and, once you use Meals, Wolt's pages (the restaurants near your address), OpenStreetMap's geocoder (when you press **Find** for an address), ntfy.sh (end-to-end encrypted, for the iPhone) and Apple's push service.

## Development

```sh
cargo test -p clockmanage-core -p cm-guard   # timer, lock rules, stats, MCP — runs on any OS
npx @tauri-apps/cli@2 dev                    # run on Windows (from an administrator console)
npx @tauri-apps/cli@2 build                  # NSIS installer on Windows; .deb and AppImage on Linux
```

On Linux the app needs WebKitGTK: `sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf` (Ubuntu 22.04 or newer), then `cargo test --workspace` runs the app's tests too. `src-tauri/tauri.linux.conf.json` is merged over `tauri.conf.json` on Linux: no launch guard; the .deb's polkit rule and remove script and the menu entry (also used by the AppImage) from `src-tauri/linux/`. `scripts/linux-smoke.sh <deb>` installs a built package and drives the running app over MCP — it changes `/etc/hosts`, so CI runs it on a throwaway runner.

UI preview in a normal browser with a mock backend: `cd ui && python -m http.server`, then `index.html?state=work` (`paused`, `break`, `await`, `idle`, `done`, `segment`; `state=idle&night=1` for a start at night, `state=idle&autolock=1` for the autolock, `state=break&ba=1` for access in the break), `overlay.html?demo=await` (`break`, `block`, `wake`, `nope`, …), `mini.html?state=work`; `index.html?route=meals` for the Meals screen (`&picks=0` without picks, `&paired=0` without an iPhone), `index.html?route=settings&update=ready` for a downloaded update (`available` for one found on Linux, `checking`, `downloading`, `error`); add `&lang=en` or `&lang=ru`, `&os=linux` for the Linux app's texts and `&admin=0` for an app that can't block sites (with `&os=linux`, the AppImage).

| Folder | What's inside |
|---|---|
| `core/` | platform-independent logic: config, the day engine, stats, the MCP handler, the `tr` runtime — covered by tests |
| `src-tauri/` | the desktop app for Windows and Linux: ticker, blocker (hosts, policies, processes), synthesized sounds, MCP and phone servers, tray and windows |
| `ui/` | main window, mini timer and alarm overlay — plain JS modules, no bundler; `ui/locales/` — the texts of every part |
| `android/` | the phone app, Kotlin with no dependencies: `cd android && gradle assembleRelease` |
| `pwa/` | the iPhone web app for meal picks — plain JS, served with `ui/locales/` by GitHub Pages (`.github/workflows/pwa.yml`; turn on *Settings → Pages → Source: GitHub Actions*) |

Every push builds the Windows installer, the Linux .deb and AppImage and the APK; the Linux job also runs the smoke test above. A `v*` tag publishes them as one release. With the `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets, CI signs the installer and the Linux packages and uploads `latest.json` next to them (one entry per platform and package type); installed apps read `releases/latest/download/latest.json` and check the signature against the public key in `src-tauri/tauri.conf.json`. Without the secrets the packages are built unsigned and apps don't see the release. Keep the private key: without it no update can be signed for the apps already installed.

The pictures and GIFs above are recorded from the UI on the mock backend by `scripts/media` (headless Chrome + ffmpeg) — rerun it after UI changes.

All texts live in one flat JSON file per language — [`ui/locales/en.json`](ui/locales/en.json) and [`ui/locales/ru.json`](ui/locales/ru.json) — shared by the Rust core, the app, the UI and the Android app: `tr("app_quit")`, `tr!("view_part_of", part = 2, parts = 3)` in Rust, `tr('settings_downloading', { version })` in JS, `tr("android_main_find_pc")` in Kotlin. Placeholders are named (`{min}`), so a translation may reorder them. `core/tests/locales.rs` keeps the files and the code in step: the same keys and placeholders in every language, every call names an existing key with exactly its arguments, no unused keys. A new language takes more than a new file: see *Adding a language* in [AGENTS.md](AGENTS.md).

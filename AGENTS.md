# AGENTS.md

Instructions for coding agents working on ClockManage. Two rules are not optional:

1. **Every user-visible text of the app ships in every supported language.** Today that is Russian and English, and both are always required. A language added later is just as required. The only exceptions are listed under [Deliberately not translated](#deliberately-not-translated).
2. **README.md stays true.** A change that makes any README sentence wrong is not finished.

## Checks

Before every commit (any OS, same as the Linux CI job):

```sh
cargo test -p clockmanage-core -p cm-guard
```

This never compiles the app in `src-tauri/`. If you touched `src-tauri/`, or a `core` API it uses (for example the `McpHost` trait), also check the app on Windows and on Linux. On Windows the app needs the launch guard as a sidecar, the same way the Windows CI job does:

```powershell
cargo build --release -p cm-guard
New-Item -ItemType Directory -Force src-tauri/binaries | Out-Null
Copy-Item target/release/cm-guard.exe src-tauri/binaries/cm-guard-x86_64-pc-windows-msvc.exe
cargo check -p clockmanage
```

Don't start the built app from a shell inside the Claude desktop app on Windows: it is an MSIX package, and every process it starts writes AppData into `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming`, so that app sees other settings than the real one started by autostart. Start it from a terminal opened outside Claude.

On Linux (Ubuntu 22.04 or newer, or a container of it, like the Linux CI job) no sidecar is needed: `src-tauri/tauri.linux.conf.json` drops the launch guard.

```sh
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf xvfb
cargo test --workspace
npx @tauri-apps/cli@2 build
bash scripts/linux-smoke.sh target/release/bundle/deb/*.deb   # changes /etc/hosts: throwaway machine only
```

For UI changes, look at the mock preview **in every language**:

```sh
python -m http.server -d ui
```

Open `index.html?state=work&route=<screen>&lang=en` for every screen you changed, then the same with every other language (`&lang=ru`, …). The screens are `today`, `plan`, `meals`, `block`, `log` and `settings`. Check `overlay.html?demo=await` and `mini.html?state=work` the same way. Always pass `&lang=`: without it the mock follows the browser's language. Texts that differ in the Linux app show with `&os=linux` (`&admin=0`: an app that can't block sites): check those as well when you change them.

## Localization: every language, every change

The app is multilingual: Settings → System → Language (Auto follows Windows; the phone follows the paired PC, and the phone's own language before pairing, unless its Settings pick one). Any change that adds or changes visible text ships it in **every** supported language in the same commit. Visible text includes:

- labels, buttons, tooltips, `placeholder`/`title`/`aria-label`
- toasts, notifications, tray items, overlays
- refusals and errors shown to the user, MCP result texts
- phone screens and the widget

Rules for every text:

- Never hardcode visible text in any language.
- Never leave a language as a TODO, and never copy one language's text into another language's file.
- If code you touch still has a hardcoded or missing-language text, wire it through `tr` too.

### How to add a text

1. Add the key to **every** file in `ui/locales/`: today `en.json` and `ru.json`.
   - Key format: snake_case `<area>_<first words of the English text>`, where the area is the file or module (`today_`, `settings_`, `dialog_`, `app_`, `day_`, `mcp_`, `android_main_`, …). Reuse an existing key when the text is the same.
   - Placeholders are named, `{min}`. Every language needs the same set of placeholders; the order may differ.
   - Keep the file format: keys sorted, 2-space indent, UTF-8 without `\u` escapes, trailing newline (`json.dumps(d, indent=2, ensure_ascii=False, sort_keys=True) + "\n"`).
   - Follow each language's typography. Russian uses «» and ё.
2. Call `tr` with a **literal** key:
   - Rust: `tr("key")`, or `tr!("key", name = value)` when the text has placeholders. Imports: `use crate::i18n::tr; use crate::tr;` in `core`, `use clockmanage_core::i18n::tr; use clockmanage_core::tr;` in `src-tauri`. Don't write a path-qualified `i18n::tr(…)`: the checker skips it.
   - JS: `import { tr } from './i18n.js'`, then `tr('key', { name })`. The arguments must be an object literal. The iPhone web app in `pwa/` has its own small `tr` with the same call shape and reads the same `ui/locales/` files (the Pages workflow copies them next to it).
   - Kotlin: `tr("key", "name" to value)`.
   - Key chosen at runtime: use a `match` or a lookup object of literal `tr` calls (see `DayKind::label` in `core/src/config.rs` and the tip tables in `ui/js/views/settings.js`). Never `tr(variable)`.
3. When code goes away, delete its keys from every file. Unused keys fail the test.

`core/tests/locales.rs` catches mismatched keys and placeholders between `ru.json` and `en.json`, a wrong or non-literal key, wrong argument names and unused keys. It does **not** catch:

- hardcoded text
- a value left untranslated
- `strings.xml` parity
- any locale file other than `ru.json` and `en.json` (see [Adding a language](#adding-a-language))

Check those yourself.

### Outside `ui/locales`, still in every language

- **Android texts the system reads** (manifest labels, the accessibility service and widget descriptions) cannot use `tr`. Add them to `android/app/src/main/res/values/strings.xml` (English, the default) **and** to `values-<lang>/strings.xml` for every other language (today `values-ru`).
- **Linux texts the system shows** cannot use `tr` either: the polkit action in `src-tauri/linux/com.nikgob.clockmanage.policy` (`<description xml:lang="ru">`, `<message xml:lang="ru">`) and the menu entry's `Comment[ru]` in `src-tauri/linux/clockmanage.desktop`. Add a variant for every other language.
- **Mock preview data** in `ui/js/mock.js` keeps its texts inline, one per language: today `L(ru, en)` pairs. A new `Config` setting also needs its default in the mock `cfg` there, or the preview and the README pictures show it unset.
- **Static tray items**: set them in `src-tauri/src/windows.rs` and again in the `TRAY_LANG` block of `src-tauri/src/app.rs`. Otherwise they keep the old language after a switch.

### Deliberately not translated

- **MCP tool descriptions and server instructions** in `core/src/mcp.rs`: English only, because the model reads them. MCP *results* do go through `tr`. `core/tests/english.rs` rejects Cyrillic in `tools/list` only, so keep the `initialize` instructions English yourself.
- **User data**: block and segment names, the emergency phrase, the diet. Defaults are written in the system language on the first run; while the user hasn't changed them, they follow the interface language (`Config::follow_lang`). A new default name needs its key in the lists there.
- **Fixed strings**: config and JSON keys, the brand name, the author's signature *Made by NikGob* at the end of Settings, and the language names in the picker (each written in its own language).
- **`README.md` and its pictures** in `docs/media/`: English.

### Formats and traps

- **Plurals.** There is no plural helper. Write `label: N` or abbreviated units (`{r} мин` / `{r} min`), not inflected words.
- **Numbers and dates.** In JS, format decimals with `dec()` and dates with `LOCALE` from `ui/js/i18n.js`. In app texts, format durations with `fmt_dur` in `src-tauri/src/app.rs`. Don't use `fmt_min` from `core/src/day.rs` for new non-Russian text: it always prints a decimal comma.
- **The UI parses some backend texts.**
  - Regexes in `ui/js/mini.js` and `ui/js/views/today.js` match the wording of `view_part_of`, `view_*_part_of` and the h/min units in every language.
  - `ui/js/views/log.js` compares against every language's value of `stats_running`.
  - If you reword those texts or add a language, update both sides.
- **Windows and Linux.** A text that names Windows things (Task Scheduler, UAC, administrator rights, "Like Windows") has a Linux variant, picked with `LINUX` from `ui/js/api.js`. Change both, in every language.
- **Tests.** Core unit tests run in Russian, because the process-wide language defaults to Russian. Assert texts of any other language only in a separate file under `core/tests/` (like `english.rs`): each file runs as its own process.
- **Rebuild.** The locale files are compiled into the Rust binary (`include_str!`) and packed into the APK. Rebuild after editing them.
- **CSP.** The CSP in `src-tauri/tauri.conf.json` must keep `connect-src 'self'`, or the UI shows raw keys. `core/tests/locales.rs` checks this.

### Adding a language

Copying a locale file is not enough: the list of languages is wired into the code. A new language needs all of these, in the same PR:

- `ui/locales/<lang>.json` with every key.
- `core/src/i18n.rs`: `Lang`, `LangPref`, `from_locale` and `FILES`.
- `src-tauri/src/system.rs`: map the Windows UI language id to the new `Lang`.
- `ui/js/i18n.js`: the accepted codes, the `navigator.language` check, `LOCALE`, `dec()` and `followLang`.
- `pwa/app.js`: the `navigator.language` check, `LOCALE` and `setLang` of the iPhone web app (it follows the PC's language once a day message brings it).
- `android/.../I18n.kt`, plus `res/values-<lang>/strings.xml`.
- `src-tauri/linux/`: the polkit action's description and message, and the `.desktop` comment.
- `core/tests/locales.rs`: compare every locale file, not only `ru` and `en`.
- `ui/js/views/settings.js`: the language picker.
- `src-tauri/tauri.conf.json`: the NSIS installer `languages`.
- The texts the UI parses (see the trap above) and the mock's `L()` in `ui/js/mock.js`.
- The README: the Languages badge and Good to know.
- This file: the "today" lists.

## Meals: an optional feature

Meals is off by default (`Config::meals.enabled`, *Settings → Meals*). While it is off nothing of it may run: its thread isn't started (`meals::wake` starts it when it is turned on), it makes no requests, the menu has no Meals screen and MCP neither lists nor runs its tools (`MEAL_TOOLS` in `core/src/mcp.rs`). Keep it that way for anything you add to Meals, and gate new Meals commands with `check_on`.

The paired iPhone is not part of Meals: its relay thread (`src-tauri/src/iphone.rs`) runs while an iPhone is linked and sends it the day. What the web app sends for Meals goes to `meals::from_phone`, which ignores it while Meals is off; Meals sends its picks and the diet through `iphone::send` and `iphone::push`, only while it is on.

### Adding a delivery service

Meals picks food from one delivery service at a time, today only Wolt. A new one (Yandex Eda, Samokat, Kuper, …) needs, in the same PR:

- `core/src/meals.rs`: its id and brand name in `SERVICES`.
- A parser module like `core/src/wolt.rs`, with tests on sample JSON: venues (`Venue`, within `max_km`, nearest first), search results and menus (`Dish`). Prices are in minor units of the currency as ISO 4217 counts them (kopecks for RUB, none for JPY): the UI and the iPhone app format them with `Intl`.
- `src-tauri/src/meals.rs`: a `Service` implementation and its arm in `service()`. `owns()` decides which links the app opens.
- `src-tauri/tauri.conf.json`: its picture host in the CSP `img-src`, or the photos don't show.
- The README: Food that fits your diet (which services) and Good to know (the new internet requests).

Texts never name a service: they take the brand name as `{service}`.

## README: keep it up to date

`README.md` is the user's manual. It covers the features with pictures, the MCP tool table, settings, install and development.

**With every change**, re-read the README parts your change touches and fix them in the same PR. This covers new, renamed or removed features, settings, MCP tools, commands, preview URLs, file locations and the folder layout.

**When asked to check the README**, go through every section against the code using the map below. Fix what is wrong and list in the PR what changed.

| When you change | Re-check in the README |
|---|---|
| MCP tools (`core/src/mcp.rs`) | the "MCP N tools" badge, the tool table and the example prompts. Outside the README, two more tool lists: the `initialize` instructions in `core/src/mcp.rs` and Settings → MCP for Claude (`ui/js/views/settings.js`) |
| settings, defaults, rules (`core/src/config.rs`, `core/src/day.rs`, `core/src/segments.rs`) | Features, Good to know |
| blocking (`src-tauri/src/blocker.rs`, `guard/`, `src-tauri/linux/`) | Blocking that bites, Good to know, Install → Linux |
| anything visible in the UI (`ui/`) | the Features section showing it, and its picture |
| mock states, overlay demos, routes (`ui/js/mock.js`, `ui/js/main.js`) | Development: the preview URLs |
| build, CI, installer (`.github/workflows/`, `Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/tauri.linux.conf.json`, `scripts/linux-smoke.sh`) | Install, Development |
| the i18n machinery or a language | the Languages badge, Good to know, the localization paragraph in Development |
| the phone app (`android/`, `core/src/phone.rs`, `src-tauri/src/phone_server.rs`) | Android companion |
| meals, delivery services (`core/src/meals.rs`, `core/src/wolt.rs`, `src-tauri/src/meals.rs`) | Food that fits your diet, the MCP tool table, Good to know (internet requests) |
| the iPhone web app and its relay (`pwa/`, `core/src/iphone.rs`, `core/src/relay.rs`, `src-tauri/src/iphone.rs`, `scripts/media/iphone-day.mjs`) | iPhone companion, Good to know (internet requests), Development (the preview) |
| top-level folders | the folder table in Development |

### Pictures

The pictures and GIFs in `docs/media/` are recorded in English from the mock backend. When a visible change alters what a picture shows, re-record the pictures. Leave the media alone for invisible changes, Rust-only changes, and text changes that don't touch English.

```sh
python -m http.server 8766 -d ui
node scripts/media/capture.mjs <stills> <frames> <chrome-profile>
python scripts/media/build.py <stills> <frames> docs/media
```

- Run the server in a second terminal, and the scripts from the repo root.
- Recording needs Chrome at its default Windows path, Node 22+, Python with Pillow, and ffmpeg.
- Use empty scratch folders, and create `<stills>` first. Keep `<chrome-profile>` on a short path (`C:\cmp`): past Windows' 260-character paths Chrome's cache storage fails and the iPhone pictures lose their picks. `capture.mjs` wipes the Chrome profile and each frames subfolder it records.
- `build.py` rebuilds every picture from a full capture, so run `capture.mjs` without names.
- If you can't record, say so in the PR description.

If you notice the README is already wrong somewhere you didn't touch, fix it when it's small. Otherwise mention it in the PR.

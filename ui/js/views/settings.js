import { call, esc, hm as clock } from '../api.js';
import { icon, CHECK } from '../icons.js';
import { applyTheme } from '../theme.js';
import { run, snack, menu, WEEKDAYS, WEEKDAYS_FULL, KINDS, kindLabel, planSummary, hm } from '../ui.js';
import { tr, LOCALE, dec } from '../i18n.js';
import { NIKGOB } from '../logo.js';

const SWATCHES = [
  ['#2E7D32', tr('settings_green')], ['#00796B', tr('settings_teal')], ['#1565C0', tr('settings_blue')], ['#5E35B1', tr('settings_purple')],
  ['#C2185B', tr('settings_raspberry')], ['#E65100', tr('settings_orange')], ['#6D4C41', tr('settings_brown')],
];

// Longer explanations live in tooltips so the screen itself stays short.
const TIP = {
  block: tr('settings_on_such_a_day_start_day'),
  plan: tr('settings_every_such_day_starts_with_this'),
  dayEnd: tr('settings_when_the_lock_lifts_by_itself'),
  night: tr('settings_at_night_start_day_works_only'),
  autolock: tr('settings_sites_apps_and_the_phone_are'),
  seg: tr('settings_a_block_is_cut_into_parts'),
  short: tr('settings_rest_between_the_parts_of_one'),
  between: tr('settings_rest_after_a_closed_block_before'),
  segments: tr('settings_non_study_segments_with_a_countdown'),
  segPrep: tr('settings_time_to_get_ready_before_the'),
  segAlarm: tr('settings_the_end_is_a_loud_alarm'),
  segAccess: tr('settings_while_the_segment_runs_until_its'),
  sound: tr('settings_a_ring_at_the_end_of'),
  overlay: tr('settings_a_hand_drawn_card_on_top'),
  reminder: tr('settings_while_the_timer_waits_for_start'),
  contrast: tr('settings_inverted_colours_a_dark_pill_in'),
  phone: tr('settings_the_clockmanage_android_app_sees_the'),
  mcp: tr('settings_a_local_server_for_claude_it'),
  autostart: tr('settings_starts_when_you_sign_in_to'),
  lang: tr('settings_the_language_of_the_interface_notifications'),
  tz: tr('settings_the_day_end_dates_and_the'),
  update: tr('settings_every_6_hours_clockmanage_looks_for'),
};
const MODE_TIP = { system: tr('settings_like_windows'), light: tr('settings_always_light'), dark: tr('settings_always_dark') };
const VARIANT_TIP = {
  fidelity: tr('settings_the_colour_exactly_as_picked_the'),
  tonal_spot: tr('settings_muted_tones_like_androids_default'),
  vibrant: tr('settings_maximum_colour_everywhere'),
};

/** "UTC+03:00" for an offset in minutes. */
function utcLabel(min) {
  const a = Math.abs(min);
  return `UTC${min < 0 ? '−' : '+'}${String(Math.floor(a / 60)).padStart(2, '0')}:${String(a % 60).padStart(2, '0')}`;
}
const TZ_OFFSETS = Array.from({ length: (840 + 720) / 30 + 1 }, (_, i) => -720 + i * 30);

function snap(v, dir, step) {
  if (dir > 0) return v < step ? v + 1 : (Math.floor(v / step) + 1) * step;
  return v <= step ? v - 1 : (Math.ceil(v / step) - 1) * step;
}

// Black or white check on top of a swatch, whichever reads better.
function onColor(hex) {
  const n = parseInt(hex.slice(1), 16);
  const lum = (0.299 * ((n >> 16) & 255) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255)) / 255;
  return lum > 0.6 ? '#1b1b1b' : '#fff';
}

export function mountSettings(root, ctx) {
  let cfg = null;
  let last = null;
  let pulse = null; // selector of the control the user just changed

  root.innerHTML = `<div class="readable" id="settings"></div>`;
  const box = root.querySelector('#settings');

  const tipAttr = (key) => (TIP[key] ? `data-tip="${esc(TIP[key])}"` : '');
  const info = (key) => `<span class="info" ${tipAttr(key)} tabindex="0">${icon('info')}</span>`;
  const seg = (name, value, opts, tips = {}) => `<div class="segmented" role="group" data-seg="${name}">${opts.map(([v, l]) =>
    `<button class="interactive" data-v="${v}" aria-pressed="${v === value}" ${tips[v] ? `data-tip="${esc(tips[v])}"` : ''}>${CHECK}${l}</button>`).join('')}</div>`;
  const sw = (path, v, label, tip, disabled = false) =>
    `<input type="checkbox" class="switch" role="switch" data-sw="${path}" ${v ? 'checked' : ''} ${disabled ? 'disabled' : ''} aria-label="${esc(label)}" ${tipAttr(tip)}>`;
  const get = (path) => path.split('.').reduce((o, k) => o[k], cfg);
  /** − value + for minutes/seconds; snaps to multiples of `step`, single units below it.
   *  `ceil` caps growth (a locked break can only get shorter). */
  const stepper = (path, { min, max, step, unit, label, disabled = false, ceil = Infinity }) => {
    const v = get(path);
    const down = snap(v, -1, step);
    const up = snap(v, 1, step);
    return `<div class="num-step" role="group" aria-label="${esc(label)}">
      <button class="icon-btn interactive" data-step="${path}" data-d="-1" data-sz="${step}" data-lo="${min}" data-hi="${Math.min(max, ceil)}" aria-label="${tr('settings_less')}: ${down} ${unit}" ${disabled || down < min ? 'disabled' : ''}>${icon('remove')}</button>
      <span class="v tnum">${v}<small> ${unit}</small></span>
      <button class="icon-btn interactive" data-step="${path}" data-d="1" data-sz="${step}" data-lo="${min}" data-hi="${Math.min(max, ceil)}" aria-label="${tr('settings_more')}: ${up} ${unit}" ${disabled || up > Math.min(max, ceil) ? 'disabled' : ''}>${icon('add')}</button>
    </div>`;
  };

  function weekSection(locked, todayIdx) {
    const tiles = WEEKDAYS.map((d, i) => {
      const k = cfg.week[i];
      const kd = KINDS.find((x) => x.id === k);
      return `<button class="day-tile interactive${i === todayIdx ? ' today' : ''}" data-day="${i}" data-kind="${k}"
        data-tip="${esc(tr('settings_day_click_to_change', { day: WEEKDAYS_FULL[i], today: i === todayIdx ? tr('settings_today_mark') : '', kind: kd.label.toLowerCase() }))}">
        <span class="dn">${d}</span>${icon(kd.icon)}<span class="kn">${kd.label}</span></button>`;
    }).join('');
    const profiles = KINDS.map((k) => {
      const p = cfg.profiles[k.id];
      return `<div class="setting profile" data-kind="${k.id}">
        <span class="badge">${icon(k.icon)}</span>
        <div class="grow"><div class="t">${k.label}</div><div class="d ellipsis" title="${esc(planSummary(p.plan))}">${esc(planSummary(p.plan))}</div></div>
        <button class="btn text interactive" data-tpl="${k.id}" ${tipAttr('plan')}>${icon('edit')}${tr('main_plan')}</button>
        <label class="sw-wrap"><span class="sw-label">${tr('day_block')}</span>${sw(`profiles.${k.id}.block`, p.block, `${tr('settings_lock')}: ${k.label}`, 'block')}</label>
      </div>`;
    }).join('');
    return `
      <section class="section">
        <h2>${tr('log_week')}</h2>
        <div class="surface">
          <div class="setting col">
            <div class="week" role="group" aria-label="${tr('settings_the_type_of_each_weekday')}">${tiles}</div>
            <div class="week-legend"><span><i class="full"></i>${tr('config_full')}</span><span><i class="light"></i>${tr('config_light')}</span><span><i></i>${tr('config_day_off')}</span>
              <span class="grow"></span><span>${locked ? tr('settings_today_is_running_changes_apply_to') : tr('settings_click_a_day_to_pick_its')}</span></div>
          </div>
          ${profiles}
        </div>
      </section>`;
  }

  function ago(ts) {
    if (!ts) return tr('settings_hasnt_connected_yet');
    const min = Math.floor((Date.now() - ts) / 60000);
    return min < 1 ? tr('settings_online') : min < 60 ? tr('settings_last_seen_min_ago', { min }) : `${tr('settings_last_seen_at')} ${new Date(ts).toLocaleTimeString(LOCALE, { hour: '2-digit', minute: '2-digit' })}`;
  }

  function phoneSection(ph) {
    const on = cfg.phone.enabled;
    const addr = ph.address ? `${ph.address}:${ph.port}` : '';
    const state = !on ? tr('settings_off') : ph.running ? `${tr('settings_running')}${addr ? ` · ${esc(addr)}` : ''} · ${tr('system_pc')} «${esc(ph.pc_name)}»` : `${tr('settings_not_running')}${ph.error ? ': ' + esc(ph.error) : ''}`;
    const devices = ph.devices.map((d) => `<div class="setting">
        <span class="badge">${icon('phone')}</span>
        <div class="grow"><div class="t">${esc(d.name)}</div><div class="d">${ago(d.last_seen)}${d.blocker === true ? tr('settings_the_phone_lock_is_on') : d.blocker === false ? ` · ${tr('settings_the_phone_lock_is')} <b>${tr('phone_off')}</b>` : ''}</div></div>
        <button class="btn text interactive" data-forget="${esc(d.id)}">${tr('settings_unpair')}</button></div>`).join('');
    const pin = ph.pin ? `<div class="setting col pin-box">
        <div class="t">${tr('settings_pin_for_the_phone')}</div>
        <div class="pin tnum" aria-live="polite">${esc(ph.pin.slice(0, 3))} ${esc(ph.pin.slice(3))}</div>
        <div class="d">${tr('settings_open_clockmanage_on_the_phone_in')} ${new Date(ph.pin_until).toLocaleTimeString(LOCALE, { hour: '2-digit', minute: '2-digit' })}.</div></div>` : '';
    return `<section class="section">
        <h2>${tr('phone_phone')}</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">${tr('settings_sync_with_android')}${info('phone')}</div><div class="d">${state}</div></div>${sw('phone.enabled', on, tr('settings_phone_sync'), 'phone')}</div>
          ${devices}
          ${pin}
          ${on && ph.apk?.available ? `<div class="setting"><div class="grow"><div class="t">${tr('settings_phone_app')}</div><div class="d">${tr('settings_this_build_includes_version')} ${esc(ph.apk.name)} (${dec((ph.apk.size / 1048576).toFixed(1))} ${tr('settings_mb_a_paired_phone_offers_the')}</div></div></div>` : ''}
          ${on ? `<div class="setting"><div class="grow"><div class="d">${tr('settings_the_phone_and_the_pc_must')}</div></div>
            <button class="btn tonal interactive" id="phone-pin">${icon('add')}${tr('settings_pair_a_phone')}</button></div>` : ''}
        </div>
      </section>`;
  }

  function updateRow(u, v) {
    const ver = esc(u.version || '');
    const state = {
      checking: tr('settings_checking'),
      downloading: tr('settings_downloading', { version: ver }),
      ready: cfg.auto_update ? tr('settings_version_is_downloaded_it_installs', { version: ver }) : tr('settings_version_is_downloaded', { version: ver }),
      latest: u.checked_at ? tr('settings_latest_version_checked_at', { time: clock(u.checked_at) }) : '',
      error: tr('settings_update_failed', { error: esc(u.error || '') }),
    }[u.state] || (cfg.auto_update ? tr('settings_from_github_releases') : tr('settings_off_only_by_the_check'));
    const busy = ['checking', 'downloading'].includes(u.state);
    // The backend refuses a restart while a day or a lock runs: say so instead of a dead click.
    const canRestart = !v.lock.base && !v.lock.blocked && ['idle', 'done'].includes(v.phase.kind);
    return `<div class="setting"><div class="grow"><div class="t">${tr('settings_automatic_updates')}${info('update')}</div><div class="d">${state}</div></div>
      ${u.state === 'ready'
        ? `<button class="btn tonal interactive" id="installupdate" ${canRestart ? '' : `disabled data-tip="${esc(tr('updater_the_update_installs_after_the_study'))}"`}>${icon('download')}${tr('settings_restart_and_update')}</button>`
        : `<button class="btn text interactive" id="checkupdate" ${busy ? 'disabled' : ''}>${icon('refresh')}${tr('settings_check')}</button>`}
      ${sw('auto_update', cfg.auto_update, tr('settings_automatic_updates'), 'update')}</div>`;
  }

  function render() {
    if (!cfg || !last) return;
    const v = last.view;
    // The same rule the backend checks: a study day, a blocking single timer or a focus lock.
    const locked = v.lock.base;
    const m = last.meta;
    const mcpUrl = m.mcp.url || `http://127.0.0.1:${cfg.mcp_port}/mcp`;
    const cmd = `claude mcp add --transport http clockmanage ${mcpUrl}`;
    const custom = !SWATCHES.some(([c]) => c.toLowerCase() === cfg.appearance.seed.toLowerCase());
    const focusKey = keyOf(document.activeElement);
    const t = cfg.timing;
    const ns = cfg.night_start;
    box.innerHTML = `
      ${locked ? `<div class="lock-note">${icon('lock')}<div class="body-m">${v.autolock?.on ? tr('settings_the_autolock_is_on_until') : tr('settings_a_study_day_is_running_the')}</div></div>` : ''}
      ${weekSection(locked, v.weekday)}

      <section class="section">
        <h2>${tr('settings_study_day')}</h2>
        <p class="section-d">${tr('settings_when_the_lock_lifts_how_a')}</p>
        <div class="surface">
          <div class="setting">
            <div class="grow"><div class="t">${tr('dialog_day_end')}${info('dayEnd')}</div><div class="d">${tr('settings_the_lock_lifts_at_this_time')} (${utcLabel(cfg.tz_offset_min)})${v.day_end_changed ? ` · ${tr('settings_today_until')} ${esc(v.day_end)}${v.day_end_next_day ? tr('settings_night') : ''}` : ''}</div></div>
            <div class="field"><input type="time" id="dayend" value="${hm(cfg.day_end_min)}" ${locked ? `min="${hm(cfg.day_end_min)}"` : ''} aria-label="${tr('dialog_day_end')}" ${tipAttr('dayEnd')}></div>
          </div>
          <div class="setting"><div class="grow"><div class="t">${tr('settings_start_at_night')}${info('night')}</div><div class="d">${ns.enabled ? tr('app_night_start_until_hold', { until: hm(ns.until_min), after: ns.after_day_end ? tr('app_night_start_and_after_the_day_end') : '', hold: ns.hold_sec }) : tr('settings_one_click_starts_the_day_any')}</div></div>
            ${sw('night_start.enabled', ns.enabled, tr('settings_start_at_night'), 'night')}</div>
          ${ns.enabled ? `<div class="setting sub"><div class="grow"><div class="t">${tr('settings_night_lasts_until')}</div></div>
            <div class="field"><input type="time" id="nightuntil" value="${hm(ns.until_min)}" max="12:00" aria-label="${tr('settings_night_lasts_until')}"></div></div>
          <div class="setting sub"><div class="grow"><div class="t">${tr('settings_after_the_day_end_too')}</div></div>
            ${sw('night_start.after_day_end', ns.after_day_end, tr('settings_after_the_day_end_too'))}</div>
          <div class="setting sub"><div class="grow"><div class="t">${tr('settings_hold_to_start')}</div></div>
            ${stepper('night_start.hold_sec', { min: 1, max: 10, step: 1, unit: tr('settings_s'), label: tr('settings_hold_to_start') })}</div>` : ''}
          <div class="setting"><div class="grow"><div class="t">${tr('settings_autolock_before_the_start')}${info('autolock')}</div><div class="d">${tr('settings_if_the_day_hasnt_started_by')}</div></div></div>
          ${['full', 'light'].map((id) => {
            const k = KINDS.find((x) => x.id === id);
            const p = cfg.profiles[id];
            const label = `${tr('settings_autolock_before_the_start')}: ${k.label}`;
            return `<div class="setting sub"><div class="grow"><div class="t">${k.label}</div>${p.block ? '' : `<div class="d">${tr('settings_no_lock_on_such_a_day')}</div>`}</div>
              ${p.autolock ? `<div class="field"><input type="time" data-autolock="${id}" value="${hm(p.autolock_min)}" aria-label="${esc(label)}"></div>` : ''}
              ${sw(`profiles.${id}.autolock`, p.autolock, label, 'autolock')}</div>`;
          }).join('')}
          <div class="setting${locked ? ' off' : ''}"><div class="grow"><div class="t">${tr('settings_work_part')}${info('seg')}</div><div class="d">${tr('settings_work_part_example')}</div></div>
            ${stepper('timing.work_segment_min', { min: 5, max: 240, step: 5, unit: tr('overlay_min'), label: tr('settings_minutes_of_work_per_part'), disabled: locked })}</div>
          <div class="setting"><div class="grow"><div class="t">${tr('settings_break_between_parts')}${info('short')}</div><div class="d">${tr('settings_between_the_parts_of_one_block')}</div></div>
            ${stepper('timing.short_break_min', { min: 1, max: 120, step: 5, unit: tr('overlay_min'), label: tr('settings_break_minutes'), ceil: locked ? t.short_break_min : Infinity })}</div>
          <div class="setting"><div class="grow"><div class="t">${tr('view_break_between_blocks')}${info('between')}</div><div class="d">${cfg.break_access ? tr('settings_after_a_closed_block_blocked_things_open') : tr('settings_after_a_closed_block_blocked_things_stay')}</div></div>
            ${stepper('timing.between_blocks_min', { min: 1, max: 180, step: 5, unit: tr('overlay_min'), label: tr('settings_minutes_between_blocks'), ceil: locked ? t.between_blocks_min : Infinity })}</div>
        </div>
      </section>

      <section class="section">
        <h2>${tr('log_segments')}</h2>
        <div class="surface">
          ${cfg.segments.map((sg, i) => `<div class="setting seg-type">
            <span class="badge">${icon(sg.alarm ? 'alarm' : 'coffee')}</span>
            <div class="grow"><input class="seg-name" data-segname="${i}" value="${esc(sg.name)}" maxlength="24" aria-label="${tr('settings_segment_name')}"></div>
            ${stepper(`segments.${i}.minutes`, { min: 1, max: 240, step: 5, unit: tr('overlay_min'), label: `${tr('settings_minutes')}: ${sg.name}`, ceil: locked ? sg.minutes : Infinity })}
            <div class="sw-wrap prep-wrap" ${tipAttr('segPrep')}><span class="sw-label">${tr('settings_getting_ready')}</span>${stepper(`segments.${i}.prep_min`, { min: 0, max: 60, step: 5, unit: tr('overlay_min'), label: `${tr('settings_getting_ready')}: ${sg.name}`, ceil: locked ? (sg.prep_min ?? 0) : Infinity })}</div>
            <label class="sw-wrap"><span class="sw-label">${tr('settings_alarm')}</span>${sw(`segments.${i}.alarm`, sg.alarm, `${tr('settings_alarm')}: ${sg.name}`, 'segAlarm')}</label>
            <label class="sw-wrap"><span class="sw-label">${tr('log_access')}</span>${sw(`segments.${i}.open_access`, sg.open_access, `${tr('settings_access_during')}: ${sg.name}`, 'segAccess', locked && !sg.open_access)}</label>
            <button class="icon-btn interactive" data-segdel="${i}" aria-label="${tr('plan_remove')} ${esc(sg.name)}" ${cfg.segments.length <= 1 ? 'disabled' : ''}>${icon('delete')}</button>
          </div>`).join('')}
          <div class="setting seg-add"><div class="grow"><div class="d">${tr('settings_lunch_a_nap_a_walk_non')}${info('segments')}${tr('settings_your_own_type_shower_10_min')}</div></div>
            <button class="btn tonal interactive" data-segadd ${cfg.segments.length >= 10 ? 'disabled' : ''}>${icon('add')}${tr('settings_add_type')}</button></div>
        </div>
      </section>

      <section class="section">
        <h2>${tr('settings_sound_and_alerts')}</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">${tr('settings_sound')}</div><div class="d">${tr('settings_end_of_work_a_break_and')}</div></div>${sw('sound', cfg.sound, tr('settings_sound'), 'sound')}</div>
          <div class="setting"><div class="grow"><div class="t">${tr('settings_cards_on_top_of_the_screen')}</div><div class="d">${tr('settings_alarm_break_nope')}</div></div>${sw('overlay', cfg.overlay, tr('settings_cards_on_top_of_the_screen'), 'overlay')}</div>
          <div class="setting"><div class="grow"><div class="t">${tr('settings_repeat_the_ring_every')}${info('reminder')}</div></div>
            ${stepper('reminder_sec', { min: 15, max: 600, step: 15, unit: tr('settings_s'), label: tr('settings_seconds_between_reminders') })}</div>
          <div class="setting col">
            <div class="hstack"><span class="t grow">${tr('settings_test')}</span>
              <button class="btn tonal interactive" data-sound="alarm" data-tip="${tr('settings_end_of_break_ring')}">${icon('alarm')}${tr('settings_ring')}</button>
              <button class="btn tonal interactive" data-sound="break" data-tip="${tr('settings_end_of_work_signal')}">${icon('coffee')}${tr('view_break')}</button>
              <button class="btn tonal interactive" data-sound="done" data-tip="${tr('settings_block_closed')}">${icon('check')}${tr('day_block')}</button>
            </div>
            <div class="hstack"><span class="t grow">${tr('settings_show_the_card')}</span>
              <button class="btn outlined interactive" data-ov="await">${tr('settings_ring')}</button>
              <button class="btn outlined interactive" data-ov="break">${tr('view_break')}</button>
              <button class="btn outlined interactive" data-ov="block">${tr('day_block')}</button>
              <button class="btn outlined interactive" data-ov="nope">${tr('app_nope')}</button>
            </div>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>${tr('settings_appearance')}</h2>
        <div class="surface">
          <div class="setting col">
            <div class="t">${tr('settings_main_colour')}</div>
            <div class="swatches">${SWATCHES.map(([c, n]) => {
              const on = c.toLowerCase() === cfg.appearance.seed.toLowerCase();
              return `<button class="swatch" style="--c:${c};--on-c:${onColor(c)}" data-seed="${c}" aria-label="${n}" data-tip="${n}" aria-pressed="${on}">${on ? CHECK : ''}</button>`;
            }).join('')}
              <label class="swatch" style="--c:${custom ? esc(cfg.appearance.seed) : 'conic-gradient(red, yellow, lime, cyan, blue, magenta, red)'};background:var(--c);--on-c:${custom ? onColor(cfg.appearance.seed) : '#fff'}" aria-pressed="${custom}" data-tip="${tr('settings_custom_colour')}">
                ${custom ? CHECK : ''}<input type="color" id="seed-in" value="${esc(cfg.appearance.seed)}" style="position:absolute;inset:0;opacity:0;width:100%;height:100%;cursor:pointer" aria-label="${tr('settings_custom_colour')}"></label>
            </div>
          </div>
          <div class="setting"><div class="grow"><div class="t">${tr('settings_theme')}</div></div>${seg('mode', cfg.appearance.mode, [['system', tr('settings_system')], ['light', tr('settings_light')], ['dark', tr('settings_dark')]], MODE_TIP)}</div>
          <div class="setting"><div class="grow"><div class="t">${tr('settings_colourfulness')}</div></div>${seg('variant', cfg.appearance.variant, [['fidelity', tr('settings_exact')], ['tonal_spot', tr('settings_calm')], ['vibrant', tr('settings_vibrant')]], VARIANT_TIP)}</div>
          <div class="setting"><div class="grow"><div class="t">${tr('settings_contrast_mini_timer')}</div><div class="d">${tr('settings_inverted_colours_visible_on_any_background')}</div></div>${sw('appearance.mini_contrast', cfg.appearance.mini_contrast, tr('settings_contrast_mini_timer'), 'contrast')}</div>
        </div>
      </section>

      ${phoneSection(m.phone)}

      <section class="section">
        <h2>${tr('settings_mcp_for_claude')}</h2>
        <p class="section-d">${tr('settings_a_local_server_for_claude_it')}</p>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">${tr('settings_local_mcp_server')}</div>
            <div class="d">${m.mcp.running ? `${tr('settings_running_on')} 127.0.0.1:${m.mcp.port}` : m.mcp.enabled ? `${tr('settings_mcp_not_running')}${m.mcp.error ? ': ' + esc(m.mcp.error) : ''}` : tr('settings_mcp_off')}. ${tr('settings_tools')}: get_session_state, get_today_stats, get_week_stats, get_plan, set_plan, set_template, set_week_schedule, set_day_end, set_night_start, set_autolock, set_block_note, finish_block, get_blocklist, add_to_blocklist, remove_from_blocklist, start_focus_lock.</div></div>${sw('mcp_enabled', cfg.mcp_enabled, tr('settings_mcp_server'), 'mcp')}</div>
          <div class="setting col">
            <div class="t">${tr('settings_address')}</div>
            <div class="hstack"><code class="code">${esc(mcpUrl)}</code><button class="icon-btn interactive" data-copy="${esc(mcpUrl)}" aria-label="${tr('settings_copy_the_address')}">${icon('copy')}</button></div>
            <div class="t" style="margin-top:8px">${tr('settings_command_for_claude_code')}</div>
            <div class="hstack"><code class="code">${esc(cmd)}</code><button class="icon-btn interactive" data-copy="${esc(cmd)}" aria-label="${tr('settings_copy_the_command')}">${icon('copy')}</button></div>
            <div><button class="btn text interactive" id="newport" data-tip="${tr('settings_if_another_program_uses_the_port')}">${icon('refresh')}${tr('settings_new_random_port')}</button></div>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>${tr('settings_theme_system')}</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">${tr('settings_language')}${info('lang')}</div></div>${seg('lang', cfg.lang || 'auto', [['auto', tr('settings_auto')], ['ru', 'Русский'], ['en', 'English']])}</div>
          <div class="setting${locked ? ' off' : ''}"><div class="grow"><div class="t">${tr('settings_time_zone')}${info('tz')}</div></div>
            <div class="field"><select id="tz" aria-label="${tr('settings_time_zone')}" ${locked ? 'disabled' : ''}>${[...new Set([...TZ_OFFSETS, cfg.tz_offset_min])].sort((x, y) => x - y).map((o) => `<option value="${o}" ${o === cfg.tz_offset_min ? 'selected' : ''}>${utcLabel(o)}</option>`).join('')}</select></div></div>
          <div class="setting"><div class="grow"><div class="t">${tr('settings_autostart_and_self_healing')}</div><div class="d">${tr('settings_starts_with_windows_comes_back_after')}</div></div>${sw('autostart', cfg.autostart, tr('settings_autostart'), 'autostart', locked && cfg.autostart)}</div>
          <div class="setting"><div class="grow"><div class="t">${tr('settings_administrator_rights')}</div><div class="d">${m.admin ? tr('settings_yes_the_lock_works_fully') : tr('settings_no_hosts_browser_policies_and_closing')}</div></div>${icon(m.admin ? 'shield' : 'warning')}</div>
        </div>
      </section>

      <section class="section">
        <h2>${tr('settings_about_clockmanage')}</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">${tr('settings_data_and_log')}</div><div class="d ellipsis" data-tip="${esc(m.data_dir)}">${esc(m.data_dir)}</div></div><button class="btn text interactive" id="opendir">${icon('folder')}${tr('app_open')}</button></div>
          <div class="setting"><div class="grow"><div class="t">${tr('settings_version')}</div><div class="d">ClockManage ${esc(m.version)}</div></div></div>
          ${updateRow(m.update, v)}
        </div>
      </section>

      <footer class="made-by">${NIKGOB}<span>Made by NikGob</span></footer>`;
    if (pulse) {
      const el = box.querySelector(pulse);
      el?.classList.add('just');
      el?.closest('.num-step')?.querySelector('.v')?.classList.add('bump');
      pulse = null;
    }
    if (focusKey) box.querySelector(focusKey)?.focus({ preventScroll: true });
  }

  /** A selector that finds "the same" control after a re-render (keeps keyboard focus). */
  function keyOf(el) {
    if (!el || !box.contains(el)) return null;
    const d = el.dataset;
    if (d.day) return `[data-day="${d.day}"]`;
    if (d.step) return `[data-step="${d.step}"][data-d="${d.d}"]`;
    if (d.sw) return `[data-sw="${d.sw}"]`;
    if (d.seed) return `[data-seed="${d.seed}"]`;
    if (d.tpl) return `[data-tpl="${d.tpl}"]`;
    if (d.autolock) return `[data-autolock="${d.autolock}"]`;
    if (d.v && el.closest('[data-seg]')) return `[data-seg="${el.closest('[data-seg]').dataset.seg}"] [data-v="${d.v}"]`;
    return el.id ? `#${el.id}` : null;
  }

  // Saves run one after another, each on top of the result of the previous one, so two quick
  // changes never overwrite each other.
  let queue = Promise.resolve();
  function save(mut, okText) {
    const job = queue.then(async () => {
      const next = structuredClone(cfg);
      mut(next);
      const saved = await run(() => call('save_config', { cfg: next }));
      if (saved) { cfg = saved; if (okText) snack(okText); }
      render();
      return !!saved;
    });
    queue = job.catch(() => {});
    return job;
  }

  // Theme changes spread as a circle from the pressed control (View Transitions API).
  function reveal(btn, appearance) {
    const apply = () => applyTheme(appearance);
    const r = btn.getBoundingClientRect();
    const x = r.left + r.width / 2;
    const y = r.top + r.height / 2;
    if (document.startViewTransition && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
      const radius = Math.hypot(Math.max(x, innerWidth - x), Math.max(y, innerHeight - y));
      const t = document.startViewTransition(apply);
      t.ready.then(() => document.documentElement.animate(
        { clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${radius}px at ${x}px ${y}px)`] },
        { duration: 900, easing: 'cubic-bezier(.2, 0, 0, 1)', pseudoElement: '::view-transition-new(root)' },
      )).catch(() => {});
    } else apply();
    save((c) => { c.appearance = appearance; });
  }

  const setPath = (obj, path, val) => {
    const ks = path.split('.');
    let o = obj;
    for (let i = 0; i < ks.length - 1; i++) o = o[ks[i]];
    o[ks.at(-1)] = val;
  };

  async function pickKind(tile) {
    const i = Number(tile.dataset.day);
    const cur = cfg.week[i];
    const kind = await menu(tile, KINDS.map((k) => ({
      value: k.id, label: k.label, icon: icon(k.icon), selected: k.id === cur,
      sub: `${planSummary(cfg.profiles[k.id].plan)}${cfg.profiles[k.id].block ? '' : tr('log_no_lock')}`,
    })), { title: WEEKDAYS_FULL[i][0].toUpperCase() + WEEKDAYS_FULL[i].slice(1) });
    if (!kind || kind === cur) return;
    pulse = `[data-day="${i}"]`;
    const ok = await save((c) => { c.week[i] = kind; });
    // During the lock (the autolock too) today only takes a stricter kind; a lighter one waits for next week.
    const rank = (k) => KINDS.findIndex((x) => x.id === k);
    const follows = last.view.can.lighter_kind || rank(kind) < rank(last.view.kind);
    if (ok && i === last.view.weekday && !last.view.started && follows) snack(tr('settings_today_is_a_day_plan_updated', { kind: kindLabel(kind).toLowerCase() }));
  }

  // "HH:MM" of a time field in minutes. A cleared field (Backspace) gives null, not 00:00.
  const minOf = (value) => {
    const [h, mm] = value.split(':').map(Number);
    return value && Number.isFinite(h) ? h * 60 + (mm || 0) : null;
  };

  box.addEventListener('change', (e) => {
    const t = e.target;
    if (t.dataset.sw) {
      pulse = `[data-sw="${t.dataset.sw}"]`;
      save((c) => setPath(c, t.dataset.sw, t.checked));
    } else if (t.dataset.segname !== undefined) {
      const i = Number(t.dataset.segname);
      const name = t.value.trim();
      // Two types with one name would merge into one (and the other would vanish).
      if (cfg.segments.some((x, j) => j !== i && x.name.toLowerCase() === name.toLowerCase())) {
        snack(tr('settings_already_exists_pick_another_name', { name }));
        render();
      } else if (name) save((c) => { c.segments[i].name = name; });
      else render();
    } else if (t.id === 'dayend') {
      const m = minOf(t.value);
      if (m === null) render();
      else save((c) => { c.day_end_min = m; });
    } else if (t.dataset.autolock) {
      const id = t.dataset.autolock;
      const m = minOf(t.value);
      if (m === null) render();
      else save((c) => { c.profiles[id].autolock_min = m; });
    } else if (t.id === 'nightuntil') {
      const m = minOf(t.value);
      // The backend caps the night at 12:00.
      if (m === null) render();
      else save((c) => { c.night_start.until_min = Math.min(720, m); });
    } else if (t.id === 'seed-in') {
      save((c) => { c.appearance.seed = t.value; });
    } else if (t.id === 'tz') {
      save((c) => { c.tz_offset_min = Number(t.value); });
    }
  });
  box.addEventListener('input', (e) => {
    if (e.target.id === 'seed-in') applyTheme({ ...cfg.appearance, seed: e.target.value });
  });
  box.addEventListener('click', async (e) => {
    const t = e.target.closest('button');
    if (!t) return;
    if (t.dataset.day !== undefined) pickKind(t);
    else if (t.dataset.step) {
      const path = t.dataset.step;
      pulse = `[data-step="${path}"][data-d="${t.dataset.d}"]`;
      const { d, sz, lo, hi } = t.dataset;
      // Step from the value the save actually applies to (several quick clicks add up).
      save((c) => {
        const v = path.split('.').reduce((o, k) => o[k], c);
        setPath(c, path, Math.min(Number(hi), Math.max(Number(lo), snap(v, Number(d), Number(sz)))));
      });
    } else if (t.dataset.tpl) {
      ctx.planTab = t.dataset.tpl;
      ctx.navigate('plan');
    } else if (t.dataset.seed) {
      pulse = `[data-seed="${t.dataset.seed}"]`;
      reveal(t, { ...cfg.appearance, seed: t.dataset.seed });
    } else if (t.closest('[data-seg]')) {
      const name = t.closest('[data-seg]').dataset.seg;
      pulse = `[data-seg="${name}"] [data-v="${t.dataset.v}"]`;
      if (['mode', 'variant'].includes(name)) reveal(t, { ...cfg.appearance, [name]: t.dataset.v });
      // The new language arrives with the state; every window reloads into it.
      else if (name === 'lang') save((c) => { c.lang = t.dataset.v; });
    } else if (t.dataset.sound) call('test_sound', { kind: t.dataset.sound });
    else if (t.dataset.ov) call('preview_overlay', { kind: t.dataset.ov });
    else if (t.dataset.copy) {
      try { await navigator.clipboard.writeText(t.dataset.copy); snack(tr('settings_copied')); } catch { snack(tr('log_couldnt_copy')); }
    } else if (t.id === 'newport') {
      const port = await run(() => call('regenerate_port'), t);
      if (port) { cfg = await call('get_config'); snack(tr('settings_new_port_update_the_address_in', { port })); }
    } else if (t.id === 'opendir') call('open_data_dir');
    else if (t.id === 'checkupdate') call('check_update');
    else if (t.id === 'installupdate') await run(() => call('install_update'), t);
    else if (t.id === 'phone-pin') await run(() => call('phone_pin'), t);
    else if (t.dataset.segdel !== undefined) {
      const i = Number(t.dataset.segdel);
      save((c) => { c.segments.splice(i, 1); });
    } else if (t.dataset.segadd !== undefined) {
      save((c) => {
        const base = tr('settings_custom');
        let name = base;
        for (let n = 2; c.segments.some((x) => x.name.toLowerCase() === name.toLowerCase()); n++) name = `${base} ${n}`;
        c.segments.push({ name, minutes: 15, alarm: false, open_access: false, prep_min: 0 });
      }).then(() => box.querySelector('.seg-type:last-of-type .seg-name')?.select());
    }
    else if (t.dataset.forget) {
      const ok = await run(() => call('phone_forget', { id: t.dataset.forget }).then(() => true), t);
      if (ok) snack(tr('settings_phone_unpaired_pair_again_with_a'));
    }
  });

  async function reload() {
    cfg = await call('get_config');
    render();
  }

  let sig = '';
  return {
    update(s) {
      last = s;
      const v = s.view;
      // Phone "last seen" re-renders once a minute, not on every poll of the phone.
      const ph = { ...s.meta.phone, devices: s.meta.phone.devices.map((d) => ({ ...d, last_seen: d.last_seen && Math.floor((Date.now() - d.last_seen) / 60000) })) };
      const nsig = `${v.lock.base}|${v.started}|${v.day_end}|${v.kind}|${JSON.stringify(s.meta.mcp)}|${s.meta.admin}|${JSON.stringify(ph)}|${JSON.stringify(s.meta.update)}|${v.phase.kind}|${v.lock.blocked}`;
      if (nsig !== sig) { sig = nsig; if (cfg) render(); }
    },
    show() { reload(); },
    config() { reload(); },
  };
}

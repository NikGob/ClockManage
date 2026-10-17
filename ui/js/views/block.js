import { call, esc, mmss } from '../api.js';
import { icon } from '../icons.js';
import { run, snack, tsHm } from '../ui.js';
import { emergencyDialog } from './dialogs.js';
import { tr } from '../i18n.js';

export function mountBlock(root, ctx) {
  let cfg = null;
  let last = null;
  let cfgSig = '';

  root.innerHTML = `
    <div class="readable">
      <section class="lock-hero" id="hero"><div class="lockart" id="lockart"></div><div id="hero-text" aria-live="polite"></div></section>

      <section class="section">
        <h2>${tr('blocking_sites')}</h2>
        <div class="surface pad stack">
          <div class="chips" id="sites"></div>
          <div class="add-row">
            <div class="field"><label for="site-in">${tr('blocking_add_site')}</label><input id="site-in" placeholder="${tr('blocking_reddit_com_or_youtube_com_shorts')}" autocomplete="off"></div>
            <button class="btn tonal interactive" id="site-add">${icon('add')}${tr('blocking_add')}</button>
          </div>
          <p class="note-text">${tr('blocking_a_path_like')} <b>youtube.com/shorts</b> ${tr('blocking_closes_only_that_section_the_rest')}</p>
          <div class="ff-line">
            <label for="ff" class="body-l grow">${tr('blocking_restart_firefox_when_the_lock_starts')}</label>
            <button class="icon-btn interactive" id="ff-now" aria-label="${tr('blocking_restart_firefox_now')}" title="${tr('blocking_restart_firefox_now')}">${icon('refresh')}</button>
            <input type="checkbox" class="switch" role="switch" id="ff">
          </div>
        </div>
      </section>

      <section class="section">
        <h2>${tr('blocking_apps')}</h2>
        <div class="surface pad stack">
          <div class="chips" id="apps"></div>
          <div class="add-row">
            <div class="field"><label for="app-in">${tr('blocking_add_an_app_process_name')}</label><input id="app-in" placeholder="Steam.exe" autocomplete="off"></div>
            <button class="btn tonal interactive" id="app-add">${icon('add')}${tr('blocking_add')}</button>
          </div>
          <p class="note-text">${tr('blocking_if_an_app_is_running_during')}</p>
        </div>
      </section>

      <section class="section">
        <h2>${tr('blocking_phone_apps')}</h2>
        <div class="surface pad stack">
          <div class="chips" id="phone-apps"></div>
          <div class="add-row">
            <div class="field"><label for="papp-in">${tr('blocking_add_android_package_name')}</label><input id="papp-in" placeholder="com.zhiliaoapp.musically" autocomplete="off"></div>
            <button class="btn tonal interactive" id="papp-add">${icon('add')}${tr('blocking_add')}</button>
          </div>
          <p class="note-text">${tr('blocking_its_easier_to_pick_from_the')}</p>
        </div>
      </section>

      <section class="section">
        <h2>${tr('blocking_access_in_breaks')}</h2>
        <div class="surface">
          <div class="setting">
            <div class="grow"><div class="t">${tr('blocking_open_blocked_things_in_the_break')}</div>
              <div class="d" id="ba-d"></div></div>
            <input type="checkbox" class="switch" role="switch" id="ba" aria-describedby="ba-d">
          </div>
        </div>
      </section>

      <section class="section">
        <h2>${tr('blocking_emergency_access')}</h2>
        <div class="surface">
          <div class="setting">
            <div class="grow"><div class="t">${tr('blocking_open_everything_for')} <span id="em-min">10</span> ${tr('blocking_minutes')}</div>
              <div class="d" id="em-d">${tr('blocking_you_have_to_type_a_long')}</div></div>
            <button class="btn outlined danger interactive" id="em">${icon('warning')}${tr('blocking_emergency')}</button>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>${tr('blocking_good_to_know')}</h2>
        <ul class="limits body-m">
          <li>${tr('blocking_the_app_runs_as_administrator_it')}</li>
          <li>${tr('blocking_chrome_edge_brave_and_yandex_pick')}</li>
          <li>${tr('blocking_shorts_opened_by_a_click_inside')}</li>
        </ul>
      </section>
    </div>`;

  const $ = (id) => root.querySelector('#' + id);

  function chips(list, kind, removable) {
    return list.map((s, i) => `<span class="chip input${removable ? ' removable' : ''}">${esc(s)}${removable ? `<button class="x interactive" data-rm="${kind}" data-i="${i}" aria-label="${tr('blocking_remove')} ${esc(s)}">${icon('close')}</button>` : ''}</span>`).join('')
      || `<span class="body-m muted">${tr('blocking_empty')}</span>`;
  }

  function renderCfg() {
    if (!cfg || !last) return;
    const locked = last.view.lock.base && last.view.lock.reason !== 'completed';
    $('sites').innerHTML = chips(cfg.blocklist.sites, 'sites', !locked);
    $('apps').innerHTML = chips(cfg.blocklist.apps, 'apps', !locked);
    $('phone-apps').innerHTML = chips(cfg.phone.apps, 'phone', !locked);
    // Off makes the lock stricter, so it works any time; on only outside the lock.
    const ba = $('ba');
    ba.checked = cfg.break_access;
    ba.disabled = !cfg.break_access && !last.view.can.edit_break_access;
    $('ba-d').textContent = `${tr('blocking_while_the_break_timer_runs_breaks')}${ba.disabled ? ` ${tr('blocking_turns_on_before_the_study_day', { day_end: last.view.day_end })}` : ''}`;
    $('ff').checked = cfg.restart_firefox;
    $('em-min').textContent = cfg.emergency_min;
  }

  function renderHero() {
    const v = last.view;
    const lk = v.lock;
    let cls = '', ic = 'lock_open', h = '', p = '';
    if (lk.blocked) {
      cls = 'on'; ic = 'lock';
      h = tr('app_tray_lock_on');
      const focusEnd = lk.focus_until ? tsHm(lk.focus_until, last.meta.tz_offset_min) : '';
      p = lk.reason === 'single' ? tr('blocking_for_the_single_timer')
        : lk.reason === 'focus' ? tr('blocking_focus_lock_until_only_emergency_access', { focus_end: focusEnd })
          : tr('blocking_lifts_when_all_blocks_are_done', { day_end: v.day_end });
      if (focusEnd && lk.reason !== 'focus') p += tr('blocking_focus_lock_until', { focus_end: focusEnd });
    } else if (lk.base) {
      cls = 'open';
      h = { break_access: tr('blocking_access_in_the_break'), emergency: tr('blocking_emergency_access'), lunch_at_pc: tr('view_lunch_at_the_pc') }[lk.reason] || tr('blocking_access_open');
      p = lk.until ? tr('blocking_the_lock_comes_back_in', { lk: mmss(lk.until - v.now) }) : '';
    } else {
      h = tr('blocking_no_lock_right_now');
      p = { not_started: v.study_day ? (v.after_day_end ? tr('blocking_no_lock_after', { day_end: v.day_end }) : tr('blocking_turns_on_with_start_day')) : (v.kind === 'off' ? tr('blocking_today_is_a_day_off_no') : tr('blocking_the_lock_is_off_for_this')), completed: tr('blocking_all_blocks_of_the_day_are'), day_end: tr('blocking_no_lock_after', { day_end: v.day_end }), not_study_day: v.kind === 'off' ? tr('blocking_today_is_a_day_off_no') : tr('blocking_the_lock_is_off_for_this') }[lk.reason] || '';
    }
    if (!last.meta.admin) p += tr('blocking_warning_the_app_runs_without_administrator');
    if (last.meta.blocker_error) p += ` ${tr('blocking_error')}: ${last.meta.blocker_error}`;
    const html = `<h2>${esc(h)}</h2><p class="body-l">${esc(p)}</p>${v.emergency_count ? `<p class="body-m">${tr('blocking_emergency_accesses_today')} ${v.emergency_count}</p>` : ''}`;
    const hero = $('hero');
    hero.className = `lock-hero ${cls}`;
    const art = $('lockart');
    if (!art.firstElementChild) {
      art.innerHTML = `<svg class="padlock" viewBox="0 0 48 48" width="44" height="44" aria-hidden="true">
        <path class="shackle" d="M16 22 V15.5 a8 8 0 0 1 16 0 V22"/>
        <g class="body"><rect x="9" y="21" width="30" height="21" rx="7"/>
        <circle class="hole" cx="24" cy="30.5" r="3"/><rect class="hole" x="22.5" y="31" width="3" height="5.5" rx="1.5"/></g>
      </svg>`;
    }
    const wasOn = hero.dataset.on === 'true';
    hero.dataset.on = String(lk.blocked);
    if (hero.dataset.seen && lk.blocked && !wasOn) {
      art.classList.remove('click');
      void art.offsetWidth;
      art.classList.add('click');
    }
    hero.dataset.seen = '1';
    const txt = $('hero-text');
    if (txt.dataset.html !== html) {
      if (txt.dataset.html) txt.animate([{ opacity: 0, transform: 'translateX(-8px)' }, { opacity: 1, transform: 'none' }], { duration: 320, easing: 'cubic-bezier(.05,.7,.1,1)' });
      txt.innerHTML = html;
      txt.dataset.html = html;
    }
    $('em').disabled = !v.can.emergency;
    $('em-d').textContent = v.can.emergency
      ? tr('blocking_you_have_to_type_a_long')
      : lk.base ? tr('blocking_access_is_already_open') : tr('blocking_available_only_while_the_lock_is');
  }

  async function save(mut, okText) {
    const next = structuredClone(cfg);
    mut(next);
    const saved = await run(() => call('save_config', { cfg: next }));
    if (saved) {
      cfg = saved;
      if (okText) snack(okText);
    }
    renderCfg();
    return saved;
  }

  const listOf = (c, kind) => (kind === 'phone' ? c.phone.apps : c.blocklist[kind]);

  function addFrom(inputId, kind) {
    const inp = $(inputId);
    const val = inp.value.trim();
    if (!val) return;
    const before = listOf(cfg, kind).length;
    save((c) => listOf(c, kind).push(val)).then((saved) => {
      if (!saved) return; // refused: the text stays for a fix
      // The backend drops what isn't a site or an app name, a duplicate and system apps.
      if (listOf(saved, kind).length > before) snack(`${tr('blocking_added')}: ${listOf(saved, kind).at(-1)}`);
      else snack(tr('blocking_not_added_not_a_site_or', { val }));
      inp.value = '';
      inp.focus();
    });
  }

  $('site-add').addEventListener('click', () => addFrom('site-in', 'sites'));
  $('app-add').addEventListener('click', () => addFrom('app-in', 'apps'));
  $('site-in').addEventListener('keydown', (e) => { if (e.key === 'Enter') addFrom('site-in', 'sites'); });
  $('app-in').addEventListener('keydown', (e) => { if (e.key === 'Enter') addFrom('app-in', 'apps'); });
  $('papp-add').addEventListener('click', () => addFrom('papp-in', 'phone'));
  $('papp-in').addEventListener('keydown', (e) => { if (e.key === 'Enter') addFrom('papp-in', 'phone'); });
  root.addEventListener('click', (e) => {
    const b = e.target.closest('[data-rm]');
    if (!b) return;
    const kind = b.dataset.rm;
    const i = Number(b.dataset.i);
    const name = listOf(cfg, kind)[i];
    save((c) => listOf(c, kind).splice(i, 1), `${tr('blocking_removed')}: ${name}`);
  });
  $('ff-now').addEventListener('click', async (e) => {
    const b = e.currentTarget;
    if (b.classList.contains('spin')) return;
    b.classList.add('spin');
    const ok = await run(() => call('restart_firefox').then(() => true));
    b.classList.remove('spin');
    if (ok) snack(tr('blocking_firefox_restarted_your_tabs_will_be'));
  });
  $('ff').addEventListener('change', (e) => save((c) => { c.restart_firefox = e.target.checked; }));
  $('ba').addEventListener('change', (e) => save((c) => { c.break_access = e.target.checked; }));
  $('em').addEventListener('click', async () => {
    const ok = await emergencyDialog(cfg, last.view.emergency_count);
    if (ok) snack(tr('blocking_emergency_access_for_min_logged', { emergency_min: cfg.emergency_min }));
  });

  async function reload() {
    cfg = await call('get_config');
    renderCfg();
  }

  return {
    update(s) {
      last = s;
      renderHero();
      // Re-render editable parts only when the lock rules change (keeps focus while typing).
      const sig = `${s.view.lock.base}|${s.view.lock.reason}|${s.view.can.edit_break_access}`;
      if (sig !== cfgSig) { cfgSig = sig; renderCfg(); }
    },
    show() { reload(); },
    config() { reload(); },
  };
}

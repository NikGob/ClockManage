import { call, mmss, dur, esc, LINUX } from '../api.js';
import { icon, morphIcon } from '../icons.js';
import { doodle, play } from '../doodles.js';
import { WaveRing } from '../wave.js';
import { run, snack, menu, burst, KINDS, KIND_RANK, kindLabel, planSummary, hm, tsHm } from '../ui.js';
import { tr } from '../i18n.js';
import { singleDialog, emergencyDialog, dayEndDialog, finishDialog, segmentDialog, nightStartDialog } from './dialogs.js';

export function endLabel(v) {
  return v.day_end + (v.day_end_next_day ? NIGHT() : '');
}

/** " ночи": a time after midnight that still belongs to today. */
const NIGHT = () => tr('settings_night');

/** Minutes after the day's midnight (may run past 24:00) -> "01:00". */
const dayHm = (m) => hm(m % 1440);

export function mountToday(root, ctx) {
  root.innerHTML = `
  <div class="today">
    <section class="hero" id="hero" aria-label="${tr('today_timer')}">
      <div class="lockline" id="lockline"></div>
      <div class="dial">
        <svg id="ring" aria-hidden="true"></svg>
        <div class="art" id="art" hidden></div>
        <div class="center" id="center">
          <div class="state" id="dstate"></div>
          <div class="big tnum" id="big" role="timer" aria-live="off"></div>
        </div>
      </div>
      <div class="phase-title">
        <h2 id="ptitle"></h2>
        <p id="psub"></p>
      </div>
      <div class="controls" id="controls"></div>
    </section>
    <aside class="side">
      <section class="surface access" id="access" hidden aria-live="polite"></section>
      <section class="surface note-card" id="note-card" hidden></section>
      <section class="surface plan-card" aria-labelledby="plan-h">
        <div class="head">
          <div class="row1"><h2 id="plan-h">${tr('main_day_plan')}</h2><button class="kind-chip interactive" id="kind" data-act="kind" aria-haspopup="menu"></button><button class="btn text interactive" id="edit-plan">${tr('today_edit')}</button></div>
          <div class="total tnum" id="total"></div>
          <div class="linear" id="total-bar"></div>
          <div class="dayend" id="dayend"></div>
        </div>
        <ol class="list" id="blocks"></ol>
      </section>
    </aside>
  </div>`;

  const $ = (id) => root.querySelector('#' + id);
  const ring = new WaveRing($('ring'));
  let ctrlSig = '';
  let artKind = '';
  let accessSig = '';
  let prevBig = '';
  let last = null;
  let prevDone = [];
  let shieldUntil = 0;
  const SHIELD_MS = 900;
  const SHIELD_REPEAT_MS = 700;

  $('edit-plan').addEventListener('click', () => ctx.navigate('plan'));
  ctx.setActions(`<button class="btn tonal interactive" data-top="mini">${icon('pip')}${tr('app_mini_timer')}</button>`);
  ctx.onAction = (e) => { if (e.target.closest('[data-top="mini"]')) call('toggle_mini'); };

  function setBig(text, small = false) {
    const el = $('big');
    el.style.fontSize = small ? '44px' : '';
    el.style.lineHeight = small ? '52px' : '';
    if (text === prevBig) return;
    if (prevBig.length === text.length && /^\d/.test(text)) {
      el.innerHTML = [...text].map((c, i) => `<span class="${c !== prevBig[i] ? 'tick' : ''}">${esc(c)}</span>`).join('');
    } else {
      el.textContent = text;
    }
    prevBig = text;
  }

  function setArt(kind) {
    if (kind === artKind) return;
    artKind = kind;
    const art = $('art');
    if (!kind) {
      art.hidden = true;
      art.innerHTML = '';
      $('center').hidden = false;
      return;
    }
    art.hidden = false;
    $('center').hidden = true;
    art.innerHTML = doodle(kind, 220);
    art.className = `art ${{ await: 'ringing', lunch: 'steaming', idle: 'writing', rest: 'resting' }[kind] || ''}`;
    play(art);
  }

  function lockLine(v, meta) {
    const lk = v.lock;
    let ic = 'lock_open', text = '', when = '', cls = '', action = '';
    const left = lk.until ? mmss(lk.until - v.now) : '';
    // The day end is a quiet button: click → move it for today (later, or earlier via "другое время").
    const endBtn = (label) => (v.can.set_day_end
      ? `<button class="when-btn interactive" data-act="dayend" aria-haspopup="menu" data-tip="${tr('today_todays_day_end_later_or_earlier')}">${esc(label)}${icon('expand')}</button>`
      : esc(label));
    const offText = v.kind === 'off' ? tr('today_day_off_no_lock') : tr('today_day_without_a_lock', { kind: kindLabel(v.kind) });
    switch (lk.reason) {
      case 'study': ic = 'lock'; text = tr('app_tray_lock_on'); when = `${endBtn(`${tr('today_until')} ${endLabel(v)}`)} ${tr('today_or_until_the_plan_is_done')}`; break;
      case 'single': ic = 'lock'; text = tr('today_locked_for_the_timer'); break;
      case 'focus': ic = 'lock'; text = tr('app_focus_lock'); when = `${tr('today_until')} ${tsHm(lk.focus_until, meta.tz_offset_min)}`; break;
      case 'break_access': text = tr('today_break_between_blocks_access_open'); when = left; cls = 'open'; break;
      case 'emergency': text = tr('blocking_emergency_access'); when = left; cls = 'open'; action = 'end'; break;
      case 'lunch_at_pc': text = tr('today_lunch_at_the_pc_access_open'); when = left; cls = 'open'; break;
      case 'segment_access': text = `${v.phase.title} — ${tr('today_access_open')}`; when = left; cls = 'open'; break;
      case 'not_started':
        // Started after the day end, a day runs without the lock.
        if (v.study_day && v.after_day_end) text = tr('today_no_lock_after', { v: endLabel(v) });
        else text = v.study_day ? tr('today_the_lock_turns_on_with_start') : offText;
        if (v.study_day && !v.after_day_end) when = `${tr('today_and_holds')} ${endBtn(`${tr('today_until')} ${endLabel(v)}`)}`;
        break;
      case 'completed': text = tr('today_all_blocks_done_the_lock_is'); break;
      case 'day_end':
        text = tr('today_no_lock_after', { v: endLabel(v) });
        if (v.started && v.can.extend_day_end) when = endBtn(tr('today_bring_the_lock_back'));
        break;
      case 'not_study_day': text = offText; break;
      default: text = '';
    }
    if (lk.focus_until && (lk.reason === 'study' || lk.reason === 'single')) when += ` · ${tr('today_focus_until')} ${tsHm(lk.focus_until, meta.tz_offset_min)}`;
    if (lk.base && !['emergency', 'break_access'].includes(lk.reason) && v.can.emergency) action = 'emergency';
    if (!meta.admin && lk.base) { text += LINUX ? tr('today_sites_are_not_blocked') : tr('today_no_administrator_rights'); cls = 'open'; }
    const btn = action === 'end'
      ? `<button class="btn text interactive" data-act="end_access">${tr('today_close_access')}</button>`
      : action === 'emergency' ? `<button class="sos interactive" data-act="emergency" aria-label="${tr('blocking_emergency_access')}">${icon('warning')}<span class="lbl">${tr('today_emergency')}</span></button>` : '';
    const el = $('lockline');
    el.className = `lockline ${cls}`;
    // `when` is HTML: every dynamic piece in it is escaped or numeric.
    const html = `${icon(ic)}<span class="grow">${esc(text)} <span class="when tnum">${when}</span></span>${btn}`;
    if (el.dataset.html !== html) {
      const had = !!el.dataset.html;
      el.innerHTML = html;
      el.dataset.html = html;
      if (had) el.querySelector('.icon')?.classList.add('pop-in');
    }
  }

  function kindChip(v) {
    const k = KINDS.find((x) => x.id === v.kind) || KINDS[0];
    const html = `${icon(k.icon)}<span>${k.label}</span>${icon('expand', 'caret')}`;
    const b = $('kind');
    if (b.dataset.html !== html) {
      const had = !!b.dataset.html;
      b.innerHTML = html;
      b.dataset.html = html;
      b.dataset.kind = k.id;
      if (had) b.classList.add('just');
    }
    b.dataset.tip = v.can.lighter_kind ? tr('today_todays_day_type_its_own_plan') : tr('today_during_study_the_day_can_only');
  }

  async function pickKind(b) {
    const v = last.view;
    const cfg = await call('get_config');
    const cur = v.kind;
    const kind = await menu(b, KINDS.map((k) => {
      const lighter = KIND_RANK[k.id] < KIND_RANK[cur];
      const p = cfg.profiles[k.id];
      return {
        value: k.id, label: k.label, icon: icon(k.icon), selected: k.id === cur,
        sub: `${planSummary(p.plan)}${p.block ? '' : tr('log_no_lock')}`,
        disabled: lighter && !v.can.lighter_kind,
        tip: lighter && !v.can.lighter_kind ? tr('today_during_study_only_denser') : '',
      };
    }), {
      title: tr('today_day_type'),
      note: v.completed ? tr('today_the_day_is_closed_a_denser')
        : v.started ? tr('today_the_day_is_running_the_new') : tr('today_todays_plan_is_replaced_with_this'),
    });
    if (!kind || kind === cur) return;
    const ok = await run(() => call('set_day_kind', { kind }).then(() => true), b);
    if (ok) snack(tr('today_today_is_a_day', { kind: kindLabel(kind).toLowerCase() }));
  }

  async function extendDayEnd(b) {
    const v = last.view;
    const MAX = 26 * 60; // 02:00 of the night after
    const opts = [];
    if (v.can.extend_day_end) {
      const base = Math.max(v.day_end_min, v.now_min);
      for (const add of [30, 60, 120]) {
        const m = Math.ceil((base + add) / 5) * 5;
        if (m <= MAX && m > v.day_end_min && !opts.includes(m)) opts.push(m);
      }
    }
    const plus = (d) => `+${d >= 60 ? `${Math.floor(d / 60)} ${tr('ui_h')} ` : ''}${d % 60 ? `${d % 60} ${tr('overlay_min')}` : ''}`.trim();
    const items = opts.map((m) => ({ value: m, label: `${tr('today_until')} ${dayHm(m)}${m >= 1440 ? NIGHT() : ''}`, icon: icon('schedule'), sub: plus(m - v.day_end_min) }));
    items.push({ value: 'custom', label: tr('today_other_time'), icon: icon('edit'), sub: tr('today_day_end_other_hint') });
    const choice = await menu(b, items, {
      title: v.after_day_end ? tr('today_bring_the_lock_back_until') : tr('today_todays_day_end'),
      note: tr('today_today_only_the_setting_stays_every', { day_end_base: v.day_end_base }),
    });
    if (!choice) return;
    if (choice === 'custom') {
      const r = await dayEndDialog(v);
      if (r) snack(r.changed ? tr('today_todays_day_end_settings_untouched', { new: r.new, r: r.new_is_next_day ? NIGHT() : '' }) : tr('today_the_day_end_didnt_change'));
      return;
    }
    const ok = await run(() => call('extend_day_end', { minutes: choice }).then(() => true), b);
    if (ok) snack(tr('today_studying_today_until', { choice: dayHm(choice) }) + (choice >= 1440 ? NIGHT() : ''));
  }

  function controls(v) {
    const p = v.phase;
    const c = v.can;
    const items = [];
    const B = (act, label, cls, ic) => items.push({ act, label, cls, ic });
    switch (p.kind) {
      case 'idle':
        if (c.start_day) B('start_day', tr('app_start_day'), 'filled xl', 'play');
        if (c.single) B('single', tr('dialog_single_timer'), c.start_day ? 'tonal lg' : 'filled lg', 'timer');
        break;
      case 'work':
        if (c.resume) B('resume', tr('app_resume'), 'filled xl', 'play');
        else if (c.pause) B('pause', tr('day_pause'), 'filled xl', 'pause');
        break;
      case 'break':
        if (c.resume) B('resume', tr('today_resume_the_break'), 'filled lg', 'play');
        else if (c.pause) B('pause', tr('day_pause'), 'tonal lg', 'pause');
        B('start_next', tr('today_start_now'), c.resume ? 'tonal lg' : 'filled lg', 'skip');
        if (c.segment) B('segment', tr('config_segment'), 'outlined lg', 'restaurant');
        break;
      case 'lunch_break':
        B('start_next', tr('today_end_lunch_early'), 'tonal lg', 'skip');
        break;
      case 'await':
        B('start_next', startLabel(p), 'filled xl', 'play');
        if (c.segment) B('segment', tr('config_segment'), 'tonal lg', 'restaurant');
        break;
      case 'segment':
        if (c.lay_down) {
          // Getting ready: the countdown and the alarm start only from "Лёг".
          B('lay_down', tr('app_in_bed'), 'filled xl', 'bed');
          B('end_segment', p.alarm ? tr('today_skip_the_nap') : tr('today_cancel'), 'text lg', 'close');
        } else {
          B('end_segment', p.alarm ? tr('app_im_up') : tr('mini_done_with', { p: p.title.toLowerCase() }), 'filled xl', p.alarm ? 'alarm' : 'check');
        }
        if (c.segment) B('segment', tr('dialog_queue'), 'outlined lg', 'add');
        if (c.segment_mode) B('seg_mode', p.stopwatch ? tr('dialog_timed') : tr('dialog_untimed'), 'text lg', p.stopwatch ? 'schedule' : 'timer');
        break;
      case 'lunch':
        B('start_next', tr('mini_done_eating_start'), 'filled xl', 'play');
        break;
      case 'done':
        if (c.single) B('single', tr('dialog_single_timer'), 'outlined lg', 'timer');
        break;
    }
    // Quiet on purpose: a text button after the main ones, behind a hold-to-confirm dialog.
    if (c.finish_block) B('finish', tr('today_close_block'), 'text lg', 'check');
    if (c.stop_single) B('stop_single', tr('today_stop'), 'outlined lg', 'stop');
    const sig = items.map((i) => i.act + i.label + i.cls).join('|');
    if (sig === ctrlSig) return;
    // The buttons just changed under the cursor: a click that was meant for the old button
    // (spamming "Обед", say) must not land on the new one ("Закончить обед").
    if (ctrlSig) shieldUntil = performance.now() + SHIELD_MS;
    ctrlSig = sig;
    const el = $('controls');
    // Keyed update: pause/resume is one control whose icon morphs; new buttons spring in.
    // A different action is a different element (it springs in), never a relabelled old one.
    const keyOf = (act, label) => (act === 'pause' || act === 'resume' ? 'toggle' : `${act}:${label}`);
    const old = new Map([...el.querySelectorAll('[data-key]')].map((b) => [b.dataset.key, b]));
    const next = items.map((i) => {
      const key = keyOf(i.act, i.label);
      let b = old.get(key);
      old.delete(key);
      const fresh = !b;
      if (fresh) {
        b = document.createElement('button');
        b.dataset.key = key;
        b.innerHTML = `${icon(i.ic)}<span class="lbl"></span>`;
        b.classList.add('enter');
      } else if (!morphIcon(b, i.ic)) {
        b.querySelector('svg')?.remove();
        b.insertAdjacentHTML('afterbegin', icon(i.ic));
      }
      b.className = `btn ${i.cls} interactive${fresh ? ' enter' : ''}`;
      b.dataset.act = i.act;
      const lbl = b.querySelector('.lbl');
      if (lbl.textContent !== i.label) {
        if (!fresh) lbl.animate([{ opacity: 0, transform: 'translateY(6px)' }, { opacity: 1, transform: 'none' }], { duration: 260, easing: 'cubic-bezier(.05,.7,.1,1)' });
        lbl.textContent = i.label;
      }
      return b;
    });
    old.forEach((b) => b.remove());
    next.forEach((b) => el.appendChild(b));
  }

  function startLabel(p) {
    const m = p.subtitle.match(/(?:часть|part) (\d+) (?:из|of) (\d+)/i);
    if (!m) return tr('overlay_start');
    return m[1] === '1' ? tr('today_start', { p: p.subtitle.split(' · ')[0] }) : tr('today_start_part', { m: m[1] });
  }

  function access(v, meta) {
    const el = $('access');
    const p = v.pause;
    let html = '';
    if (p && v.lock.base) {
      html = `<div class="row1">${icon('pause')}<div class="grow"><div class="title-m">${tr('mini_paused')} ${dur(p.paused_ms)}</div>
        <div class="body-m muted">${meta.break_access && !v.lock.focus_until ? tr('today_blocked_things_stay_closed_on_a') : tr('today_blocked_things_stay_closed_during_a')}</div></div></div>`;
    }
    // Only rebuild when the structure changes; the countdown text updates in place.
    const sig = html.replace(/\d+:\d+|\d+ (ч|мин|h|min)/g, '#');
    el.hidden = !html;
    el.classList.remove('warn');
    if (sig !== accessSig) { el.innerHTML = html; accessSig = sig; }
    else {
      // Same structure: refresh every live text in place (title "Пауза 12 мин", note, countdown).
      const tmp = document.createElement('div');
      tmp.innerHTML = html;
      for (const sel of ['.title-m', '.body-m', '.count']) {
        const cur = el.querySelector(sel);
        const next = tmp.querySelector(sel);
        if (cur && next && cur.textContent !== next.textContent) cur.textContent = next.textContent;
      }
    }
  }

  // Today's day end: one-off shift, and whether the rest of the plan fits before it.
  function dayEnd(v) {
    const el = $('dayend');
    const f = v.forecast;
    let fit = '';
    if (v.mode === 'plan' && !v.completed && f.work_left_ms > 0) {
      fit = v.after_day_end
        ? ''
        : f.fits
          ? `<span class="ok">${tr('today_the_plan_fits_to_spare', { margin_ms: dur(f.margin_ms) })}</span>`
          : `<span class="bad">${tr('today_the_plan_is_too_long', { f: dur(-f.margin_ms) })}</span>`;
    }
    const html = `${icon('alarm')}<span class="grow">${tr('dialog_day_end')} <b class="tnum">${esc(endLabel(v))}</b>${v.day_end_changed ? ` <span class="was">· ${tr('today_usually')} ${esc(v.day_end_base)}</span>` : ''}${fit ? `<br>${fit}` : ''}</span>
      ${v.can.set_day_end ? `<button class="btn text interactive" data-act="dayend" aria-haspopup="menu">${tr('today_change')}</button>` : ''}`;
    if (el.dataset.html !== html) { el.innerHTML = html; el.dataset.html = html; }
  }

  function blocks(v) {
    const ol = $('blocks');
    $('total').textContent = v.blocks.length ? tr('today_of_of_study', { work_ms: dur(v.work_ms), planned_ms: dur(v.planned_ms) }) : '';
    $('total-bar').style.setProperty('--v', v.planned_ms ? Math.min(1, v.work_ms / v.planned_ms) : 0);
    $('total-bar').hidden = !v.blocks.length;
    if (!v.blocks.length) {
      ol.innerHTML = `<li class="empty-plan"><p class="body-m muted">${tr('today_todays_plan_is_empty')}</p><button class="btn tonal interactive" data-act="goplan" style="margin-top:12px">${icon('add')}${tr('today_make_a_plan')}</button></li>`;
      return;
    }
    const html = v.blocks.map((b) => {
      if (b.kind === 'break') {
        const st = b.done ? (b.started ? tr('plan_done') : tr('today_skipped')) : b.started ? tr('stats_running') : b.queued ? tr('today_queued') : tr('today_segment');
        return `<li class="blk seg ${b.done ? 'done' : ''}"><span class="st">${b.done ? icon('check', 's20') : icon('coffee', 's20')}</span>
          <span class="name ellipsis">${esc(b.name)}</span><span class="meta tnum">${b.minutes} ${tr('overlay_min')} · ${st}</span></li>`;
      }
      const st = b.done ? icon('check', 's20') : '<span class="dot"></span>';
      const meta = `${Math.floor(b.work_ms / 60000)} ${tr('dialog_of')} ${b.minutes} ${tr('overlay_min')} · ${b.done ? tr('today_done') : `${tr('today_part')} ${Math.min(b.parts_done + 1, b.parts)}/${b.parts}`}`;
      return `<li class="blk ${b.done ? 'done' : ''} ${b.current ? 'current' : ''}">
        <span class="st">${st}</span><span class="name ellipsis">${esc(b.name)}</span><span class="meta tnum">${meta}</span>
        <div class="linear" style="--v:${Math.min(1, b.work_ms / (b.minutes * 60000))}"></div></li>`;
    }).join('');
    if (ol.dataset.html !== html) {
      ol.innerHTML = html;
      ol.dataset.html = html;
      v.blocks.forEach((b, i) => {
        if (b.done && prevDone[i] === false) {
          ol.children[i]?.classList.add('just-done');
          burst(ol.children[i]?.querySelector('.st'));
        }
      });
    }
    prevDone = v.blocks.map((b) => b.done);
  }

  // End-of-block line, for when the overlay card is off or was dismissed.
  let noteFor = null;
  function noteCard(v) {
    const el = $('note-card');
    const pn = v.pending_note;
    el.hidden = !pn;
    if (!pn) { noteFor = null; return; }
    if (noteFor === pn.block) return; // keep what is being typed
    noteFor = pn.block;
    el.innerHTML = `<div class="title-m">${tr('today_closed', { name: esc(pn.name) })}</div>
      <div class="field"><label for="note-in">${tr('today_what_was_boring_what_distracted_you')}</label><input id="note-in" maxlength="300" autocomplete="off"></div>
      <div class="hstack"><button class="btn tonal interactive" data-act="note_save">${icon('check')}${tr('overlay_save')}</button>
        <button class="btn text interactive" data-act="note_skip">${tr('overlay_skip')}</button></div>`;
    el.querySelector('#note-in').addEventListener('keydown', (e) => { if (e.key === 'Enter') el.querySelector('[data-act="note_save"]').click(); });
  }

  function update(s) {
    last = s;
    const v = s.view;
    const p = v.phase;
    const hero = $('hero');
    hero.dataset.kind = p.kind;
    hero.dataset.paused = String(!!p.paused);
    hero.dataset.over = String(p.kind === 'segment' && p.remaining_ms < 0 && !p.stopwatch);
    lockLine(v, s.meta);
    kindChip(v);

    let frac = 0, running = false, big = '', small = false, state = '', title = p.title, sub = p.subtitle;
    let art = '';
    switch (p.kind) {
      case 'work':
      case 'break':
      case 'lunch_break':
        frac = p.dur_ms ? p.elapsed_ms / p.dur_ms : 0;
        running = p.running;
        big = mmss(p.remaining_ms);
        state = p.paused ? tr('today_paused') : p.kind === 'work' ? tr('log_work') : tr('log_break');
        break;
      case 'segment': {
        const over = p.remaining_ms < 0;
        if (p.stopwatch) {
          // "Без времени": counts up; the ring fills to the usual length and stays full.
          frac = p.dur_ms ? Math.min(1, p.elapsed_ms / p.dur_ms) : 0;
          running = true;
          big = mmss(p.elapsed_ms);
          state = over ? tr('today_untimed_longer_than_usual') : tr('mini_untimed');
          break;
        }
        frac = over ? 1 : p.elapsed_ms / p.dur_ms;
        running = !over;
        big = over ? `+${mmss(-p.remaining_ms)}` : mmss(p.remaining_ms);
        state = p.prep ? (over ? tr('mini_time_to_lie_down') : tr('mini_getting_ready')) : over ? tr('mini_over') : p.alarm ? tr('today_nap_alarm') : tr('today_segment');
        break;
      }
      case 'await':
        art = 'await';
        sub = `${p.subtitle} · ${tr('today_waiting')} ${mmss(p.waiting_ms)}`;
        break;
      case 'lunch':
        art = 'lunch';
        sub = `${tr('today_lunch_without_a_timer')} ${mmss(p.elapsed_ms)}`;
        break;
      case 'done':
        art = 'day';
        sub = tr('today_of_study', { work_ms: dur(v.work_ms) });
        break;
      default:
        big = v.planned_ms ? dur(v.planned_ms) : '—';
        small = true;
        state = v.mode === 'single' ? '' : tr('today_planned');
        title = v.started ? p.title : (v.study_day ? tr('today_ready_to_start') : tr('config_day_off'));
        sub = v.started ? '' : (v.blocks.length ? `${tr('today_planned_of_study', { planned_ms: dur(v.planned_ms) })} · ${v.blocks.filter((b) => b.kind !== 'break').map((b) => b.name).join(' · ')}` : tr('today_add_blocks_to_the_plan'));
        if (!v.started && v.mode !== 'single') art = v.study_day ? 'idle' : 'rest';
    }
    setArt(art);
    ring.set(frac, running);
    setBig(big, small);
    $('dstate').textContent = state;
    const t = $('ptitle');
    if (t.textContent !== title) {
      if (t.textContent) t.animate([{ opacity: 0, transform: 'translateY(10px) scale(.98)' }, { opacity: 1, transform: 'none' }], { duration: 400, easing: 'cubic-bezier(.05,.7,.1,1)' });
      t.textContent = title;
    }
    $('psub').textContent = sub;
    controls(v);
    access(v, s.meta);
    noteCard(v);
    blocks(v);
    dayEnd(v);
  }

  root.addEventListener('click', async (e) => {
    const b = e.target.closest('[data-act]');
    if (!b || !last) return;
    const act = b.dataset.act;
    const v = last.view;
    if (b.closest('#controls') && performance.now() < shieldUntil) {
      // Still clicking right after the buttons changed: ignore, and keep ignoring until the
      // clicks stop for a moment.
      shieldUntil = performance.now() + SHIELD_REPEAT_MS;
      b.animate([{ transform: 'translateX(0)' }, { transform: 'translateX(-4px)' }, { transform: 'translateX(4px)' }, { transform: 'none' }], { duration: 240, easing: 'ease-in-out' });
      return;
    }
    switch (act) {
      case 'start_day': if (v.can.night_start) await nightStartDialog(v); else await run(() => call('start_day'), b); break;
      case 'pause': await run(() => call('pause'), b); break;
      case 'resume': await run(() => call('resume'), b); break;
      // `expect`: the backend refuses if the phase moved on since this button was drawn.
      case 'start_next': {
        const wasBreak = v.phase.kind === 'break' || v.phase.kind === 'lunch_break';
        const ok = await run(() => call('start_next', { expect: v.phase.kind }).then(() => true), b);
        // Skipping a break is one click; a misclick is taken back within 10 seconds.
        if (ok && wasBreak) {
          snack(v.phase.kind === 'lunch_break' ? tr('today_lunch_ended_early') : tr('today_break_skipped'), 10000, {
            label: tr('today_cancel'),
            run: () => run(() => call('undo_skip').then(() => snack(tr('today_the_break_goes_on')))),
          });
        }
        break;
      }
      case 'stop_single': await run(() => call('stop_single'), b); break;
      case 'end_access': await run(() => call('end_access'), b); break;
      case 'mini': await call('toggle_mini'); break;
      case 'goplan': ctx.navigate('plan'); break;
      case 'kind': await pickKind(b); break;
      case 'dayend': await extendDayEnd(b); break;
      case 'segment': {
        const picked = await segmentDialog(last.meta.segments || [], v.phase.kind === 'segment', v.phase.kind === 'segment' ? v.phase.queue : []);
        if (picked?.length) snack(picked.map((t) => t.name).join(' → ') + (v.phase.kind === 'segment' ? tr('today_suffix_queued') : tr('today_countdown_started')));
        break;
      }
      case 'end_segment': await run(() => call('end_segment'), b); break;
      case 'lay_down': {
        const ok = await run(() => call('lay_down').then(() => true), b);
        if (ok) snack(`${v.phase.title}: ${tr('today_lay_down_countdown_started')}${v.phase.alarm ? tr('today_alarm_at_the_end') : ''}`);
        break;
      }
      case 'seg_mode': {
        const ok = await run(() => call('set_segment_mode', { stopwatch: !v.phase.stopwatch }).then(() => true), b);
        if (ok) snack(v.phase.stopwatch ? tr('today_counting_down_again') : tr('today_untimed_a_stopwatch_no_end_reminders'));
        break;
      }
      case 'single': await singleDialog(); break;
      case 'note_save':
      case 'note_skip': {
        const text = act === 'note_save' ? root.querySelector('#note-in')?.value.trim() : '';
        const ok = await run(() => call('set_block_note', { block: v.pending_note.block, note: text || null }).then(() => true), b);
        if (ok && text) snack(tr('today_line_saved_to_the_block'));
        break;
      }
      case 'finish': {
        const ok = await finishDialog(v);
        if (ok) snack(tr('today_block_closed_at_the_time_worked'));
        break;
      }
      case 'emergency': {
        const cfg = await call('get_config');
        const ok = await emergencyDialog(cfg, v.emergency_count);
        if (ok) snack(tr('blocking_emergency_access_for_min_logged', { emergency_min: cfg.emergency_min }));
        break;
      }
    }
  });

  return { update };
}

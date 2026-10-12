import { call, esc, dur } from '../api.js';
import { icon } from '../icons.js';
import { dialog, run, snack, holdButton } from '../ui.js';
import { tr, dec } from '../i18n.js';

export function singleDialog() {
  return dialog(`
    <h2>${tr('dialog_single_timer')}</h2>
    <p class="body-m muted">${tr('dialog_no_day_plan_work_a_break')}</p>
    <div class="inline-form">
      <div class="field"><label for="s-work">${tr('dialog_work_min')}</label><input id="s-work" type="number" min="1" max="240" value="45" autofocus></div>
      <div class="field"><label for="s-break">${tr('dialog_break_min')}</label><input id="s-break" type="number" min="1" max="120" value="10"></div>
    </div>
    <div class="toggle-line"><label for="s-hasbreak" class="body-l">${tr('dialog_break_after_work')}</label><input id="s-hasbreak" type="checkbox" class="switch" role="switch" checked></div>
    <div class="toggle-line"><label for="s-block" class="body-l">${tr('dialog_block_distractions')}</label><input id="s-block" type="checkbox" class="switch" role="switch" checked></div>
    <div class="actions">
      <button class="btn text interactive" data-close>${tr('ui_cancel')}</button>
      <button class="btn filled interactive" data-ok>${icon('play')}${tr('mini_start')}</button>
    </div>`, (d, close) => {
    const hb = d.querySelector('#s-hasbreak');
    const br = d.querySelector('#s-break');
    hb.addEventListener('change', () => { br.disabled = !hb.checked; });
    d.querySelector('[data-ok]').addEventListener('click', async (e) => {
      const work = Math.round(Number(d.querySelector('#s-work').value));
      if (!(work >= 1 && work <= 240)) { snack(tr('dialog_work_is_1_to_240_minutes')); return; }
      const cfg = { work_min: work, break_min: hb.checked ? Math.max(1, Math.round(Number(br.value) || 10)) : 0, block: d.querySelector('#s-block').checked };
      const r = await run(() => call('start_single', { cfg }).then(() => true), e.currentTarget);
      if (r) close(true);
    });
  });
}

export function emergencyDialog(cfg, countToday) {
  const phrase = cfg.emergency_phrase;
  return dialog(`
    <h2>${tr('blocking_emergency_access')}</h2>
    <p class="body-m">${tr('dialog_opens_everything_blocked_for')} <b>${cfg.emergency_min} ${tr('blocking_minutes')}</b>${tr('dialog_then_the_lock_comes_back_by')}${countToday ? tr('dialog_time_s_today_already', { count_today: countToday }) : ''}.</p>
    <p class="label-m muted">${tr('dialog_type_the_phrase_by_hand_character')}</p>
    <div class="phrase" aria-label="${tr('dialog_phrase')}">${esc(phrase)}</div>
    <div class="field"><label for="e-in">${tr('dialog_phrase')}</label><textarea id="e-in" rows="3" autocomplete="off" spellcheck="false"></textarea>
      <span class="support match" id="e-match">${tr('dialog_0_of')} ${phrase.length}</span></div>
    <div class="actions">
      <button class="btn text interactive" data-close autofocus>${tr('dialog_never_mind_back_to_work')}</button>
      <button class="btn filled danger interactive" data-ok disabled>${icon('lock_open')}${tr('dialog_open_for')} ${cfg.emergency_min} ${tr('overlay_min')}</button>
    </div>`, (d, close) => {
    const ta = d.querySelector('#e-in');
    const ok = d.querySelector('[data-ok]');
    const m = d.querySelector('#e-match');
    const norm = (s) => s.trim().split(/\s+/).join(' ');
    // No shortcuts: paste and drop are disabled.
    ['paste', 'drop'].forEach((ev) => ta.addEventListener(ev, (e) => { e.preventDefault(); snack(tr('dialog_pasting_is_off_type_it_by')); }));
    ta.addEventListener('input', () => {
      const a = norm(ta.value);
      const target = norm(phrase);
      let i = 0;
      while (i < a.length && a[i] === target[i]) i++;
      m.textContent = `${i} ${tr('dialog_of')} ${target.length}${i < a.length ? tr('dialog_mistake_at_character') + (i + 1) : ''}`;
      d.querySelector('.field').classList.toggle('invalid', i < a.length);
      ok.disabled = a !== target;
    });
    ok.addEventListener('click', async (e) => {
      const r = await run(() => call('emergency', { phrase: ta.value }).then(() => true), e.currentTarget);
      if (r) close(true);
    });
  });
}

export function captchaDialog(minutes) {
  return dialog(`
    <h2>${tr('dialog_extend_access')}</h2>
    <p class="body-m muted">${tr('dialog_solve_the_sums_in_your_head')}</p>
    <div class="problems" id="c-probs"></div>
    <p class="label-m muted" id="c-wait"></p>
    <div class="actions">
      <button class="btn text interactive" data-close>${tr('dialog_back_to_work')}</button>
      <button class="btn filled interactive hold" data-ok disabled><span class="fillbar"></span>${icon('add')}${tr('dialog_hold_3_s')}${minutes} ${tr('overlay_min')}</button>
    </div>`, async (d, close) => {
    const probs = d.querySelector('#c-probs');
    const ok = d.querySelector('[data-ok]');
    const wait = d.querySelector('#c-wait');
    const bar = ok.querySelector('.fillbar');
    let cap = null;
    let readyAt = 0;
    let tick = 0;

    const load = async () => {
      cap = await call('captcha_new');
      readyAt = Date.now() + cap.wait_ms;
      probs.innerHTML = cap.problems.map((p, i) => `<span class="p tnum">${esc(p)} =</span>
        <div class="field"><label class="sr-only" for="c${i}">${tr('dialog_answer')} ${i + 1}</label><input id="c${i}" type="number" inputmode="numeric" autocomplete="off"></div>`).join('');
      probs.querySelector('input')?.focus();
      probs.querySelectorAll('input').forEach((inp) => inp.addEventListener('paste', (e) => e.preventDefault()));
    };
    const filled = () => [...probs.querySelectorAll('input')].every((i) => i.value.trim() !== '');
    const refresh = () => {
      const left = Math.ceil((readyAt - Date.now()) / 1000);
      wait.textContent = left > 0 ? tr('dialog_the_button_unlocks_in_s', { left }) : '';
      ok.disabled = left > 0 || !filled();
    };
    tick = setInterval(refresh, 250);
    d.addEventListener('close', () => clearInterval(tick));
    probs.addEventListener('input', refresh);

    let holdStart = 0;
    let raf = 0;
    const HOLD = 3000;
    const stopHold = () => {
      cancelAnimationFrame(raf);
      holdStart = 0;
      bar.style.transition = 'transform 200ms';
      bar.style.transform = 'scaleX(0)';
    };
    const frame = async () => {
      const t = Math.min(1, (Date.now() - holdStart) / HOLD);
      bar.style.transition = 'none';
      bar.style.transform = `scaleX(${t})`;
      if (t < 1) { raf = requestAnimationFrame(frame); return; }
      stopHold();
      const answers = [...probs.querySelectorAll('input')].map((i) => Math.round(Number(i.value)));
      try {
        await call('captcha_submit', { id: cap.id, answers });
        clearInterval(tick);
        close(true);
      } catch (e) {
        snack(String(e));
        await load();
        refresh();
      }
    };
    const startHold = (e) => {
      if (ok.disabled || holdStart) return;
      e.preventDefault();
      holdStart = Date.now();
      raf = requestAnimationFrame(frame);
    };
    ok.addEventListener('pointerdown', startHold);
    ok.addEventListener('keydown', (e) => { if ((e.key === ' ' || e.key === 'Enter') && !e.repeat) startHold(e); });
    ['pointerup', 'pointerleave', 'keyup', 'blur'].forEach((ev) => ok.addEventListener(ev, () => { if (holdStart) stopHold(); }));
    ok.addEventListener('click', (e) => e.preventDefault());

    await load();
    refresh();
  });
}

/**
 * "Close the block now" — deliberately slow: the numbers first, then a 3-second hold.
 * A habit click or a stray Enter never closes a block.
 */
export function finishDialog(v) {
  const i = v.phase.block;
  const b = v.blocks[i];
  if (!b) return Promise.resolve(false);
  const worked = b.work_ms / 60000;
  const workedTxt = dec(Math.round(worked * 10) / 10);
  // Whole minutes, rounded down: the backend closes on exactly this.
  const to = Math.max(1, Math.floor(worked));
  const cut = Math.max(0, b.minutes - to);
  return dialog(`
    <h2>${tr('dialog_close')}${esc(b.name)}${tr('dialog_now')}</h2>
    <p class="body-l">${tr('dialog_worked')} <b>${workedTxt} ${tr('dialog_of')} ${b.minutes} ${tr('overlay_min')}</b>.</p>
    <p class="body-m muted">${tr('dialog_the_blocks_plan_becomes_min', { to })}${cut ? ` — <b>${cut} ${tr('overlay_min')}</b> ${tr('dialog_not_worked_leave_the_plan')}` : ''}. ${tr('dialog_next_the_break_between_blocks')}${v.blocks.filter((x) => !x.done).length <= 1 ? tr('dialog_and_the_end_of_the_day') : ''}. ${tr('dialog_this_goes_to_the_log')}</p>
    <div class="actions">
      <button class="btn text interactive" data-close autofocus>${tr('dialog_no_keep_working')}</button>
      <button class="btn tonal interactive hold" data-ok><span class="fillbar"></span>${icon('check')}${tr('dialog_hold_3_s_close')}</button>
    </div>`, (d, close) => {
    holdButton(d.querySelector('[data-ok]'), 3000, async () => {
      const r = await run(() => call('finish_block', { name: b.name }));
      if (r) close(true);
    });
  });
}

/**
 * "Отрезок": pick one or several (lunch → nap); they run one after another. While a segment
 * runs, the picks go to its queue.
 */
export function segmentDialog(types, running, queue = []) {
  const picked = [];
  // While a segment runs: what already waits, each can be taken out.
  const queued = queue.length ? `<div class="seg-queue" id="sq-now"><span class="body-m muted">${tr('dialog_already_queued')}</span>${queue.map((q, i) => `<span class="chip input removable">${esc(q)}<button class="x interactive" data-drop="${i}" aria-label="${tr('dialog_remove_from_the_queue', { q: esc(q) })}">${icon('close')}</button></span>`).join('')}</div>` : '';
  return dialog(`
    <h2>${running ? tr('dialog_add_to_the_queue') : tr('config_segment')}</h2>
    <p class="body-m muted">${running ? tr('dialog_starts_right_after_the_current_one') : tr('dialog_several_in_a_row_form_a')}</p>
    ${queued}
    <div class="segmented" role="group" aria-label="${tr('dialog_how_to_count')}" id="seg-mode">
      <button class="interactive" data-mode="timer" aria-pressed="true">${icon('schedule')}${tr('dialog_timed')}</button>
      <button class="interactive" data-mode="stopwatch" aria-pressed="false">${icon('timer')}${tr('dialog_untimed')}</button>
    </div>
    <div class="seg-types">${types.map((t, i) => `<button class="btn tonal interactive" data-t="${i}">${icon(t.alarm ? 'alarm' : /^(обед|lunch)/i.test(t.name) ? 'restaurant' : 'coffee')}${esc(t.name)} · ${t.minutes} ${tr('overlay_min')}</button>`).join('')}</div>
    <div class="seg-queue" id="sq" aria-live="polite"></div>
    <div class="actions">
      <button class="btn text interactive" data-close>${tr('ui_cancel')}</button>
      <button class="btn filled interactive" data-ok disabled>${icon('play')}${running ? tr('dialog_queue') : tr('overlay_start')}</button>
    </div>`, (d, close) => {
    const sq = d.querySelector('#sq');
    const ok = d.querySelector('[data-ok]');
    // The mode applies to the types picked from now on: lunch without a timer → a nap with one.
    let stopwatch = false;
    const label = (t) => (t.stopwatch ? `${t.name} ${tr('mini_untimed')}` : `${t.name} ${t.minutes} ${tr('overlay_min')}${t.prep_min ? tr('dialog_to_get_ready', { prep_min: t.prep_min }) : ''}`);
    const draw = () => {
      sq.innerHTML = picked.length
        ? picked.map((t, i) => `<span class="chip input removable">${esc(label(t))}<button class="x interactive" data-rm="${i}" aria-label="${tr('blocking_remove')}">${icon('close')}</button></span>`).join('<span class="arrow">→</span>')
        : `<span class="body-m muted">${stopwatch ? tr('dialog_a_stopwatch_counting_up_no_end') : tr('dialog_tap_a_type_several_are_fine')}</span>`;
      ok.disabled = !picked.length;
    };
    d.querySelector('#seg-mode').addEventListener('click', (e) => {
      const m = e.target.closest('[data-mode]');
      if (!m) return;
      stopwatch = m.dataset.mode === 'stopwatch';
      d.querySelectorAll('#seg-mode [data-mode]').forEach((x) => x.setAttribute('aria-pressed', String(x === m)));
      draw();
    });
    draw();
    d.addEventListener('click', async (e) => {
      const drop = e.target.closest('[data-drop]');
      if (drop) {
        const ok = await run(() => call('drop_queued', { index: Number(drop.dataset.drop) }).then(() => true));
        if (ok) {
          // Indices shift after a removal: drop the chip and renumber the rest.
          drop.closest('.chip').remove();
          d.querySelectorAll('[data-drop]').forEach((b, i) => { b.dataset.drop = String(i); });
          if (!d.querySelector('[data-drop]')) d.querySelector('#sq-now')?.remove();
          snack(tr('dialog_removed_from_the_queue'));
        }
        return;
      }
      const t = e.target.closest('[data-t]');
      const rm = e.target.closest('[data-rm]');
      if (t) {
        const ty = types[Number(t.dataset.t)];
        const prep = ty.prep_min ?? (ty.alarm ? 10 : 0);
        picked.push({ ...ty, stopwatch, prep_min: stopwatch ? 0 : prep });
        draw();
      }
      if (rm) { picked.splice(Number(rm.dataset.rm), 1); draw(); }
    });
    ok.addEventListener('click', async (e) => {
      const items = picked.map((t) => ({ name: t.name, minutes: t.minutes, stopwatch: !!t.stopwatch }));
      const r = await run(() => call('start_segments', { items }).then(() => true), e.currentTarget);
      if (r) close(picked);
    });
  });
}

/** One-off day end for today only: later when there is no time, earlier to finish sooner. */
export function dayEndDialog(v) {
  const midnight = v.day_end_at - v.day_end_min * 60000;
  return dialog(`
    <h2>${tr('dialog_day_end')}</h2>
    <p class="body-m muted">${tr('dialog_today_only_the_setting_stays')} ${esc(v.day_end_base)}${tr('dialog_the_lock_the_timers_and_the')}</p>
    <div class="inline-form">
      <div class="field"><label for="de-time">${tr('dialog_day_end_field')}</label><input id="de-time" type="time" value="${esc(v.day_end)}" required autofocus>
        <span class="support">${tr('dialog_later_than_now_and_no_later')}</span></div>
    </div>
    <div class="field"><label for="de-reason">${tr('dialog_reason_for_the_log_optional')}</label><input id="de-reason" maxlength="200" autocomplete="off"></div>
    <p class="body-m" id="de-fit" aria-live="polite"></p>
    <div class="actions">
      ${v.day_end_changed ? `<button class="btn text interactive" data-reset>${tr('dialog_back_to')} ${esc(v.day_end_base)}</button><span class="grow"></span>` : ''}
      <button class="btn text interactive" data-close>${tr('ui_cancel')}</button>
      <button class="btn filled interactive" data-ok>${icon('check')}${tr('overlay_save')}</button>
    </div>`, (d, close) => {
    const time = d.querySelector('#de-time');
    const fit = d.querySelector('#de-fit');
    const ok = d.querySelector('[data-ok]');
    const target = (hm) => {
      const [h, m] = hm.split(':').map(Number);
      if (!Number.isFinite(h) || !Number.isFinite(m)) return null;
      const min = h * 60 + m;
      return midnight + (min <= 120 ? min + 1440 : min) * 60000;
    };
    const preview = () => {
      const ts = target(time.value);
      const f = v.forecast;
      let text = '';
      let bad = false;
      if (ts === null) { text = tr('dialog_enter_a_time'); bad = true; }
      else if (ts <= Date.now()) { text = tr('dialog_that_time_has_passed'); bad = true; }
      else if (f.work_left_ms <= 0) text = tr('dialog_todays_plan_is_already_closed');
      else {
        const margin = ts - f.finish_at;
        text = tr('dialog_left_of_study_of_breaks', { work_left_ms: dur(f.work_left_ms), breaks_left_ms: dur(f.breaks_left_ms), fit: margin >= 0 ? tr('dialog_fits_spare', { spare: dur(margin) }) : tr('dialog_too_long', { over: dur(-margin) }) });
      }
      fit.textContent = text;
      fit.classList.toggle('error-text', bad);
      ok.disabled = bad;
    };
    time.addEventListener('input', preview);
    preview();
    const submit = async (hm, btn) => {
      const reason = d.querySelector('#de-reason').value.trim() || null;
      const r = await run(() => call('set_day_end', { time: hm, reason }), btn);
      if (r) close(r);
    };
    ok.addEventListener('click', (e) => submit(time.value, e.currentTarget));
    d.querySelector('[data-reset]')?.addEventListener('click', (e) => { e.preventDefault(); submit(v.day_end_base, e.currentTarget); });
  });
}

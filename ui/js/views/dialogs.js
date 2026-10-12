import { call, esc, dur } from '../api.js';
import { icon } from '../icons.js';
import { dialog, run, snack, holdButton } from '../ui.js';
import { L, dec } from '../i18n.js';

export function singleDialog() {
  return dialog(`
    <h2>${L('Один таймер', 'Single timer')}</h2>
    <p class="body-m muted">${L('Без плана дня: работа, перерыв, и так по кругу, пока не остановишь.', 'No day plan: work, a break, and round again until you stop it.')}</p>
    <div class="inline-form">
      <div class="field"><label for="s-work">${L('Работа, мин', 'Work, min')}</label><input id="s-work" type="number" min="1" max="240" value="45" autofocus></div>
      <div class="field"><label for="s-break">${L('Перерыв, мин', 'Break, min')}</label><input id="s-break" type="number" min="1" max="120" value="10"></div>
    </div>
    <div class="toggle-line"><label for="s-hasbreak" class="body-l">${L('Перерыв после работы', 'Break after work')}</label><input id="s-hasbreak" type="checkbox" class="switch" role="switch" checked></div>
    <div class="toggle-line"><label for="s-block" class="body-l">${L('Блокировать отвлекалки', 'Block distractions')}</label><input id="s-block" type="checkbox" class="switch" role="switch" checked></div>
    <div class="actions">
      <button class="btn text interactive" data-close>${L('Отмена', 'Cancel')}</button>
      <button class="btn filled interactive" data-ok>${icon('play')}${L('Старт', 'Start')}</button>
    </div>`, (d, close) => {
    const hb = d.querySelector('#s-hasbreak');
    const br = d.querySelector('#s-break');
    hb.addEventListener('change', () => { br.disabled = !hb.checked; });
    d.querySelector('[data-ok]').addEventListener('click', async (e) => {
      const work = Math.round(Number(d.querySelector('#s-work').value));
      if (!(work >= 1 && work <= 240)) { snack(L('Работа — от 1 до 240 минут', 'Work is 1 to 240 minutes')); return; }
      const cfg = { work_min: work, break_min: hb.checked ? Math.max(1, Math.round(Number(br.value) || 10)) : 0, block: d.querySelector('#s-block').checked };
      const r = await run(() => call('start_single', { cfg }).then(() => true), e.currentTarget);
      if (r) close(true);
    });
  });
}

export function emergencyDialog(cfg, countToday) {
  const phrase = cfg.emergency_phrase;
  return dialog(`
    <h2>${L('Аварийный доступ', 'Emergency access')}</h2>
    <p class="body-m">${L('Откроет всё заблокированное на', 'Opens everything blocked for')} <b>${cfg.emergency_min} ${L('минут', 'minutes')}</b>${L(', потом блокировка вернётся сама. Это попадёт в лог', ', then the lock comes back by itself. This goes to the log')}${countToday ? L(` — сегодня уже ${countToday} раз`, ` — ${countToday} time(s) today already`) : ''}.</p>
    <p class="label-m muted">${L('Перепиши фразу руками, символ в символ:', 'Type the phrase by hand, character for character:')}</p>
    <div class="phrase" aria-label="${L('Фраза', 'Phrase')}">${esc(phrase)}</div>
    <div class="field"><label for="e-in">${L('Фраза', 'Phrase')}</label><textarea id="e-in" rows="3" autocomplete="off" spellcheck="false"></textarea>
      <span class="support match" id="e-match">${L('0 из', '0 of')} ${phrase.length}</span></div>
    <div class="actions">
      <button class="btn text interactive" data-close autofocus>${L('Не надо, работаю', 'Never mind, back to work')}</button>
      <button class="btn filled danger interactive" data-ok disabled>${icon('lock_open')}${L('Открыть на', 'Open for')} ${cfg.emergency_min} ${L('мин', 'min')}</button>
    </div>`, (d, close) => {
    const ta = d.querySelector('#e-in');
    const ok = d.querySelector('[data-ok]');
    const m = d.querySelector('#e-match');
    const norm = (s) => s.trim().split(/\s+/).join(' ');
    // No shortcuts: paste and drop are disabled.
    ['paste', 'drop'].forEach((ev) => ta.addEventListener(ev, (e) => { e.preventDefault(); snack(L('Вставка отключена — только руками', 'Pasting is off — type it by hand')); }));
    ta.addEventListener('input', () => {
      const a = norm(ta.value);
      const target = norm(phrase);
      let i = 0;
      while (i < a.length && a[i] === target[i]) i++;
      m.textContent = `${i} ${L('из', 'of')} ${target.length}${i < a.length ? L(' · ошибка в символе ', ' · mistake at character ') + (i + 1) : ''}`;
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
    <h2>${L('Продлить доступ', 'Extend access')}</h2>
    <p class="body-m muted">${L('Реши примеры в уме и удерживай кнопку. Если это слишком лениво — может, пора работать?', 'Solve the sums in your head and hold the button. If that’s too much effort — maybe it’s time to work?')}</p>
    <div class="problems" id="c-probs"></div>
    <p class="label-m muted" id="c-wait"></p>
    <div class="actions">
      <button class="btn text interactive" data-close>${L('Вернуться к работе', 'Back to work')}</button>
      <button class="btn filled interactive hold" data-ok disabled><span class="fillbar"></span>${icon('add')}${L('Удерживай 3 сек · +', 'Hold 3 s · +')}${minutes} ${L('мин', 'min')}</button>
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
        <div class="field"><label class="sr-only" for="c${i}">${L('Ответ', 'Answer')} ${i + 1}</label><input id="c${i}" type="number" inputmode="numeric" autocomplete="off"></div>`).join('');
      probs.querySelector('input')?.focus();
      probs.querySelectorAll('input').forEach((inp) => inp.addEventListener('paste', (e) => e.preventDefault()));
    };
    const filled = () => [...probs.querySelectorAll('input')].every((i) => i.value.trim() !== '');
    const refresh = () => {
      const left = Math.ceil((readyAt - Date.now()) / 1000);
      wait.textContent = left > 0 ? L(`Кнопка станет доступна через ${left} с`, `The button unlocks in ${left} s`) : '';
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
    <h2>${L('Закрыть «', 'Close “')}${esc(b.name)}${L('» сейчас?', '” now?')}</h2>
    <p class="body-l">${L('Отработано', 'Worked')} <b>${workedTxt} ${L('из', 'of')} ${b.minutes} ${L('мин', 'min')}</b>.</p>
    <p class="body-m muted">${L(`План блока станет ${to} мин`, `The block’s plan becomes ${to} min`)}${cut ? ` — <b>${cut} ${L('мин', 'min')}</b> ${L('недоработки уйдут из плана', 'not worked leave the plan')}` : ''}. ${L('Дальше — перерыв между блоками', 'Next — the break between blocks')}${v.blocks.filter((x) => !x.done).length <= 1 ? L(' и конец дня', ' and the end of the day') : ''}. ${L('Это попадёт в лог.', 'This goes to the log.')}</p>
    <div class="actions">
      <button class="btn text interactive" data-close autofocus>${L('Нет, работаю дальше', 'No, keep working')}</button>
      <button class="btn tonal interactive hold" data-ok><span class="fillbar"></span>${icon('check')}${L('Удерживай 3 сек — закрыть', 'Hold 3 s — close')}</button>
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
  const queued = queue.length ? `<div class="seg-queue" id="sq-now"><span class="body-m muted">${L('Уже в очереди:', 'Already queued:')}</span>${queue.map((q, i) => `<span class="chip input removable">${esc(q)}<button class="x interactive" data-drop="${i}" aria-label="${L(`Убрать ${esc(q)} из очереди`, `Remove ${esc(q)} from the queue`)}">${icon('close')}</button></span>`).join('')}</div>` : '';
  return dialog(`
    <h2>${running ? L('Добавить в очередь', 'Add to the queue') : L('Отрезок', 'Segment')}</h2>
    <p class="body-m muted">${running ? L('Начнётся сразу после текущего.', 'Starts right after the current one.') : L('Несколько подряд идут очередью. Блокировка — как на перерыве.', 'Several in a row form a queue. The lock works as on a break.')}</p>
    ${queued}
    <div class="segmented" role="group" aria-label="${L('Как считать', 'How to count')}" id="seg-mode">
      <button class="interactive" data-mode="timer" aria-pressed="true">${icon('schedule')}${L('По таймеру', 'Timed')}</button>
      <button class="interactive" data-mode="stopwatch" aria-pressed="false">${icon('timer')}${L('Без времени', 'Untimed')}</button>
    </div>
    <div class="seg-types">${types.map((t, i) => `<button class="btn tonal interactive" data-t="${i}">${icon(t.alarm ? 'alarm' : /^(обед|lunch)/i.test(t.name) ? 'restaurant' : 'coffee')}${esc(t.name)} · ${t.minutes} ${L('мин', 'min')}</button>`).join('')}</div>
    <div class="seg-queue" id="sq" aria-live="polite"></div>
    <div class="actions">
      <button class="btn text interactive" data-close>${L('Отмена', 'Cancel')}</button>
      <button class="btn filled interactive" data-ok disabled>${icon('play')}${running ? L('В очередь', 'Queue') : L('Начать', 'Start')}</button>
    </div>`, (d, close) => {
    const sq = d.querySelector('#sq');
    const ok = d.querySelector('[data-ok]');
    // The mode applies to the types picked from now on: lunch without a timer → a nap with one.
    let stopwatch = false;
    const label = (t) => (t.stopwatch ? `${t.name} ${L('без времени', 'untimed')}` : `${t.name} ${t.minutes} ${L('мин', 'min')}${t.prep_min ? L(` + ${t.prep_min} на подготовку`, ` + ${t.prep_min} to get ready`) : ''}`);
    const draw = () => {
      sq.innerHTML = picked.length
        ? picked.map((t, i) => `<span class="chip input removable">${esc(label(t))}<button class="x interactive" data-rm="${i}" aria-label="${L('Убрать', 'Remove')}">${icon('close')}</button></span>`).join('<span class="arrow">→</span>')
        : `<span class="body-m muted">${stopwatch ? L('Секундомер вверх, без напоминаний о конце; факт попадёт в статистику.', 'A stopwatch counting up, no end reminders; the actual time goes to the stats.') : L('Нажми на тип — можно несколько: обед → сон', 'Tap a type — several are fine: lunch → nap')}</span>`;
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
          snack(L('Убрано из очереди', 'Removed from the queue'));
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
    <h2>${L('Конец дня', 'Day end')}</h2>
    <p class="body-m muted">${L('Только на сегодня — в настройках останется', 'Today only — the setting stays')} ${esc(v.day_end_base)}${L('. Блокировка, таймеры и уведомление «день закончен» сразу пойдут от нового времени.', '. The lock, the timers and the “day is over” notice follow the new time at once.')}</p>
    <div class="inline-form">
      <div class="field"><label for="de-time">${L('Конец дня, МСК', 'Day end')}</label><input id="de-time" type="time" value="${esc(v.day_end)}" required autofocus>
        <span class="support">${L('Позже текущего времени и не позже 02:00 ночи', 'Later than now and no later than 02:00 at night')}</span></div>
    </div>
    <div class="field"><label for="de-reason">${L('Причина — в лог, необязательно', 'Reason — for the log, optional')}</label><input id="de-reason" maxlength="200" autocomplete="off"></div>
    <p class="body-m" id="de-fit" aria-live="polite"></p>
    <div class="actions">
      ${v.day_end_changed ? `<button class="btn text interactive" data-reset>${L('Вернуть', 'Back to')} ${esc(v.day_end_base)}</button><span class="grow"></span>` : ''}
      <button class="btn text interactive" data-close>${L('Отмена', 'Cancel')}</button>
      <button class="btn filled interactive" data-ok>${icon('check')}${L('Сохранить', 'Save')}</button>
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
      if (ts === null) { text = L('Укажи время', 'Enter a time'); bad = true; }
      else if (ts <= Date.now()) { text = L('Это время уже прошло', 'That time has passed'); bad = true; }
      else if (f.work_left_ms <= 0) text = L('План на сегодня уже закрыт', 'Today’s plan is already closed');
      else {
        const margin = ts - f.finish_at;
        text = L(
          `Осталось ${dur(f.work_left_ms)} учёбы + ${dur(f.breaks_left_ms)} перерывов: ${margin >= 0 ? `влезает, запас ${dur(margin)}` : `не влезает на ${dur(-margin)}`}`,
          `Left: ${dur(f.work_left_ms)} of study + ${dur(f.breaks_left_ms)} of breaks: ${margin >= 0 ? `fits, ${dur(margin)} to spare` : `${dur(-margin)} too long`}`,
        );
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

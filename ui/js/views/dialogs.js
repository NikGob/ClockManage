import { call, esc, dur } from '../api.js';
import { icon } from '../icons.js';
import { dialog, run, snack, holdButton } from '../ui.js';

export function lunchDialog(lunchMin) {
  return dialog(`
    <h2>Обед</h2>
    <p class="body-m muted">Один раз за день. Учебное время не идёт.</p>
    <div class="options" role="radiogroup" aria-label="Как обедаешь">
      <label class="option">
        <input type="radio" name="mode" value="free" checked>
        <span class="t">Просто поесть</span>
        <span class="d">Без таймера. Блокировка остаётся. Вернёшься — нажмёшь «Пообедал».</span>
      </label>
      <label class="option">
        <input type="radio" name="mode" value="timer">
        <span class="t">Таймер ${lunchMin} мин</span>
        <span class="d">По окончании зазвонит будильник.</span>
        <span class="sub"><input type="checkbox" class="check" name="pc" id="lunch-pc"><label for="lunch-pc">Ем за ПК — открыть YouTube, Discord и остальное на время обеда</label></span>
      </label>
    </div>
    <div class="actions">
      <button class="btn text interactive" data-close>Отмена</button>
      <button class="btn filled interactive" data-ok>${icon('restaurant')}Начать обед</button>
    </div>`, (d, close) => {
    const pc = d.querySelector('[name=pc]');
    const sync = () => {
      const timer = d.querySelector('[name=mode]:checked').value === 'timer';
      pc.disabled = !timer;
      if (!timer) pc.checked = false;
    };
    d.querySelectorAll('[name=mode]').forEach((r) => r.addEventListener('change', sync));
    pc.addEventListener('change', () => { if (pc.checked) d.querySelector('[value=timer]').checked = true; sync(); });
    sync();
    d.querySelector('[data-ok]').addEventListener('click', async (e) => {
      const withTimer = d.querySelector('[name=mode]:checked').value === 'timer';
      const r = await run(() => call('start_lunch', { withTimer, atPc: withTimer && pc.checked }).then(() => true), e.currentTarget);
      if (r) close(true);
    });
  });
}

export function singleDialog() {
  return dialog(`
    <h2>Один таймер</h2>
    <p class="body-m muted">Без плана дня: работа, перерыв, и так по кругу, пока не остановишь.</p>
    <div class="inline-form">
      <div class="field"><label for="s-work">Работа, мин</label><input id="s-work" type="number" min="1" max="240" value="45" autofocus></div>
      <div class="field"><label for="s-break">Перерыв, мин</label><input id="s-break" type="number" min="1" max="120" value="10"></div>
    </div>
    <div class="toggle-line"><label for="s-hasbreak" class="body-l">Перерыв после работы</label><input id="s-hasbreak" type="checkbox" class="switch" role="switch" checked></div>
    <div class="toggle-line"><label for="s-block" class="body-l">Блокировать отвлекалки</label><input id="s-block" type="checkbox" class="switch" role="switch" checked></div>
    <div class="actions">
      <button class="btn text interactive" data-close>Отмена</button>
      <button class="btn filled interactive" data-ok>${icon('play')}Старт</button>
    </div>`, (d, close) => {
    const hb = d.querySelector('#s-hasbreak');
    const br = d.querySelector('#s-break');
    hb.addEventListener('change', () => { br.disabled = !hb.checked; });
    d.querySelector('[data-ok]').addEventListener('click', async (e) => {
      const work = Math.round(Number(d.querySelector('#s-work').value));
      if (!(work >= 1 && work <= 240)) { snack('Работа — от 1 до 240 минут'); return; }
      const cfg = { work_min: work, break_min: hb.checked ? Math.max(1, Math.round(Number(br.value) || 10)) : 0, block: d.querySelector('#s-block').checked };
      const r = await run(() => call('start_single', { cfg }).then(() => true), e.currentTarget);
      if (r) close(true);
    });
  });
}

export function emergencyDialog(cfg, countToday) {
  const phrase = cfg.emergency_phrase;
  return dialog(`
    <h2>Аварийный доступ</h2>
    <p class="body-m">Откроет всё заблокированное на <b>${cfg.emergency_min} минут</b>, потом блокировка вернётся сама. Это попадёт в лог${countToday ? ` — сегодня уже ${countToday} раз` : ''}.</p>
    <p class="label-m muted">Перепиши фразу руками, символ в символ:</p>
    <div class="phrase" aria-label="Фраза">${esc(phrase)}</div>
    <div class="field"><label for="e-in">Фраза</label><textarea id="e-in" rows="3" autocomplete="off" spellcheck="false"></textarea>
      <span class="support match" id="e-match">0 из ${phrase.length}</span></div>
    <div class="actions">
      <button class="btn text interactive" data-close autofocus>Не надо, работаю</button>
      <button class="btn filled danger interactive" data-ok disabled>${icon('lock_open')}Открыть на ${cfg.emergency_min} мин</button>
    </div>`, (d, close) => {
    const ta = d.querySelector('#e-in');
    const ok = d.querySelector('[data-ok]');
    const m = d.querySelector('#e-match');
    const norm = (s) => s.trim().split(/\s+/).join(' ');
    // No shortcuts: paste and drop are disabled.
    ['paste', 'drop'].forEach((ev) => ta.addEventListener(ev, (e) => { e.preventDefault(); snack('Вставка отключена — только руками'); }));
    ta.addEventListener('input', () => {
      const a = norm(ta.value);
      const target = norm(phrase);
      let i = 0;
      while (i < a.length && a[i] === target[i]) i++;
      m.textContent = `${i} из ${target.length}${i < a.length ? ' · ошибка в символе ' + (i + 1) : ''}`;
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
    <h2>Продлить доступ</h2>
    <p class="body-m muted">Реши примеры в уме и удерживай кнопку. Если это слишком лениво — может, пора работать?</p>
    <div class="problems" id="c-probs"></div>
    <p class="label-m muted" id="c-wait"></p>
    <div class="actions">
      <button class="btn text interactive" data-close>Вернуться к работе</button>
      <button class="btn filled interactive hold" data-ok disabled><span class="fillbar"></span>${icon('add')}Удерживай 3 сек · +${minutes} мин</button>
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
        <div class="field"><label class="sr-only" for="c${i}">Ответ ${i + 1}</label><input id="c${i}" type="number" inputmode="numeric" autocomplete="off"></div>`).join('');
      probs.querySelector('input')?.focus();
      probs.querySelectorAll('input').forEach((inp) => inp.addEventListener('paste', (e) => e.preventDefault()));
    };
    const filled = () => [...probs.querySelectorAll('input')].every((i) => i.value.trim() !== '');
    const refresh = () => {
      const left = Math.ceil((readyAt - Date.now()) / 1000);
      wait.textContent = left > 0 ? `Кнопка станет доступна через ${left} с` : '';
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
  const workedTxt = String(Math.round(worked * 10) / 10).replace('.', ',');
  const to = Math.max(1, Math.round(worked));
  const cut = Math.max(0, b.minutes - to);
  return dialog(`
    <h2>Закрыть «${esc(b.name)}» сейчас?</h2>
    <p class="body-l">Отработано <b>${workedTxt} из ${b.minutes} мин</b>.</p>
    <p class="body-m muted">План блока станет ${to} мин${cut ? ` — <b>${cut} мин</b> недоработки уйдут из плана` : ''}. Дальше — перерыв между блоками${v.blocks.filter((x) => !x.done).length <= 1 ? ' и конец дня' : ''}. Это попадёт в лог.</p>
    <div class="actions">
      <button class="btn text interactive" data-close autofocus>Нет, работаю дальше</button>
      <button class="btn tonal interactive hold" data-ok><span class="fillbar"></span>${icon('check')}Удерживай 3 сек — закрыть</button>
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
export function segmentDialog(types, running) {
  const picked = [];
  return dialog(`
    <h2>${running ? 'Добавить в очередь' : 'Отрезок'}</h2>
    <p class="body-m muted">${running ? 'Начнётся сразу после текущего.' : 'Обратный отсчёт; несколько подряд идут очередью. Блокировка — как на перерыве.'}</p>
    <div class="seg-types">${types.map((t, i) => `<button class="btn tonal interactive" data-t="${i}">${icon(t.alarm ? 'alarm' : t.name.toLowerCase().startsWith('обед') ? 'restaurant' : 'coffee')}${esc(t.name)} · ${t.minutes} мин</button>`).join('')}</div>
    <div class="seg-queue" id="sq" aria-live="polite"></div>
    <div class="actions">
      <button class="btn text interactive" data-close>Отмена</button>
      <button class="btn filled interactive" data-ok disabled>${icon('play')}${running ? 'В очередь' : 'Начать'}</button>
    </div>`, (d, close) => {
    const sq = d.querySelector('#sq');
    const ok = d.querySelector('[data-ok]');
    const draw = () => {
      sq.innerHTML = picked.length
        ? picked.map((t, i) => `<span class="chip input removable">${esc(t.name)} ${t.minutes} мин<button class="x interactive" data-rm="${i}" aria-label="Убрать">${icon('close')}</button></span>`).join('<span class="arrow">→</span>')
        : '<span class="body-m muted">Нажми на тип — можно несколько: обед → сон</span>';
      ok.disabled = !picked.length;
    };
    draw();
    d.addEventListener('click', (e) => {
      const t = e.target.closest('[data-t]');
      const rm = e.target.closest('[data-rm]');
      if (t) { picked.push(types[Number(t.dataset.t)]); draw(); }
      if (rm) { picked.splice(Number(rm.dataset.rm), 1); draw(); }
    });
    ok.addEventListener('click', async (e) => {
      const items = picked.map((t) => ({ name: t.name, minutes: t.minutes }));
      const r = await run(() => call('start_segments', { items }).then(() => true), e.currentTarget);
      if (r) close(picked);
    });
  });
}

/** One-off day end for today only: later when there is no time, earlier to finish sooner. */
export function dayEndDialog(v) {
  const midnight = v.day_end_at - v.day_end_min * 60000;
  return dialog(`
    <h2>Конец дня</h2>
    <p class="body-m muted">Только на сегодня — в настройках останется ${esc(v.day_end_base)}. Блокировка, таймеры и уведомление «день закончен» сразу пойдут от нового времени.</p>
    <div class="inline-form">
      <div class="field"><label for="de-time">Конец дня, МСК</label><input id="de-time" type="time" value="${esc(v.day_end)}" required autofocus>
        <span class="support">Позже текущего времени и не позже 02:00 ночи</span></div>
    </div>
    <div class="field"><label for="de-reason">Причина — в лог, необязательно</label><input id="de-reason" maxlength="200" autocomplete="off"></div>
    <p class="body-m" id="de-fit" aria-live="polite"></p>
    <div class="actions">
      ${v.day_end_changed ? `<button class="btn text interactive" data-reset>Вернуть ${esc(v.day_end_base)}</button><span class="grow"></span>` : ''}
      <button class="btn text interactive" data-close>Отмена</button>
      <button class="btn filled interactive" data-ok>${icon('check')}Сохранить</button>
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
      if (ts === null) { text = 'Укажи время'; bad = true; }
      else if (ts <= Date.now()) { text = 'Это время уже прошло'; bad = true; }
      else if (f.work_left_ms <= 0) text = 'План на сегодня уже закрыт';
      else {
        const margin = ts - f.finish_at;
        text = `Осталось ${dur(f.work_left_ms)} учёбы + ${dur(f.breaks_left_ms)} перерывов: ${margin >= 0 ? `влезает, запас ${dur(margin)}` : `не влезает на ${dur(-margin)}`}`;
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

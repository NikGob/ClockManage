import { call, esc } from '../api.js';
import { icon } from '../icons.js';
import { dialog, run, snack } from '../ui.js';

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

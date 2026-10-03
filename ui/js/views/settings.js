import { call, esc } from '../api.js';
import { icon, CHECK } from '../icons.js';
import { applyTheme } from '../theme.js';
import { run, snack, menu, WEEKDAYS, WEEKDAYS_FULL, KINDS, kindLabel, planSummary, hm } from '../ui.js';

const SWATCHES = [
  ['#2E7D32', 'Зелёный'], ['#00796B', 'Бирюзовый'], ['#1565C0', 'Синий'], ['#5E35B1', 'Фиолетовый'],
  ['#C2185B', 'Малиновый'], ['#E65100', 'Оранжевый'], ['#6D4C41', 'Коричневый'],
];

// Longer explanations live in tooltips so the screen itself stays short.
const TIP = {
  block: 'В такой день «Начать день» закрывает сайты и приложения из списка — до конца дня или пока не закроешь весь план.',
  plan: 'С этого плана начинается каждый такой день. Сегодняшний план правится на экране «План».',
  dayEnd: 'Во сколько блокировка снимается сама, даже если план не закрыт. Во время учёбы это значение по умолчанию можно только сдвинуть позже.\nСдвинуть только сегодняшний день (позже или раньше, до 02:00 ночи) — нажми на время на экране «Сегодня».',
  seg: 'Блок режется на части такой длины. Хвост короче 15 мин приклеивается к последней части: 50 мин — это одна часть.',
  short: 'Отдых между частями одного блока. Не съедает учебное время. Во время учёбы его можно только сократить.',
  between: 'Отдых после закрытого блока, перед следующим. Во время учёбы — только короче.',
  segments: 'Неучебные отрезки с обратным отсчётом: кнопка «Отрезок» в перерыве или пункт плана. За 5 минут и в конце — уведомление, потом каждые 5 минут — на сколько превышено. Блокировка — как на перерыве. Во время учёбы — только короче.',
  segAlarm: 'Конец — громкий будильник, который звонит, пока не нажмёшь «Встал» (даже если звук выключен). Без предупреждения за 5 минут. Для сна.',
  segAccess: 'На время отрезка (до его планового конца) заблокированное открыто — как «ем за ПК». Включается только вне учёбы.',
  sound: 'Звонок на конце перерыва, мягкий сигнал на конце работы и фанфары на закрытии блока.',
  overlay: 'Рисованная карточка поверх всех окон на каждом переходе: будильник, перерыв, закрытый блок, «Не-не-не».',
  reminder: 'Пока таймер ждёт «Начать часть», звонок повторяется с этим интервалом. После конца дня — молчит.',
  contrast: 'Инверсные цвета: тёмная плашка в светлой теме и светлая в тёмной. Не теряется ни на каком фоне.',
  phone: 'Приложение ClockManage для Android видит таймер по Wi-Fi, может ставить паузу, начинать части и менять длину блоков. Без связи продолжает считать само. Сайты из блок-листа и выбранные приложения блокирует и на телефоне.',
  mcp: 'Локальный сервер для Claude: видит состояние таймера и статистику, может задать план. Слушает только 127.0.0.1.',
  autostart: 'Запуск при входе в Windows без окна UAC и перезапуск раз в 5 минут, если процесс убили. Во время учёбы выключить нельзя.',
};
const MODE_TIP = { system: 'Как в Windows', light: 'Всегда светлая', dark: 'Всегда тёмная' };
const VARIANT_TIP = {
  fidelity: 'Цвет ровно как выбран — самые заметные акценты',
  tonal_spot: 'Приглушённые тона, как в Android по умолчанию',
  vibrant: 'Максимум цвета во всех элементах',
};

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
      <button class="icon-btn interactive" data-step="${path}" data-d="-1" data-sz="${step}" data-lo="${min}" data-hi="${Math.min(max, ceil)}" aria-label="Меньше: ${down} ${unit}" ${disabled || down < min ? 'disabled' : ''}>${icon('remove')}</button>
      <span class="v tnum">${v}<small> ${unit}</small></span>
      <button class="icon-btn interactive" data-step="${path}" data-d="1" data-sz="${step}" data-lo="${min}" data-hi="${Math.min(max, ceil)}" aria-label="Больше: ${up} ${unit}" ${disabled || up > Math.min(max, ceil) ? 'disabled' : ''}>${icon('add')}</button>
    </div>`;
  };

  function weekSection(locked, todayIdx) {
    const tiles = WEEKDAYS.map((d, i) => {
      const k = cfg.week[i];
      const kd = KINDS.find((x) => x.id === k);
      return `<button class="day-tile interactive${i === todayIdx ? ' today' : ''}" data-day="${i}" data-kind="${k}"
        data-tip="${esc(`${WEEKDAYS_FULL[i]}${i === todayIdx ? ' (сегодня)' : ''}: ${kd.label.toLowerCase()} день. Нажми, чтобы сменить.`)}">
        <span class="dn">${d}</span>${icon(kd.icon)}<span class="kn">${kd.label}</span></button>`;
    }).join('');
    const profiles = KINDS.map((k) => {
      const p = cfg.profiles[k.id];
      return `<div class="setting profile" data-kind="${k.id}">
        <span class="badge">${icon(k.icon)}</span>
        <div class="grow"><div class="t">${k.label}</div><div class="d ellipsis" title="${esc(planSummary(p.plan))}">${esc(planSummary(p.plan))}</div></div>
        <button class="btn text interactive" data-tpl="${k.id}" ${tipAttr('plan')}>${icon('edit')}План</button>
        <label class="sw-wrap"><span class="sw-label">Блок</span>${sw(`profiles.${k.id}.block`, p.block, `Блокировка: ${k.label}`, 'block')}</label>
      </div>`;
    }).join('');
    return `
      <section class="section">
        <h2>Неделя</h2>
        <div class="surface">
          <div class="setting col">
            <div class="week" role="group" aria-label="Тип каждого дня недели">${tiles}</div>
            <div class="week-legend"><span><i class="full"></i>Полный</span><span><i class="light"></i>Лёгкий</span><span><i></i>Выходной</span>
              <span class="grow"></span><span>${locked ? 'Сегодняшний день уже идёт — изменения для следующих дней' : 'Нажми на день, чтобы выбрать тип'}</span></div>
          </div>
          ${profiles}
        </div>
      </section>`;
  }

  function ago(ts) {
    if (!ts) return 'ещё не выходил на связь';
    const min = Math.floor((Date.now() - ts) / 60000);
    return min < 1 ? 'на связи' : min < 60 ? `был на связи ${min} мин назад` : `был на связи в ${new Date(ts).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })}`;
  }

  function phoneSection(ph) {
    const on = cfg.phone.enabled;
    const addr = ph.address ? `${ph.address}:${ph.port}` : '';
    const state = !on ? 'Выключена' : ph.running ? `Работает${addr ? ` · ${esc(addr)}` : ''} · ПК «${esc(ph.pc_name)}»` : `Не запущена${ph.error ? ': ' + esc(ph.error) : ''}`;
    const devices = ph.devices.map((d) => `<div class="setting">
        <span class="badge">${icon('phone')}</span>
        <div class="grow"><div class="t">${esc(d.name)}</div><div class="d">${ago(d.last_seen)}${d.blocker === true ? ' · блокировка на телефоне включена' : d.blocker === false ? ' · блокировка на телефоне <b>выключена</b>' : ''}</div></div>
        <button class="btn text interactive" data-forget="${esc(d.id)}">Отключить</button></div>`).join('');
    const pin = ph.pin ? `<div class="setting col pin-box">
        <div class="t">PIN для телефона</div>
        <div class="pin tnum" aria-live="polite">${esc(ph.pin.slice(0, 3))} ${esc(ph.pin.slice(3))}</div>
        <div class="d">Открой ClockManage на телефоне в той же сети Wi-Fi, выбери этот ПК и введи PIN. Действует до ${new Date(ph.pin_until).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })}.</div></div>` : '';
    return `<section class="section">
        <h2>Телефон</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">Синхронизация с Android${info('phone')}</div><div class="d">${state}</div></div>${sw('phone.enabled', on, 'Синхронизация с телефоном', 'phone')}</div>
          ${devices}
          ${pin}
          ${on ? `<div class="setting"><div class="grow"><div class="d">Телефон и ПК должны быть в одной сети. Приложения, которые блокируются на телефоне, — на экране «Блокировка».</div></div>
            <button class="btn tonal interactive" id="phone-pin">${icon('add')}Подключить телефон</button></div>` : ''}
        </div>
      </section>`;
  }

  function render() {
    if (!cfg || !last) return;
    const v = last.view;
    const locked = v.lock.base && v.started && v.lock.reason !== 'single';
    const m = last.meta;
    const mcpUrl = m.mcp.url || `http://127.0.0.1:${cfg.mcp_port}/mcp`;
    const cmd = `claude mcp add --transport http clockmanage ${mcpUrl}`;
    const custom = !SWATCHES.some(([c]) => c.toLowerCase() === cfg.appearance.seed.toLowerCase());
    const focusKey = keyOf(document.activeElement);
    const t = cfg.timing;
    box.innerHTML = `
      ${locked ? `<div class="lock-note">${icon('lock')}<div class="body-m">Идёт учебный день. Конец дня можно только сдвинуть позже, перерывы — только сократить. Неделя и шаблоны меняются для следующих дней.</div></div>` : ''}
      ${weekSection(locked, v.weekday)}

      <section class="section">
        <h2>Учебный день</h2>
        <div class="surface">
          <div class="setting">
            <div class="grow"><div class="t">Конец дня${info('dayEnd')}</div><div class="d">Блокировка снимается в это время (МСК)${v.day_end_changed ? ` · сегодня — до ${esc(v.day_end)}${v.day_end_next_day ? ' ночи' : ''}` : ''}</div></div>
            <div class="field"><input type="time" id="dayend" value="${hm(cfg.day_end_min)}" ${locked ? `min="${hm(cfg.day_end_min)}"` : ''} aria-label="Конец дня" ${tipAttr('dayEnd')}></div>
          </div>
          <div class="setting${locked ? ' off' : ''}"><div class="grow"><div class="t">Часть работы${info('seg')}</div><div class="d">1,5 ч = 45 + 45</div></div>
            ${stepper('timing.work_segment_min', { min: 5, max: 240, step: 5, unit: 'мин', label: 'Минут работы в отрезке', disabled: locked })}</div>
          <div class="setting"><div class="grow"><div class="t">Перерыв между частями${info('short')}</div></div>
            ${stepper('timing.short_break_min', { min: 1, max: 120, step: 5, unit: 'мин', label: 'Минут перерыва', ceil: locked ? t.short_break_min : Infinity })}</div>
          <div class="setting"><div class="grow"><div class="t">Перерыв между блоками${info('between')}</div></div>
            ${stepper('timing.between_blocks_min', { min: 1, max: 180, step: 5, unit: 'мин', label: 'Минут между блоками', ceil: locked ? t.between_blocks_min : Infinity })}</div>
        </div>
      </section>

      <section class="section">
        <h2>Отрезки</h2>
        <div class="surface">
          ${cfg.segments.map((sg, i) => `<div class="setting seg-type">
            <span class="badge">${icon(sg.alarm ? 'alarm' : 'coffee')}</span>
            <div class="grow"><input class="seg-name" data-segname="${i}" value="${esc(sg.name)}" maxlength="24" aria-label="Название отрезка"></div>
            ${stepper(`segments.${i}.minutes`, { min: 1, max: 240, step: 5, unit: 'мин', label: `Минут: ${sg.name}`, ceil: locked ? sg.minutes : Infinity })}
            <label class="sw-wrap"><span class="sw-label">Будильник</span>${sw(`segments.${i}.alarm`, sg.alarm, `Будильник: ${sg.name}`, 'segAlarm')}</label>
            <label class="sw-wrap"><span class="sw-label">Доступ</span>${sw(`segments.${i}.open_access`, sg.open_access, `Доступ на время: ${sg.name}`, 'segAccess', locked && !sg.open_access)}</label>
            <button class="icon-btn interactive" data-segdel="${i}" aria-label="Удалить ${esc(sg.name)}" ${cfg.segments.length <= 1 ? 'disabled' : ''}>${icon('delete')}</button>
          </div>`).join('')}
          <div class="setting"><div class="grow"><div class="d">Обед, сон, прогулка — неучебное время с обратным отсчётом${info('segments')} Свой тип: «Душ 10 мин». В план дня отрезки добавляются на экране «План».</div></div>
            <button class="btn tonal interactive" data-segadd ${cfg.segments.length >= 10 ? 'disabled' : ''}>${icon('add')}Добавить тип</button></div>
        </div>
      </section>

      <section class="section">
        <h2>Звук и оповещения</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">Звук</div><div class="d">Конец работы, перерыва и блока</div></div>${sw('sound', cfg.sound, 'Звук', 'sound')}</div>
          <div class="setting"><div class="grow"><div class="t">Карточки поверх экрана</div><div class="d">Будильник, перерыв, «Не-не-не»</div></div>${sw('overlay', cfg.overlay, 'Карточки поверх экрана', 'overlay')}</div>
          <div class="setting"><div class="grow"><div class="t">Повторять звонок каждые${info('reminder')}</div></div>
            ${stepper('reminder_sec', { min: 15, max: 600, step: 15, unit: 'сек', label: 'Секунд между напоминаниями' })}</div>
          <div class="setting col">
            <div class="hstack"><span class="t grow">Проверить</span>
              <button class="btn tonal interactive" data-sound="alarm" data-tip="Звонок конца перерыва">${icon('alarm')}Звонок</button>
              <button class="btn tonal interactive" data-sound="break" data-tip="Сигнал конца работы">${icon('coffee')}Перерыв</button>
              <button class="btn tonal interactive" data-sound="done" data-tip="Закрытие блока">${icon('check')}Блок</button>
            </div>
            <div class="hstack"><span class="t grow">Показать карточку</span>
              <button class="btn outlined interactive" data-ov="await">Звонок</button>
              <button class="btn outlined interactive" data-ov="break">Перерыв</button>
              <button class="btn outlined interactive" data-ov="block">Блок</button>
              <button class="btn outlined interactive" data-ov="nope">Не-не-не</button>
            </div>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>Оформление</h2>
        <div class="surface">
          <div class="setting col">
            <div class="t">Основной цвет</div>
            <div class="swatches">${SWATCHES.map(([c, n]) => {
              const on = c.toLowerCase() === cfg.appearance.seed.toLowerCase();
              return `<button class="swatch" style="--c:${c};--on-c:${onColor(c)}" data-seed="${c}" aria-label="${n}" data-tip="${n}" aria-pressed="${on}">${on ? CHECK : ''}</button>`;
            }).join('')}
              <label class="swatch" style="--c:${custom ? esc(cfg.appearance.seed) : 'conic-gradient(red, yellow, lime, cyan, blue, magenta, red)'};background:var(--c);--on-c:${custom ? onColor(cfg.appearance.seed) : '#fff'}" aria-pressed="${custom}" data-tip="Свой цвет">
                ${custom ? CHECK : ''}<input type="color" id="seed-in" value="${esc(cfg.appearance.seed)}" style="position:absolute;inset:0;opacity:0;width:100%;height:100%;cursor:pointer" aria-label="Свой цвет"></label>
            </div>
          </div>
          <div class="setting"><div class="grow"><div class="t">Тема</div></div>${seg('mode', cfg.appearance.mode, [['system', 'Системная'], ['light', 'Светлая'], ['dark', 'Тёмная']], MODE_TIP)}</div>
          <div class="setting"><div class="grow"><div class="t">Насыщенность</div></div>${seg('variant', cfg.appearance.variant, [['fidelity', 'Точная'], ['tonal_spot', 'Спокойная'], ['vibrant', 'Яркая']], VARIANT_TIP)}</div>
          <div class="setting"><div class="grow"><div class="t">Контрастный мини-таймер</div><div class="d">Инверсные цвета — виден на любом фоне</div></div>${sw('appearance.mini_contrast', cfg.appearance.mini_contrast, 'Контрастный мини-таймер', 'contrast')}</div>
        </div>
      </section>

      ${phoneSection(m.phone)}

      <section class="section">
        <h2>MCP для Claude</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">Локальный MCP-сервер</div>
            <div class="d">${m.mcp.running ? `Работает на 127.0.0.1:${m.mcp.port}` : m.mcp.enabled ? `Не запущен${m.mcp.error ? ': ' + esc(m.mcp.error) : ''}` : 'Выключен'}. Инструменты: set_plan, set_day_end, get_plan, get_session_state, get_today_stats.</div></div>${sw('mcp_enabled', cfg.mcp_enabled, 'MCP-сервер', 'mcp')}</div>
          <div class="setting col">
            <div class="t">Адрес</div>
            <div class="hstack"><code class="code">${esc(mcpUrl)}</code><button class="icon-btn interactive" data-copy="${esc(mcpUrl)}" aria-label="Скопировать адрес">${icon('copy')}</button></div>
            <div class="t" style="margin-top:8px">Команда для Claude Code</div>
            <div class="hstack"><code class="code">${esc(cmd)}</code><button class="icon-btn interactive" data-copy="${esc(cmd)}" aria-label="Скопировать команду">${icon('copy')}</button></div>
            <div><button class="btn text interactive" id="newport" data-tip="Если порт занят другой программой. Адрес в Claude после этого надо обновить.">${icon('refresh')}Новый случайный порт</button></div>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>Система</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">Автозапуск и самовосстановление</div><div class="d">Старт с Windows, подъём после закрытия</div></div>${sw('autostart', cfg.autostart, 'Автозапуск', 'autostart', locked && cfg.autostart)}</div>
          <div class="setting"><div class="grow"><div class="t">Права администратора</div><div class="d">${m.admin ? 'Есть — блокировка работает полностью' : 'Нет — hosts, политики браузеров и закрытие приложений недоступны'}</div></div>${icon(m.admin ? 'shield' : 'warning')}</div>
          <div class="setting"><div class="grow"><div class="t">Данные и журнал</div><div class="d ellipsis" data-tip="${esc(m.data_dir)}">${esc(m.data_dir)}</div></div><button class="btn text interactive" id="opendir">${icon('folder')}Открыть</button></div>
          <div class="setting"><div class="grow"><div class="t">Версия</div><div class="d">ClockManage ${esc(m.version)}</div></div></div>
        </div>
      </section>`;
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
      sub: `${planSummary(cfg.profiles[k.id].plan)}${cfg.profiles[k.id].block ? '' : ' · без блокировки'}`,
    })), { title: WEEKDAYS_FULL[i][0].toUpperCase() + WEEKDAYS_FULL[i].slice(1) });
    if (!kind || kind === cur) return;
    pulse = `[data-day="${i}"]`;
    const ok = await save((c) => { c.week[i] = kind; });
    if (ok && i === last.view.weekday && !last.view.started) snack(`Сегодня — ${kindLabel(kind).toLowerCase()} день, план обновлён`);
  }

  box.addEventListener('change', (e) => {
    const t = e.target;
    if (t.dataset.sw) {
      pulse = `[data-sw="${t.dataset.sw}"]`;
      save((c) => setPath(c, t.dataset.sw, t.checked));
    } else if (t.dataset.segname !== undefined) {
      const i = Number(t.dataset.segname);
      const name = t.value.trim();
      if (name) save((c) => { c.segments[i].name = name; });
      else render();
    } else if (t.id === 'dayend') {
      const [h, mm] = t.value.split(':').map(Number);
      if (Number.isFinite(h)) save((c) => { c.day_end_min = h * 60 + (mm || 0); });
    } else if (t.id === 'seed-in') {
      save((c) => { c.appearance.seed = t.value; });
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
    } else if (t.dataset.sound) call('test_sound', { kind: t.dataset.sound });
    else if (t.dataset.ov) call('preview_overlay', { kind: t.dataset.ov });
    else if (t.dataset.copy) {
      try { await navigator.clipboard.writeText(t.dataset.copy); snack('Скопировано'); } catch { snack('Не удалось скопировать'); }
    } else if (t.id === 'newport') {
      const port = await run(() => call('regenerate_port'), t);
      if (port) { cfg = await call('get_config'); snack(`Новый порт: ${port}. Обнови адрес в Claude.`); }
    } else if (t.id === 'opendir') call('open_data_dir');
    else if (t.id === 'phone-pin') await run(() => call('phone_pin'), t);
    else if (t.dataset.segdel !== undefined) {
      const i = Number(t.dataset.segdel);
      save((c) => { c.segments.splice(i, 1); });
    } else if (t.dataset.segadd !== undefined) {
      save((c) => {
        let name = 'Свой';
        for (let n = 2; c.segments.some((x) => x.name.toLowerCase() === name.toLowerCase()); n++) name = `Свой ${n}`;
        c.segments.push({ name, minutes: 15, alarm: false, open_access: false });
      }).then(() => box.querySelector('.seg-type:last-of-type .seg-name')?.select());
    }
    else if (t.dataset.forget) {
      const ok = await run(() => call('phone_forget', { id: t.dataset.forget }).then(() => true), t);
      if (ok) snack('Телефон отключён — подключить снова можно по новому PIN');
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
      const nsig = `${v.lock.base}|${v.started}|${v.day_end}|${v.kind}|${JSON.stringify(s.meta.mcp)}|${s.meta.admin}|${JSON.stringify(ph)}`;
      if (nsig !== sig && cfg) { sig = nsig; render(); }
    },
    show() { reload(); },
    config() { reload(); },
  };
}

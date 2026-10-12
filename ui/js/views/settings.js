import { call, esc } from '../api.js';
import { icon, CHECK } from '../icons.js';
import { applyTheme } from '../theme.js';
import { run, snack, menu, WEEKDAYS, WEEKDAYS_FULL, KINDS, kindLabel, planSummary, hm } from '../ui.js';
import { L, LOCALE, dec } from '../i18n.js';

const SWATCHES = [
  ['#2E7D32', L('Зелёный', 'Green')], ['#00796B', L('Бирюзовый', 'Teal')], ['#1565C0', L('Синий', 'Blue')], ['#5E35B1', L('Фиолетовый', 'Purple')],
  ['#C2185B', L('Малиновый', 'Raspberry')], ['#E65100', L('Оранжевый', 'Orange')], ['#6D4C41', L('Коричневый', 'Brown')],
];

// Longer explanations live in tooltips so the screen itself stays short.
const TIP = {
  block: L('В такой день «Начать день» закрывает сайты и приложения из списка — до конца дня или пока не закроешь весь план.', 'On such a day “Start day” closes the listed sites and apps — until the day end or until the whole plan is done.'),
  plan: L('С этого плана начинается каждый такой день. Сегодняшний план правится на экране «План».', 'Every such day starts with this plan. Today’s plan is edited on the Plan screen.'),
  dayEnd: L('Во сколько блокировка снимается сама, даже если план не закрыт. Во время учёбы это значение по умолчанию можно только сдвинуть позже.\nСдвинуть только сегодняшний день (позже или раньше, до 02:00 ночи) — нажми на время на экране «Сегодня».', 'When the lock lifts by itself, even if the plan isn’t done. During study this default can only move later.\nTo move just today (later or earlier, up to 02:00 at night), click the time on the Today screen.'),
  seg: L('Блок режется на части такой длины. Хвост короче 15 мин приклеивается к последней части: 50 мин — это одна часть.', 'A block is cut into parts of this length. A tail shorter than 15 min sticks to the last part: 50 min is one part.'),
  short: L('Отдых между частями одного блока. Не съедает учебное время. Во время учёбы его можно только сократить.', 'Rest between the parts of one block. It doesn’t eat into study time. During study it can only get shorter.'),
  between: L('Отдых после закрытого блока, перед следующим. Во время учёбы — только короче.', 'Rest after a closed block, before the next one. During study — only shorter.'),
  segments: L('Неучебные отрезки с обратным отсчётом: кнопка «Отрезок» в перерыве или пункт плана. За 5 минут и в конце — уведомление, потом каждые 5 минут — на сколько превышено. При старте можно выбрать «Без времени» — секундомер вверх. Блокировка — как на перерыве. Во время учёбы — только короче.', 'Non-study segments with a countdown: the Segment button on a break, or a plan item. A notice 5 minutes before and at the end, then every 5 minutes — how much over. At the start you can pick “Untimed” — a stopwatch counting up. The lock works as on a break. During study — only shorter.'),
  segPrep: L('Время собраться перед отсчётом: кофе, дойти до кровати. Отрезок ждёт кнопку «Лёг», а когда подготовка выходит — только напоминает. Отсчёт и будильник идут от «Лёг». 0 — отсчёт сразу.', 'Time to get ready before the countdown: coffee, getting to bed. The segment waits for “In bed”, and when the time is up it only reminds you. The countdown and the alarm start from “In bed”. 0 — the countdown starts at once.'),
  segAlarm: L('Конец — громкий будильник, который звонит, пока не нажмёшь «Встал» (даже если звук выключен). Без предупреждения за 5 минут. Для сна.', 'The end is a loud alarm that rings until you press “I’m up” (even with the sound off). No 5-minute warning. For naps.'),
  segAccess: L('На время отрезка (до его планового конца) заблокированное открыто — как «ем за ПК». Включается только вне учёбы.', 'While the segment runs (until its planned end), blocked things are open — like lunch at the PC. Can only be turned on outside study.'),
  sound: L('Звонок на конце перерыва, мягкий сигнал на конце работы и фанфары на закрытии блока.', 'A ring at the end of a break, a soft signal at the end of work and a fanfare when a block closes.'),
  overlay: L('Рисованная карточка поверх всех окон на каждом переходе: будильник, перерыв, закрытый блок, «Не-не-не».', 'A hand-drawn card on top of all windows at every step: alarm, break, closed block, “Nope”.'),
  reminder: L('Пока таймер ждёт «Начать часть», звонок повторяется с этим интервалом. После конца дня — молчит.', 'While the timer waits for “Start part”, the ring repeats at this interval. After the day end it stays quiet.'),
  contrast: L('Инверсные цвета: тёмная плашка в светлой теме и светлая в тёмной. Не теряется ни на каком фоне.', 'Inverted colours: a dark pill in the light theme and a light one in the dark. Never gets lost on any background.'),
  phone: L('Приложение ClockManage для Android видит таймер по Wi-Fi, может ставить паузу, начинать части и менять длину блоков. Без связи продолжает считать само. Сайты из блок-листа и выбранные приложения блокирует и на телефоне.', 'The ClockManage Android app sees the timer over Wi-Fi, can pause, start parts and change block lengths. Offline it keeps counting on its own. It blocks the listed sites and the chosen apps on the phone too.'),
  mcp: L('Локальный сервер для Claude: видит состояние таймера и статистику, может задать план. Слушает только 127.0.0.1.', 'A local server for Claude: it sees the timer state and the stats and can set the plan. Listens on 127.0.0.1 only.'),
  autostart: L('Запуск при входе в Windows без окна UAC и перезапуск раз в 5 минут, если процесс убили. Во время учёбы выключить нельзя.', 'Starts when you sign in to Windows without a UAC prompt, and restarts every 5 minutes if the process was killed. Can’t be turned off during study.'),
  lang: L('Язык интерфейса, уведомлений и записей в журнале. «Авто» — как в Windows.', 'The language of the interface, notifications and log entries. “Auto” follows Windows.'),
  tz: L('По этим часам считаются конец дня, даты и журнал. Меняется только вне блокировки.', 'The day end, dates and the log follow this clock. Can only be changed outside the lock.'),
};
const MODE_TIP = { system: L('Как в Windows', 'Like Windows'), light: L('Всегда светлая', 'Always light'), dark: L('Всегда тёмная', 'Always dark') };
const VARIANT_TIP = {
  fidelity: L('Цвет ровно как выбран — самые заметные акценты', 'The colour exactly as picked — the boldest accents'),
  tonal_spot: L('Приглушённые тона, как в Android по умолчанию', 'Muted tones, like Android’s default'),
  vibrant: L('Максимум цвета во всех элементах', 'Maximum colour everywhere'),
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
      <button class="icon-btn interactive" data-step="${path}" data-d="-1" data-sz="${step}" data-lo="${min}" data-hi="${Math.min(max, ceil)}" aria-label="${L('Меньше', 'Less')}: ${down} ${unit}" ${disabled || down < min ? 'disabled' : ''}>${icon('remove')}</button>
      <span class="v tnum">${v}<small> ${unit}</small></span>
      <button class="icon-btn interactive" data-step="${path}" data-d="1" data-sz="${step}" data-lo="${min}" data-hi="${Math.min(max, ceil)}" aria-label="${L('Больше', 'More')}: ${up} ${unit}" ${disabled || up > Math.min(max, ceil) ? 'disabled' : ''}>${icon('add')}</button>
    </div>`;
  };

  function weekSection(locked, todayIdx) {
    const tiles = WEEKDAYS.map((d, i) => {
      const k = cfg.week[i];
      const kd = KINDS.find((x) => x.id === k);
      return `<button class="day-tile interactive${i === todayIdx ? ' today' : ''}" data-day="${i}" data-kind="${k}"
        data-tip="${esc(L(`${WEEKDAYS_FULL[i]}${i === todayIdx ? ' (сегодня)' : ''}: ${kd.label.toLowerCase()} день. Нажми, чтобы сменить.`, `${WEEKDAYS_FULL[i]}${i === todayIdx ? ' (today)' : ''}: ${kd.label.toLowerCase()} day. Click to change.`))}">
        <span class="dn">${d}</span>${icon(kd.icon)}<span class="kn">${kd.label}</span></button>`;
    }).join('');
    const profiles = KINDS.map((k) => {
      const p = cfg.profiles[k.id];
      return `<div class="setting profile" data-kind="${k.id}">
        <span class="badge">${icon(k.icon)}</span>
        <div class="grow"><div class="t">${k.label}</div><div class="d ellipsis" title="${esc(planSummary(p.plan))}">${esc(planSummary(p.plan))}</div></div>
        <button class="btn text interactive" data-tpl="${k.id}" ${tipAttr('plan')}>${icon('edit')}${L('План', 'Plan')}</button>
        <label class="sw-wrap"><span class="sw-label">${L('Блок', 'Block')}</span>${sw(`profiles.${k.id}.block`, p.block, `${L('Блокировка', 'Lock')}: ${k.label}`, 'block')}</label>
      </div>`;
    }).join('');
    return `
      <section class="section">
        <h2>${L('Неделя', 'Week')}</h2>
        <div class="surface">
          <div class="setting col">
            <div class="week" role="group" aria-label="${L('Тип каждого дня недели', 'The type of each weekday')}">${tiles}</div>
            <div class="week-legend"><span><i class="full"></i>${L('Полный', 'Full')}</span><span><i class="light"></i>${L('Лёгкий', 'Light')}</span><span><i></i>${L('Выходной', 'Day off')}</span>
              <span class="grow"></span><span>${locked ? L('Сегодняшний день уже идёт — изменения для следующих дней', 'Today is running — changes apply to the next days') : L('Нажми на день, чтобы выбрать тип', 'Click a day to pick its type')}</span></div>
          </div>
          ${profiles}
        </div>
      </section>`;
  }

  function ago(ts) {
    if (!ts) return L('ещё не выходил на связь', 'hasn’t connected yet');
    const min = Math.floor((Date.now() - ts) / 60000);
    return min < 1 ? L('на связи', 'online') : min < 60 ? L(`был на связи ${min} мин назад`, `last seen ${min} min ago`) : `${L('был на связи в', 'last seen at')} ${new Date(ts).toLocaleTimeString(LOCALE, { hour: '2-digit', minute: '2-digit' })}`;
  }

  function phoneSection(ph) {
    const on = cfg.phone.enabled;
    const addr = ph.address ? `${ph.address}:${ph.port}` : '';
    const state = !on ? L('Выключена', 'Off') : ph.running ? `${L('Работает', 'Running')}${addr ? ` · ${esc(addr)}` : ''} · ${L('ПК', 'PC')} «${esc(ph.pc_name)}»` : `${L('Не запущена', 'Not running')}${ph.error ? ': ' + esc(ph.error) : ''}`;
    const devices = ph.devices.map((d) => `<div class="setting">
        <span class="badge">${icon('phone')}</span>
        <div class="grow"><div class="t">${esc(d.name)}</div><div class="d">${ago(d.last_seen)}${d.blocker === true ? L(' · блокировка на телефоне включена', ' · the phone lock is on') : d.blocker === false ? ` · ${L('блокировка на телефоне', 'the phone lock is')} <b>${L('выключена', 'off')}</b>` : ''}</div></div>
        <button class="btn text interactive" data-forget="${esc(d.id)}">${L('Отключить', 'Unpair')}</button></div>`).join('');
    const pin = ph.pin ? `<div class="setting col pin-box">
        <div class="t">${L('PIN для телефона', 'PIN for the phone')}</div>
        <div class="pin tnum" aria-live="polite">${esc(ph.pin.slice(0, 3))} ${esc(ph.pin.slice(3))}</div>
        <div class="d">${L('Открой ClockManage на телефоне в той же сети Wi-Fi, выбери этот ПК и введи PIN. Действует до', 'Open ClockManage on the phone in the same Wi-Fi network, pick this PC and enter the PIN. Valid until')} ${new Date(ph.pin_until).toLocaleTimeString(LOCALE, { hour: '2-digit', minute: '2-digit' })}.</div></div>` : '';
    return `<section class="section">
        <h2>${L('Телефон', 'Phone')}</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">${L('Синхронизация с Android', 'Sync with Android')}${info('phone')}</div><div class="d">${state}</div></div>${sw('phone.enabled', on, L('Синхронизация с телефоном', 'Phone sync'), 'phone')}</div>
          ${devices}
          ${pin}
          ${on && ph.apk?.available ? `<div class="setting"><div class="grow"><div class="t">${L('Приложение для телефона', 'Phone app')}</div><div class="d">${L('В этой сборке — версия', 'This build includes version')} ${esc(ph.apk.name)} (${dec((ph.apk.size / 1048576).toFixed(1))} ${L('МБ). Подключённый телефон предложит обновиться сам, по Wi-Fi.', 'MB). A paired phone offers the update by itself, over Wi-Fi.')}</div></div></div>` : ''}
          ${on ? `<div class="setting"><div class="grow"><div class="d">${L('Телефон и ПК должны быть в одной сети. Приложения, которые блокируются на телефоне, — на экране «Блокировка».', 'The phone and the PC must be on the same network. Apps blocked on the phone are set on the Blocking screen.')}</div></div>
            <button class="btn tonal interactive" id="phone-pin">${icon('add')}${L('Подключить телефон', 'Pair a phone')}</button></div>` : ''}
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
      ${locked ? `<div class="lock-note">${icon('lock')}<div class="body-m">${L('Идёт учебный день. Конец дня можно только сдвинуть позже, перерывы — только сократить. Неделя и шаблоны меняются для следующих дней.', 'A study day is running. The day end can only move later, breaks can only get shorter. The week and the templates change for the next days.')}</div></div>` : ''}
      ${weekSection(locked, v.weekday)}

      <section class="section">
        <h2>${L('Учебный день', 'Study day')}</h2>
        <div class="surface">
          <div class="setting">
            <div class="grow"><div class="t">${L('Конец дня', 'Day end')}${info('dayEnd')}</div><div class="d">${L('Блокировка снимается в это время', 'The lock lifts at this time')} (${utcLabel(cfg.tz_offset_min)})${v.day_end_changed ? ` · ${L('сегодня — до', 'today — until')} ${esc(v.day_end)}${v.day_end_next_day ? L(' ночи', ' (night)') : ''}` : ''}</div></div>
            <div class="field"><input type="time" id="dayend" value="${hm(cfg.day_end_min)}" ${locked ? `min="${hm(cfg.day_end_min)}"` : ''} aria-label="${L('Конец дня', 'Day end')}" ${tipAttr('dayEnd')}></div>
          </div>
          <div class="setting${locked ? ' off' : ''}"><div class="grow"><div class="t">${L('Часть работы', 'Work part')}${info('seg')}</div><div class="d">${L('1,5 ч = 45 + 45', '1.5 h = 45 + 45')}</div></div>
            ${stepper('timing.work_segment_min', { min: 5, max: 240, step: 5, unit: L('мин', 'min'), label: L('Минут работы в части', 'Minutes of work per part'), disabled: locked })}</div>
          <div class="setting"><div class="grow"><div class="t">${L('Перерыв между частями', 'Break between parts')}${info('short')}</div></div>
            ${stepper('timing.short_break_min', { min: 1, max: 120, step: 5, unit: L('мин', 'min'), label: L('Минут перерыва', 'Break minutes'), ceil: locked ? t.short_break_min : Infinity })}</div>
          <div class="setting"><div class="grow"><div class="t">${L('Перерыв между блоками', 'Break between blocks')}${info('between')}</div></div>
            ${stepper('timing.between_blocks_min', { min: 1, max: 180, step: 5, unit: L('мин', 'min'), label: L('Минут между блоками', 'Minutes between blocks'), ceil: locked ? t.between_blocks_min : Infinity })}</div>
        </div>
      </section>

      <section class="section">
        <h2>${L('Отрезки', 'Segments')}</h2>
        <div class="surface">
          ${cfg.segments.map((sg, i) => `<div class="setting seg-type">
            <span class="badge">${icon(sg.alarm ? 'alarm' : 'coffee')}</span>
            <div class="grow"><input class="seg-name" data-segname="${i}" value="${esc(sg.name)}" maxlength="24" aria-label="${L('Название отрезка', 'Segment name')}"></div>
            ${stepper(`segments.${i}.minutes`, { min: 1, max: 240, step: 5, unit: L('мин', 'min'), label: `${L('Минут', 'Minutes')}: ${sg.name}`, ceil: locked ? sg.minutes : Infinity })}
            <div class="sw-wrap prep-wrap" ${tipAttr('segPrep')}><span class="sw-label">${L('Подготовка', 'Getting ready')}</span>${stepper(`segments.${i}.prep_min`, { min: 0, max: 60, step: 5, unit: L('мин', 'min'), label: `${L('Подготовка', 'Getting ready')}: ${sg.name}`, ceil: locked ? (sg.prep_min ?? 0) : Infinity })}</div>
            <label class="sw-wrap"><span class="sw-label">${L('Будильник', 'Alarm')}</span>${sw(`segments.${i}.alarm`, sg.alarm, `${L('Будильник', 'Alarm')}: ${sg.name}`, 'segAlarm')}</label>
            <label class="sw-wrap"><span class="sw-label">${L('Доступ', 'Access')}</span>${sw(`segments.${i}.open_access`, sg.open_access, `${L('Доступ на время', 'Access during')}: ${sg.name}`, 'segAccess', locked && !sg.open_access)}</label>
            <button class="icon-btn interactive" data-segdel="${i}" aria-label="${L('Удалить', 'Remove')} ${esc(sg.name)}" ${cfg.segments.length <= 1 ? 'disabled' : ''}>${icon('delete')}</button>
          </div>`).join('')}
          <div class="setting seg-add"><div class="grow"><div class="d">${L('Обед, сон, прогулка — неучебное время с обратным отсчётом', 'Lunch, a nap, a walk — non-study time with a countdown')}${info('segments')}${L('. Свой тип: «Душ 10 мин». В план дня отрезки добавляются на экране «План».', '. Your own type: “Shower 10 min”. Segments are added to the day on the Plan screen.')}</div></div>
            <button class="btn tonal interactive" data-segadd ${cfg.segments.length >= 10 ? 'disabled' : ''}>${icon('add')}${L('Добавить тип', 'Add type')}</button></div>
        </div>
      </section>

      <section class="section">
        <h2>${L('Звук и оповещения', 'Sound and alerts')}</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">${L('Звук', 'Sound')}</div><div class="d">${L('Конец работы, перерыва и блока', 'End of work, a break and a block')}</div></div>${sw('sound', cfg.sound, L('Звук', 'Sound'), 'sound')}</div>
          <div class="setting"><div class="grow"><div class="t">${L('Карточки поверх экрана', 'Cards on top of the screen')}</div><div class="d">${L('Будильник, перерыв, «Не-не-не»', 'Alarm, break, “Nope”')}</div></div>${sw('overlay', cfg.overlay, L('Карточки поверх экрана', 'Cards on top of the screen'), 'overlay')}</div>
          <div class="setting"><div class="grow"><div class="t">${L('Повторять звонок каждые', 'Repeat the ring every')}${info('reminder')}</div></div>
            ${stepper('reminder_sec', { min: 15, max: 600, step: 15, unit: L('сек', 's'), label: L('Секунд между напоминаниями', 'Seconds between reminders') })}</div>
          <div class="setting col">
            <div class="hstack"><span class="t grow">${L('Проверить', 'Test')}</span>
              <button class="btn tonal interactive" data-sound="alarm" data-tip="${L('Звонок конца перерыва', 'End-of-break ring')}">${icon('alarm')}${L('Звонок', 'Ring')}</button>
              <button class="btn tonal interactive" data-sound="break" data-tip="${L('Сигнал конца работы', 'End-of-work signal')}">${icon('coffee')}${L('Перерыв', 'Break')}</button>
              <button class="btn tonal interactive" data-sound="done" data-tip="${L('Закрытие блока', 'Block closed')}">${icon('check')}${L('Блок', 'Block')}</button>
            </div>
            <div class="hstack"><span class="t grow">${L('Показать карточку', 'Show the card')}</span>
              <button class="btn outlined interactive" data-ov="await">${L('Звонок', 'Ring')}</button>
              <button class="btn outlined interactive" data-ov="break">${L('Перерыв', 'Break')}</button>
              <button class="btn outlined interactive" data-ov="block">${L('Блок', 'Block')}</button>
              <button class="btn outlined interactive" data-ov="nope">${L('Не-не-не', 'Nope')}</button>
            </div>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>${L('Оформление', 'Appearance')}</h2>
        <div class="surface">
          <div class="setting col">
            <div class="t">${L('Основной цвет', 'Main colour')}</div>
            <div class="swatches">${SWATCHES.map(([c, n]) => {
              const on = c.toLowerCase() === cfg.appearance.seed.toLowerCase();
              return `<button class="swatch" style="--c:${c};--on-c:${onColor(c)}" data-seed="${c}" aria-label="${n}" data-tip="${n}" aria-pressed="${on}">${on ? CHECK : ''}</button>`;
            }).join('')}
              <label class="swatch" style="--c:${custom ? esc(cfg.appearance.seed) : 'conic-gradient(red, yellow, lime, cyan, blue, magenta, red)'};background:var(--c);--on-c:${custom ? onColor(cfg.appearance.seed) : '#fff'}" aria-pressed="${custom}" data-tip="${L('Свой цвет', 'Custom colour')}">
                ${custom ? CHECK : ''}<input type="color" id="seed-in" value="${esc(cfg.appearance.seed)}" style="position:absolute;inset:0;opacity:0;width:100%;height:100%;cursor:pointer" aria-label="${L('Свой цвет', 'Custom colour')}"></label>
            </div>
          </div>
          <div class="setting"><div class="grow"><div class="t">${L('Тема', 'Theme')}</div></div>${seg('mode', cfg.appearance.mode, [['system', L('Системная', 'System')], ['light', L('Светлая', 'Light')], ['dark', L('Тёмная', 'Dark')]], MODE_TIP)}</div>
          <div class="setting"><div class="grow"><div class="t">${L('Насыщенность', 'Colourfulness')}</div></div>${seg('variant', cfg.appearance.variant, [['fidelity', L('Точная', 'Exact')], ['tonal_spot', L('Спокойная', 'Calm')], ['vibrant', L('Яркая', 'Vibrant')]], VARIANT_TIP)}</div>
          <div class="setting"><div class="grow"><div class="t">${L('Контрастный мини-таймер', 'Contrast mini timer')}</div><div class="d">${L('Инверсные цвета — виден на любом фоне', 'Inverted colours — visible on any background')}</div></div>${sw('appearance.mini_contrast', cfg.appearance.mini_contrast, L('Контрастный мини-таймер', 'Contrast mini timer'), 'contrast')}</div>
        </div>
      </section>

      ${phoneSection(m.phone)}

      <section class="section">
        <h2>${L('MCP для Claude', 'MCP for Claude')}</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">${L('Локальный MCP-сервер', 'Local MCP server')}</div>
            <div class="d">${m.mcp.running ? `${L('Работает на', 'Running on')} 127.0.0.1:${m.mcp.port}` : m.mcp.enabled ? `${L('Не запущен', 'Not running')}${m.mcp.error ? ': ' + esc(m.mcp.error) : ''}` : L('Выключен', 'Off')}. ${L('Инструменты', 'Tools')}: set_plan, set_day_end, get_plan, get_session_state, get_today_stats, get_blocklist, add_to_blocklist, remove_from_blocklist, start_focus_lock.</div></div>${sw('mcp_enabled', cfg.mcp_enabled, L('MCP-сервер', 'MCP server'), 'mcp')}</div>
          <div class="setting col">
            <div class="t">${L('Адрес', 'Address')}</div>
            <div class="hstack"><code class="code">${esc(mcpUrl)}</code><button class="icon-btn interactive" data-copy="${esc(mcpUrl)}" aria-label="${L('Скопировать адрес', 'Copy the address')}">${icon('copy')}</button></div>
            <div class="t" style="margin-top:8px">${L('Команда для Claude Code', 'Command for Claude Code')}</div>
            <div class="hstack"><code class="code">${esc(cmd)}</code><button class="icon-btn interactive" data-copy="${esc(cmd)}" aria-label="${L('Скопировать команду', 'Copy the command')}">${icon('copy')}</button></div>
            <div><button class="btn text interactive" id="newport" data-tip="${L('Если порт занят другой программой. Адрес в Claude после этого надо обновить.', 'If another program uses the port. Update the address in Claude afterwards.')}">${icon('refresh')}${L('Новый случайный порт', 'New random port')}</button></div>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>${L('Система', 'System')}</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">${L('Язык', 'Language')}${info('lang')}</div></div>${seg('lang', cfg.lang || 'auto', [['auto', L('Авто', 'Auto')], ['ru', 'Русский'], ['en', 'English']])}</div>
          <div class="setting${locked ? ' off' : ''}"><div class="grow"><div class="t">${L('Часовой пояс', 'Time zone')}${info('tz')}</div></div>
            <div class="field"><select id="tz" aria-label="${L('Часовой пояс', 'Time zone')}" ${locked ? 'disabled' : ''}>${[...new Set([...TZ_OFFSETS, cfg.tz_offset_min])].sort((x, y) => x - y).map((o) => `<option value="${o}" ${o === cfg.tz_offset_min ? 'selected' : ''}>${utcLabel(o)}</option>`).join('')}</select></div></div>
          <div class="setting"><div class="grow"><div class="t">${L('Автозапуск и самовосстановление', 'Autostart and self-healing')}</div><div class="d">${L('Старт с Windows, подъём после закрытия', 'Starts with Windows, comes back after being closed')}</div></div>${sw('autostart', cfg.autostart, L('Автозапуск', 'Autostart'), 'autostart', locked && cfg.autostart)}</div>
          <div class="setting"><div class="grow"><div class="t">${L('Права администратора', 'Administrator rights')}</div><div class="d">${m.admin ? L('Есть — блокировка работает полностью', 'Yes — the lock works fully') : L('Нет — hosts, политики браузеров и закрытие приложений недоступны', 'No — hosts, browser policies and closing apps are unavailable')}</div></div>${icon(m.admin ? 'shield' : 'warning')}</div>
          <div class="setting"><div class="grow"><div class="t">${L('Данные и журнал', 'Data and log')}</div><div class="d ellipsis" data-tip="${esc(m.data_dir)}">${esc(m.data_dir)}</div></div><button class="btn text interactive" id="opendir">${icon('folder')}${L('Открыть', 'Open')}</button></div>
          <div class="setting"><div class="grow"><div class="t">${L('Версия', 'Version')}</div><div class="d">ClockManage ${esc(m.version)}</div></div></div>
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
      sub: `${planSummary(cfg.profiles[k.id].plan)}${cfg.profiles[k.id].block ? '' : L(' · без блокировки', ' · no lock')}`,
    })), { title: WEEKDAYS_FULL[i][0].toUpperCase() + WEEKDAYS_FULL[i].slice(1) });
    if (!kind || kind === cur) return;
    pulse = `[data-day="${i}"]`;
    const ok = await save((c) => { c.week[i] = kind; });
    if (ok && i === last.view.weekday && !last.view.started) snack(L(`Сегодня — ${kindLabel(kind).toLowerCase()} день, план обновлён`, `Today is a ${kindLabel(kind).toLowerCase()} day, plan updated`));
  }

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
        snack(L(`«${name}» уже есть — выбери другое название`, `“${name}” already exists — pick another name`));
        render();
      } else if (name) save((c) => { c.segments[i].name = name; });
      else render();
    } else if (t.id === 'dayend') {
      const [h, mm] = t.value.split(':').map(Number);
      if (Number.isFinite(h)) save((c) => { c.day_end_min = h * 60 + (mm || 0); });
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
      try { await navigator.clipboard.writeText(t.dataset.copy); snack(L('Скопировано', 'Copied')); } catch { snack(L('Не удалось скопировать', 'Couldn’t copy')); }
    } else if (t.id === 'newport') {
      const port = await run(() => call('regenerate_port'), t);
      if (port) { cfg = await call('get_config'); snack(L(`Новый порт: ${port}. Обнови адрес в Claude.`, `New port: ${port}. Update the address in Claude.`)); }
    } else if (t.id === 'opendir') call('open_data_dir');
    else if (t.id === 'phone-pin') await run(() => call('phone_pin'), t);
    else if (t.dataset.segdel !== undefined) {
      const i = Number(t.dataset.segdel);
      save((c) => { c.segments.splice(i, 1); });
    } else if (t.dataset.segadd !== undefined) {
      save((c) => {
        const base = L('Свой', 'Custom');
        let name = base;
        for (let n = 2; c.segments.some((x) => x.name.toLowerCase() === name.toLowerCase()); n++) name = `${base} ${n}`;
        c.segments.push({ name, minutes: 15, alarm: false, open_access: false, prep_min: 0 });
      }).then(() => box.querySelector('.seg-type:last-of-type .seg-name')?.select());
    }
    else if (t.dataset.forget) {
      const ok = await run(() => call('phone_forget', { id: t.dataset.forget }).then(() => true), t);
      if (ok) snack(L('Телефон отключён — подключить снова можно по новому PIN', 'Phone unpaired — pair again with a new PIN'));
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
      if (nsig !== sig) { sig = nsig; if (cfg) render(); }
    },
    show() { reload(); },
    config() { reload(); },
  };
}

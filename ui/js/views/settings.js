import { call, esc } from '../api.js';
import { icon } from '../icons.js';
import { applyTheme } from '../theme.js';
import { run, snack, WEEKDAYS } from '../ui.js';

const SWATCHES = [
  ['#2E7D32', 'Зелёный'], ['#00796B', 'Бирюзовый'], ['#1565C0', 'Синий'], ['#5E35B1', 'Фиолетовый'],
  ['#C2185B', 'Малиновый'], ['#E65100', 'Оранжевый'], ['#6D4C41', 'Коричневый'],
];

export function mountSettings(root, ctx) {
  let cfg = null;
  let last = null;

  root.innerHTML = `<div class="readable" id="settings"></div>`;
  const box = root.querySelector('#settings');

  const seg = (name, value, opts) => `<div class="segmented" role="group" data-seg="${name}">${opts.map(([v, l]) =>
    `<button class="interactive" data-v="${v}" aria-pressed="${v === value}">${icon('check')}${l}</button>`).join('')}</div>`;
  const num = (path, v, min, max, label, disabled = false) => `<div class="field num"><input type="number" data-num="${path}" value="${v}" min="${min}" max="${max}" aria-label="${label}" ${disabled ? 'disabled' : ''}></div>`;
  const sw = (path, v, label, disabled = false) => `<input type="checkbox" class="switch" role="switch" data-sw="${path}" ${v ? 'checked' : ''} ${disabled ? 'disabled' : ''} aria-label="${label}">`;

  function render() {
    if (!cfg || !last) return;
    const v = last.view;
    const locked = v.lock.base && v.started;
    const m = last.meta;
    const mcpUrl = m.mcp.url || `http://127.0.0.1:${cfg.mcp_port}/mcp`;
    const cmd = `claude mcp add --transport http clockmanage ${mcpUrl}`;
    const dayEnd = `${String(Math.floor(cfg.day_end_min / 60)).padStart(2, '0')}:${String(cfg.day_end_min % 60).padStart(2, '0')}`;
    const custom = !SWATCHES.some(([c]) => c.toLowerCase() === cfg.appearance.seed.toLowerCase());
    const focused = document.activeElement?.dataset?.num;
    box.innerHTML = `
      ${locked ? `<div class="lock-note">${icon('lock')}<div class="body-m">Идёт учебный день — расписание и длительности заблокированы до его конца.</div></div>` : ''}
      <section class="section">
        <h2>Учебные дни</h2>
        <div class="surface">
          <div class="setting col">
            <div><div class="t">Дни, когда действует учебный режим</div><div class="d">В эти дни «Начать день» включает блокировку.</div></div>
            <div class="chips weekdays" role="group" aria-label="Дни недели">${WEEKDAYS.map((d, i) =>
              `<button class="chip interactive" data-day="${i}" aria-pressed="${cfg.study_days[i]}" ${locked ? 'disabled' : ''}>${cfg.study_days[i] ? icon('check') : ''}${d}</button>`).join('')}</div>
          </div>
          <div class="setting">
            <div class="grow"><div class="t">Конец учебного дня (МСК)</div><div class="d">В это время блокировка снимается, даже если блоки не закрыты.</div></div>
            <div class="field"><input type="time" id="dayend" value="${dayEnd}" ${locked ? 'disabled' : ''} aria-label="Конец дня"></div>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>Отрезки и перерывы</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">Работа в одном отрезке</div><div class="d">Блок делится на отрезки: 1,5 ч = 45 + 45</div></div>${num('timing.work_segment_min', cfg.timing.work_segment_min, 5, 240, 'Минут работы', locked)}</div>
          <div class="setting"><div class="grow"><div class="t">Перерыв между отрезками</div></div>${num('timing.short_break_min', cfg.timing.short_break_min, 1, 120, 'Минут перерыва', locked)}</div>
          <div class="setting"><div class="grow"><div class="t">Перерыв между блоками</div></div>${num('timing.between_blocks_min', cfg.timing.between_blocks_min, 1, 180, 'Минут между блоками', locked)}</div>
          <div class="setting"><div class="grow"><div class="t">Обед с таймером</div><div class="d">Один раз за день, кнопкой в перерыве</div></div>${num('timing.lunch_min', cfg.timing.lunch_min, 5, 180, 'Минут обеда', locked)}</div>
        </div>
      </section>

      <section class="section">
        <h2>Звук и оповещения</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">Звук</div><div class="d">Конец работы, конец перерыва, конец блока</div></div>${sw('sound', cfg.sound, 'Звук')}</div>
          <div class="setting"><div class="grow"><div class="t">Будильник поверх экрана</div><div class="d">Рисованная анимация поверх всех окон на каждом переходе</div></div>${sw('overlay', cfg.overlay, 'Будильник поверх экрана')}</div>
          <div class="setting"><div class="grow"><div class="t">Напоминать, пока не нажал «Начать», каждые, сек</div></div>${num('reminder_sec', cfg.reminder_sec, 15, 600, 'Секунд между напоминаниями')}</div>
          <div class="setting col">
            <div class="t">Послушать звук</div>
            <div class="hstack">
              <button class="btn tonal interactive" data-sound="alarm">${icon('alarm')}Звонок</button>
              <button class="btn tonal interactive" data-sound="break">${icon('coffee')}Перерыв</button>
              <button class="btn tonal interactive" data-sound="done">${icon('check')}Блок закрыт</button>
            </div>
            <div class="t" style="margin-top:8px">Показать оповещение на экране</div>
            <div class="hstack">
              <button class="btn outlined interactive" data-ov="await">${icon('alarm')}Звонок</button>
              <button class="btn outlined interactive" data-ov="break">${icon('coffee')}Перерыв</button>
              <button class="btn outlined interactive" data-ov="block">${icon('check')}Блок закрыт</button>
            </div>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>Оформление</h2>
        <div class="surface">
          <div class="setting col">
            <div class="t">Основной цвет</div>
            <div class="swatches">${SWATCHES.map(([c, n]) => `<button class="swatch" style="--c:${c}" data-seed="${c}" aria-label="${n}" title="${n}" aria-pressed="${c.toLowerCase() === cfg.appearance.seed.toLowerCase()}"></button>`).join('')}
              <label class="swatch" style="--c:${custom ? esc(cfg.appearance.seed) : 'conic-gradient(red, yellow, lime, cyan, blue, magenta, red)'};background:var(--c);display:grid;place-items:center" aria-pressed="${custom}" title="Свой цвет">
                <input type="color" id="seed-in" value="${esc(cfg.appearance.seed)}" style="opacity:0;width:100%;height:100%;cursor:pointer" aria-label="Свой цвет"></label>
            </div>
          </div>
          <div class="setting"><div class="grow"><div class="t">Тема</div></div>${seg('mode', cfg.appearance.mode, [['system', 'Системная'], ['light', 'Светлая'], ['dark', 'Тёмная']])}</div>
          <div class="setting"><div class="grow"><div class="t">Насыщенность</div><div class="d">«Точная» держит цвет как выбран — самый заметный акцент</div></div>${seg('variant', cfg.appearance.variant, [['fidelity', 'Точная'], ['tonal_spot', 'Спокойная'], ['vibrant', 'Яркая']])}</div>
        </div>
      </section>

      <section class="section">
        <h2>MCP для Claude</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">Локальный MCP-сервер</div>
            <div class="d">${m.mcp.running ? `Работает на 127.0.0.1:${m.mcp.port}` : m.mcp.enabled ? `Не запущен${m.mcp.error ? ': ' + esc(m.mcp.error) : ''}` : 'Выключен'}. Инструменты: set_plan, get_plan, get_session_state, get_today_stats.</div></div>${sw('mcp_enabled', cfg.mcp_enabled, 'MCP-сервер')}</div>
          <div class="setting col">
            <div class="t">Адрес</div>
            <div class="hstack"><code class="code">${esc(mcpUrl)}</code><button class="icon-btn interactive" data-copy="${esc(mcpUrl)}" aria-label="Скопировать адрес">${icon('copy')}</button></div>
            <div class="t" style="margin-top:8px">Команда для Claude Code</div>
            <div class="hstack"><code class="code">${esc(cmd)}</code><button class="icon-btn interactive" data-copy="${esc(cmd)}" aria-label="Скопировать команду">${icon('copy')}</button></div>
            <div><button class="btn text interactive" id="newport">${icon('refresh')}Новый случайный порт</button></div>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>Система</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">Автозапуск и самовосстановление</div><div class="d">Запуск при входе в Windows без окна UAC и перезапуск раз в 5 минут, если процесс убили</div></div>${sw('autostart', cfg.autostart, 'Автозапуск', locked && cfg.autostart)}</div>
          <div class="setting"><div class="grow"><div class="t">Права администратора</div><div class="d">${m.admin ? 'Есть — блокировка работает полностью' : 'Нет — hosts, политики браузеров и закрытие приложений недоступны'}</div></div>${icon(m.admin ? 'shield' : 'warning')}</div>
          <div class="setting"><div class="grow"><div class="t">Данные и журнал</div><div class="d ellipsis" title="${esc(m.data_dir)}">${esc(m.data_dir)}</div></div><button class="btn text interactive" id="opendir">${icon('folder')}Открыть</button></div>
          <div class="setting"><div class="grow"><div class="t">Версия</div><div class="d">ClockManage ${esc(m.version)}</div></div></div>
        </div>
      </section>`;
    if (focused) box.querySelector(`[data-num="${focused}"]`)?.focus();
  }

  async function save(mut, okText) {
    const next = structuredClone(cfg);
    mut(next);
    const saved = await run(() => call('save_config', { cfg: next }));
    if (saved) { cfg = saved; if (okText) snack(okText); }
    render();
    return !!saved;
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
        { duration: 650, easing: 'cubic-bezier(.2, 0, 0, 1)', pseudoElement: '::view-transition-new(root)' },
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

  box.addEventListener('change', (e) => {
    const t = e.target;
    if (t.dataset.sw) save((c) => setPath(c, t.dataset.sw, t.checked));
    else if (t.dataset.num) save((c) => setPath(c, t.dataset.num, Math.round(Number(t.value))));
    else if (t.id === 'dayend') {
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
    if (t.dataset.day !== undefined) save((c) => { c.study_days[Number(t.dataset.day)] = !c.study_days[Number(t.dataset.day)]; });
    else if (t.dataset.seed) reveal(t, { ...cfg.appearance, seed: t.dataset.seed });
    else if (t.closest('[data-seg]')) {
      const name = t.closest('[data-seg]').dataset.seg;
      if (['mode', 'variant'].includes(name)) reveal(t, { ...cfg.appearance, [name]: t.dataset.v });
    } else if (t.dataset.sound) call('test_sound', { kind: t.dataset.sound });
    else if (t.dataset.ov) call('preview_overlay', { kind: t.dataset.ov });
    else if (t.dataset.copy) {
      try { await navigator.clipboard.writeText(t.dataset.copy); snack('Скопировано'); } catch { snack('Не удалось скопировать'); }
    } else if (t.id === 'newport') {
      const port = await run(() => call('regenerate_port'), t);
      if (port) { cfg = await call('get_config'); snack(`Новый порт: ${port}. Обнови адрес в Claude.`); }
    } else if (t.id === 'opendir') call('open_data_dir');
  });

  async function reload() {
    cfg = await call('get_config');
    render();
  }

  let sig = '';
  return {
    update(s) {
      last = s;
      const nsig = `${s.view.lock.base}|${s.view.started}|${JSON.stringify(s.meta.mcp)}|${s.meta.admin}`;
      if (nsig !== sig && cfg) { sig = nsig; render(); }
    },
    show() { reload(); },
    config() { reload(); },
  };
}

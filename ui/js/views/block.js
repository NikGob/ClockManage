import { call, esc, mmss } from '../api.js';
import { icon } from '../icons.js';
import { run, snack, tsHm } from '../ui.js';
import { emergencyDialog } from './dialogs.js';
import { L } from '../i18n.js';

export function mountBlock(root, ctx) {
  let cfg = null;
  let last = null;
  let cfgSig = '';

  root.innerHTML = `
    <div class="readable">
      <section class="lock-hero" id="hero"><div class="lockart" id="lockart"></div><div id="hero-text" aria-live="polite"></div></section>

      <section class="section">
        <h2>${L('Сайты', 'Sites')}</h2>
        <div class="surface pad stack">
          <div class="chips" id="sites"></div>
          <div class="add-row">
            <div class="field"><label for="site-in">${L('Добавить сайт', 'Add site')}</label><input id="site-in" placeholder="${L('reddit.com или youtube.com/shorts', 'reddit.com or youtube.com/shorts')}" autocomplete="off"></div>
            <button class="btn tonal interactive" id="site-add">${icon('add')}${L('Добавить', 'Add')}</button>
          </div>
          <p class="note-text">${L('Путь вроде', 'A path like')} <b>youtube.com/shorts</b> ${L('закрывает только раздел — остальной YouTube доступен.', 'closes only that section — the rest of YouTube stays open.')}</p>
          <div class="ff-line">
            <label for="ff" class="body-l grow">${L('Перезапускать Firefox при блокировке', 'Restart Firefox when the lock starts')}</label>
            <button class="icon-btn interactive" id="ff-now" aria-label="${L('Перезапустить Firefox сейчас', 'Restart Firefox now')}" title="${L('Перезапустить Firefox сейчас', 'Restart Firefox now')}">${icon('refresh')}</button>
            <input type="checkbox" class="switch" role="switch" id="ff">
          </div>
        </div>
      </section>

      <section class="section">
        <h2>${L('Приложения', 'Apps')}</h2>
        <div class="surface pad stack">
          <div class="chips" id="apps"></div>
          <div class="add-row">
            <div class="field"><label for="app-in">${L('Добавить приложение (имя процесса)', 'Add an app (process name)')}</label><input id="app-in" placeholder="Steam.exe" autocomplete="off"></div>
            <button class="btn tonal interactive" id="app-add">${icon('add')}${L('Добавить', 'Add')}</button>
          </div>
          <p class="note-text">${L('Если приложение запущено во время учёбы — оно закрывается в течение секунды.', 'If an app is running during study, it is closed within a second.')}</p>
        </div>
      </section>

      <section class="section">
        <h2>${L('Приложения на телефоне', 'Phone apps')}</h2>
        <div class="surface pad stack">
          <div class="chips" id="phone-apps"></div>
          <div class="add-row">
            <div class="field"><label for="papp-in">${L('Добавить (имя пакета Android)', 'Add (Android package name)')}</label><input id="papp-in" placeholder="com.zhiliaoapp.musically" autocomplete="off"></div>
            <button class="btn tonal interactive" id="papp-add">${icon('add')}${L('Добавить', 'Add')}</button>
          </div>
          <p class="note-text">${L('Удобнее выбрать из списка установленных — в самом приложении на телефоне. Сайты телефон берёт из списка выше. Звонилку, настройки и лаунчер заблокировать нельзя.', 'It’s easier to pick from the installed apps — in the phone app itself. The phone takes sites from the list above. The dialer, settings and launcher can’t be blocked.')}</p>
        </div>
      </section>

      <section class="section">
        <h2>${L('Пауза и доступ', 'Pause and access')}</h2>
        <div class="surface">
          <div class="setting">
            <div class="grow"><div class="t">${L('Пускать в заблокированное на паузе', 'Open blocked things while paused')}</div>
              <div class="d" id="pa-d"></div></div>
            <input type="checkbox" class="switch" role="switch" id="pa" aria-describedby="pa-d">
          </div>
          <div class="setting">
            <div class="grow"><div class="t">${L('Доступ на паузе, мин', 'Access on pause, min')}</div><div class="d">${L('Потом блок возвращается, даже если пауза продолжается. Продление — через мини-капчу.', 'Then the lock comes back, even if the pause goes on. Extending takes a mini captcha.')}</div></div>
            <div class="field num"><input type="number" id="pa-min" min="1" max="60" aria-label="${L('Минут доступа на паузе', 'Minutes of access on pause')}"></div>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>${L('Аварийный доступ', 'Emergency access')}</h2>
        <div class="surface">
          <div class="setting">
            <div class="grow"><div class="t">${L('Открыть всё на', 'Open everything for')} <span id="em-min">10</span> ${L('минут', 'minutes')}</div>
              <div class="d" id="em-d">${L('Нужно вручную переписать длинную фразу. Каждый раз пишется в лог.', 'You have to type a long phrase by hand. Every use is logged.')}</div></div>
            <button class="btn outlined danger interactive" id="em">${icon('warning')}${L('Аварийно…', 'Emergency…')}</button>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>${L('Что нужно знать', 'Good to know')}</h2>
        <ul class="limits body-m">
          <li>${L('Программа работает с правами администратора: закрыть её во время учёбы нельзя, а hosts и политики браузеров остаются в силе, даже если процесс убить. Каждые 5 минут планировщик поднимает её обратно.', 'The app runs as administrator: it can’t be closed during study, and hosts and browser policies stay in force even if the process is killed. Every 5 minutes the Task Scheduler brings it back.')}</li>
          <li>${L('Chrome, Edge, Brave и Яндекс подхватывают блок-лист сразу. Firefox — после перезапуска, поэтому программа сама перезапускает его при включении блокировки (если переключатель выше включён).', 'Chrome, Edge, Brave and Yandex pick up the block list at once. Firefox only after a restart, so the app restarts it when the lock turns on (if the switch above is on).')}</li>
          <li>${L('Shorts, открытый кликом внутри YouTube, браузер может показать без перезагрузки страницы — политика сработает на следующем переходе или обновлении.', 'Shorts opened by a click inside YouTube may show without a page reload — the policy applies on the next navigation or refresh.')}</li>
        </ul>
      </section>
    </div>`;

  const $ = (id) => root.querySelector('#' + id);

  function chips(list, kind, removable) {
    return list.map((s, i) => `<span class="chip input${removable ? ' removable' : ''}">${esc(s)}${removable ? `<button class="x interactive" data-rm="${kind}" data-i="${i}" aria-label="${L('Убрать', 'Remove')} ${esc(s)}">${icon('close')}</button>` : ''}</span>`).join('')
      || `<span class="body-m muted">${L('Пусто', 'Empty')}</span>`;
  }

  function renderCfg() {
    if (!cfg || !last) return;
    const locked = last.view.lock.base && last.view.lock.reason !== 'completed';
    $('sites').innerHTML = chips(cfg.blocklist.sites, 'sites', !locked);
    $('apps').innerHTML = chips(cfg.blocklist.apps, 'apps', !locked);
    $('phone-apps').innerHTML = chips(cfg.phone.apps, 'phone', !locked);
    const pa = $('pa');
    pa.checked = cfg.pause_access;
    pa.disabled = !last.view.can.edit_pause_access;
    $('pa-d').textContent = last.view.can.edit_pause_access
      ? L(`Меняется до начала учебного дня или после ${last.view.day_end}. Во время учёбы — заблокировано.`, `Changes before the study day starts or after ${last.view.day_end}. Locked during study.`)
      : `${L('Во время учёбы не меняется. Сейчас:', 'Doesn’t change during study. Now:')} ${cfg.pause_access ? L('включено', 'on') : L('выключено', 'off')}.`;
    $('pa-min').value = cfg.pause_access_min;
    $('ff').checked = cfg.restart_firefox;
    $('pa-min').disabled = locked;
    $('em-min').textContent = cfg.emergency_min;
  }

  function renderHero() {
    const v = last.view;
    const lk = v.lock;
    let cls = '', ic = 'lock_open', h = '', p = '';
    if (lk.blocked) {
      cls = 'on'; ic = 'lock';
      h = L('Блокировка включена', 'Lock on');
      const focusEnd = lk.focus_until ? tsHm(lk.focus_until, last.meta.tz_offset_min) : '';
      p = lk.reason === 'single' ? L('На время одиночного таймера.', 'For the single timer.')
        : lk.reason === 'focus' ? L(`Фокус-блокировка до ${focusEnd}. Раньше её снимает только аварийный доступ.`, `Focus lock until ${focusEnd}. Only emergency access lifts it earlier.`)
          : L(`Снимется, когда отсидишь все блоки, или в ${v.day_end}.`, `Lifts when all blocks are done, or at ${v.day_end}.`);
      if (focusEnd && lk.reason !== 'focus') p += L(` Фокус-блокировка — до ${focusEnd}.`, ` Focus lock — until ${focusEnd}.`);
    } else if (lk.base) {
      cls = 'open';
      h = { pause_access: L('Доступ на паузе', 'Access on pause'), emergency: L('Аварийный доступ', 'Emergency access'), lunch_at_pc: L('Обед за ПК', 'Lunch at the PC') }[lk.reason] || L('Доступ открыт', 'Access open');
      p = lk.until ? L(`Блокировка вернётся через ${mmss(lk.until - v.now)}`, `The lock comes back in ${mmss(lk.until - v.now)}`) : '';
    } else {
      h = L('Сейчас без блокировки', 'No lock right now');
      p = { not_started: v.study_day ? L('Включится по кнопке «Начать день».', 'Turns on with “Start day”.') : (v.kind === 'off' ? L('Сегодня выходной — без блокировки.', 'Today is a day off — no lock.') : L('В этот тип дня блокировка выключена.', 'The lock is off for this day type.')), completed: L('Все блоки дня отсижены.', 'All blocks of the day are done.'), day_end: L(`После ${v.day_end} блокировки нет.`, `No lock after ${v.day_end}.`), not_study_day: v.kind === 'off' ? L('Сегодня выходной — без блокировки.', 'Today is a day off — no lock.') : L('В этот тип дня блокировка выключена.', 'The lock is off for this day type.') }[lk.reason] || '';
    }
    if (!last.meta.admin) p += L(' Внимание: программа запущена без прав администратора — сайты не блокируются.', ' Warning: the app runs without administrator rights — sites are not blocked.');
    if (last.meta.blocker_error) p += ` ${L('Ошибка', 'Error')}: ${last.meta.blocker_error}`;
    const html = `<h2>${esc(h)}</h2><p class="body-l">${esc(p)}</p>${v.emergency_count ? `<p class="body-m">${L('Аварийных доступов сегодня:', 'Emergency accesses today:')} ${v.emergency_count}</p>` : ''}`;
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
      ? L('Нужно вручную переписать длинную фразу. Каждый раз пишется в лог.', 'You have to type a long phrase by hand. Every use is logged.')
      : lk.base ? L('Доступ уже открыт.', 'Access is already open.') : L('Доступен, только пока действует блокировка.', 'Available only while the lock is on.');
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
  }

  const listOf = (c, kind) => (kind === 'phone' ? c.phone.apps : c.blocklist[kind]);

  function addFrom(inputId, kind) {
    const inp = $(inputId);
    const val = inp.value.trim();
    if (!val) return;
    save((c) => listOf(c, kind).push(val), `${L('Добавлено', 'Added')}: ${val}`).then(() => { inp.value = ''; inp.focus(); });
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
    save((c) => listOf(c, kind).splice(i, 1), `${L('Убрано', 'Removed')}: ${name}`);
  });
  $('ff-now').addEventListener('click', async (e) => {
    const b = e.currentTarget;
    if (b.classList.contains('spin')) return;
    b.classList.add('spin');
    const ok = await run(() => call('restart_firefox').then(() => true));
    b.classList.remove('spin');
    if (ok) snack(L('Firefox перезапущен — вкладки восстановятся', 'Firefox restarted — your tabs will be restored'));
  });
  $('ff').addEventListener('change', (e) => save((c) => { c.restart_firefox = e.target.checked; }));
  $('pa').addEventListener('change', (e) => save((c) => { c.pause_access = e.target.checked; }));
  $('pa-min').addEventListener('change', (e) => save((c) => { c.pause_access_min = Math.round(Number(e.target.value) || 10); }));
  $('em').addEventListener('click', async () => {
    const ok = await emergencyDialog(cfg, last.view.emergency_count);
    if (ok) snack(L(`Аварийный доступ на ${cfg.emergency_min} мин. Записано в лог.`, `Emergency access for ${cfg.emergency_min} min. Logged.`));
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
      const sig = `${s.view.lock.base}|${s.view.lock.reason}|${s.view.can.edit_pause_access}`;
      if (sig !== cfgSig) { cfgSig = sig; renderCfg(); }
    },
    show() { reload(); },
    config() { reload(); },
  };
}

import { call, esc, mmss } from '../api.js';
import { icon } from '../icons.js';
import { doodle, play } from '../doodles.js';
import { run, snack } from '../ui.js';
import { emergencyDialog } from './dialogs.js';

export function mountBlock(root, ctx) {
  let cfg = null;
  let last = null;
  let cfgSig = '';

  root.innerHTML = `
    <div class="readable">
      <section class="lock-hero" id="hero"><div class="lockart" id="lockart"></div><div id="hero-text" aria-live="polite"></div></section>

      <section class="section">
        <h2>Сайты</h2>
        <div class="surface pad stack">
          <div class="chips" id="sites"></div>
          <div class="add-row">
            <div class="field"><label for="site-in">Добавить сайт</label><input id="site-in" placeholder="reddit.com или youtube.com/shorts" autocomplete="off"></div>
            <button class="btn tonal interactive" id="site-add">${icon('add')}Добавить</button>
          </div>
          <div class="toggle-line"><label for="ff" class="body-l">Перезапускать Firefox, чтобы Shorts блокировался сразу<span class="d body-m muted" style="display:block">Firefox читает правила только при старте. Вкладки восстановятся сами.</span></label><input type="checkbox" class="switch" role="switch" id="ff"></div>
          <p class="note-text">Домен блокируется целиком через hosts и политики браузеров. Путь вроде <b>youtube.com/shorts</b> блокирует только этот раздел — обычный YouTube остаётся доступен.</p>
        </div>
      </section>

      <section class="section">
        <h2>Приложения</h2>
        <div class="surface pad stack">
          <div class="chips" id="apps"></div>
          <div class="add-row">
            <div class="field"><label for="app-in">Добавить приложение (имя процесса)</label><input id="app-in" placeholder="Steam.exe" autocomplete="off"></div>
            <button class="btn tonal interactive" id="app-add">${icon('add')}Добавить</button>
          </div>
          <p class="note-text">Если приложение запущено во время учёбы — оно закрывается в течение секунды.</p>
        </div>
      </section>

      <section class="section">
        <h2>Пауза и доступ</h2>
        <div class="surface">
          <div class="setting">
            <div class="grow"><div class="t">Пускать в заблокированное на паузе</div>
              <div class="d" id="pa-d"></div></div>
            <input type="checkbox" class="switch" role="switch" id="pa" aria-describedby="pa-d">
          </div>
          <div class="setting">
            <div class="grow"><div class="t">Доступ на паузе, мин</div><div class="d">Потом блок возвращается, даже если пауза продолжается. Продление — через мини-капчу.</div></div>
            <div class="field num"><input type="number" id="pa-min" min="1" max="60" aria-label="Минут доступа на паузе"></div>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>Аварийный доступ</h2>
        <div class="surface">
          <div class="setting">
            <div class="grow"><div class="t">Открыть всё на <span id="em-min">10</span> минут</div>
              <div class="d" id="em-d">Нужно вручную переписать длинную фразу. Каждый раз пишется в лог.</div></div>
            <button class="btn outlined danger interactive" id="em">${icon('warning')}Аварийно…</button>
          </div>
        </div>
      </section>

      <section class="section">
        <h2>Что нужно знать</h2>
        <ul class="limits body-m">
          <li>Программа работает с правами администратора: закрыть её во время учёбы нельзя, а hosts и политики браузеров остаются в силе, даже если процесс убить. Каждые 5 минут планировщик поднимает её обратно.</li>
          <li>Chrome, Edge, Brave и Яндекс подхватывают блок-лист сразу. Firefox — после перезапуска, поэтому программа сама перезапускает его при включении блокировки (если переключатель выше включён).</li>
          <li>Shorts, открытый кликом внутри YouTube, браузер может показать без перезагрузки страницы — политика сработает на следующем переходе или обновлении.</li>
        </ul>
      </section>
    </div>`;

  const $ = (id) => root.querySelector('#' + id);

  function chips(list, kind, removable) {
    return list.map((s, i) => `<span class="chip input">${esc(s)}${removable ? `<button class="x interactive" data-rm="${kind}" data-i="${i}" aria-label="Убрать ${esc(s)}">${icon('close')}</button>` : ''}</span>`).join('')
      || '<span class="body-m muted">Пусто</span>';
  }

  function renderCfg() {
    if (!cfg || !last) return;
    const locked = last.view.lock.base && last.view.lock.reason !== 'completed';
    $('sites').innerHTML = chips(cfg.blocklist.sites, 'sites', !locked);
    $('apps').innerHTML = chips(cfg.blocklist.apps, 'apps', !locked);
    const pa = $('pa');
    pa.checked = cfg.pause_access;
    pa.disabled = !last.view.can.edit_pause_access;
    $('pa-d').textContent = last.view.can.edit_pause_access
      ? `Меняется до начала учебного дня или после ${last.view.day_end}. Во время учёбы — заблокировано.`
      : `Во время учёбы не меняется. Сейчас: ${cfg.pause_access ? 'включено' : 'выключено'}.`;
    $('pa-min').value = cfg.pause_access_min;
    $('ff').checked = cfg.restart_firefox;
    $('pa-min').disabled = locked;
    $('em-min').textContent = cfg.emergency_min;
  }

  function renderHero() {
    const v = last.view;
    const L = v.lock;
    let cls = '', ic = 'lock_open', h = '', p = '';
    if (L.blocked) {
      cls = 'on'; ic = 'lock';
      h = 'Блокировка включена';
      p = L.reason === 'single' ? 'На время одиночного таймера.' : `Снимется, когда отсидишь все блоки, или в ${v.day_end}.`;
    } else if (L.base) {
      cls = 'open';
      h = { pause_access: 'Доступ на паузе', emergency: 'Аварийный доступ', lunch_at_pc: 'Обед за ПК' }[L.reason] || 'Доступ открыт';
      p = L.until ? `Блокировка вернётся через ${mmss(L.until - v.now)}` : '';
    } else {
      h = 'Сейчас без блокировки';
      p = { not_started: v.study_day ? 'Включится по кнопке «Начать день».' : 'Сегодня не учебный день.', completed: 'Все блоки дня отсижены.', day_end: `После ${v.day_end} блокировки нет.`, not_study_day: 'Сегодня не учебный день.' }[L.reason] || '';
    }
    if (!last.meta.admin) p += ' Внимание: программа запущена без прав администратора — сайты не блокируются.';
    if (last.meta.blocker_error) p += ` Ошибка: ${last.meta.blocker_error}`;
    const html = `<h2>${esc(h)}</h2><p class="body-l">${esc(p)}</p>${v.emergency_count ? `<p class="body-m">Аварийных доступов сегодня: ${v.emergency_count}</p>` : ''}`;
    const hero = $('hero');
    hero.className = `lock-hero ${cls}`;
    const art = $('lockart');
    if (!art.firstElementChild) { art.innerHTML = doodle('lock', 64); play(art); }
    const txt = $('hero-text');
    if (txt.dataset.html !== html) {
      if (txt.dataset.html) txt.animate([{ opacity: 0, transform: 'translateX(-8px)' }, { opacity: 1, transform: 'none' }], { duration: 320, easing: 'cubic-bezier(.05,.7,.1,1)' });
      txt.innerHTML = html;
      txt.dataset.html = html;
    }
    $('em').disabled = !v.can.emergency;
    $('em-d').textContent = v.can.emergency
      ? 'Нужно вручную переписать длинную фразу. Каждый раз пишется в лог.'
      : L.base ? 'Доступ уже открыт.' : 'Доступен, только пока действует блокировка.';
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

  function addFrom(inputId, kind) {
    const inp = $(inputId);
    const val = inp.value.trim();
    if (!val) return;
    save((c) => c.blocklist[kind].push(val), `Добавлено: ${val}`).then(() => { inp.value = ''; inp.focus(); });
  }

  $('site-add').addEventListener('click', () => addFrom('site-in', 'sites'));
  $('app-add').addEventListener('click', () => addFrom('app-in', 'apps'));
  $('site-in').addEventListener('keydown', (e) => { if (e.key === 'Enter') addFrom('site-in', 'sites'); });
  $('app-in').addEventListener('keydown', (e) => { if (e.key === 'Enter') addFrom('app-in', 'apps'); });
  root.addEventListener('click', (e) => {
    const b = e.target.closest('[data-rm]');
    if (!b) return;
    const kind = b.dataset.rm;
    const i = Number(b.dataset.i);
    const name = cfg.blocklist[kind][i];
    save((c) => c.blocklist[kind].splice(i, 1), `Убрано: ${name}`);
  });
  $('ff').addEventListener('change', (e) => save((c) => { c.restart_firefox = e.target.checked; }));
  $('pa').addEventListener('change', (e) => save((c) => { c.pause_access = e.target.checked; }));
  $('pa-min').addEventListener('change', (e) => save((c) => { c.pause_access_min = Math.round(Number(e.target.value) || 10); }));
  $('em').addEventListener('click', async () => {
    const ok = await emergencyDialog(cfg, last.view.emergency_count);
    if (ok) snack(`Аварийный доступ на ${cfg.emergency_min} мин. Записано в лог.`);
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

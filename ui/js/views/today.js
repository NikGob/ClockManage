import { call, mmss, dur, esc } from '../api.js';
import { icon, morphIcon } from '../icons.js';
import { doodle, play } from '../doodles.js';
import { WaveRing } from '../wave.js';
import { run, snack, menu, KINDS, KIND_RANK, kindLabel, planSummary, hm } from '../ui.js';
import { lunchDialog, singleDialog, emergencyDialog, captchaDialog } from './dialogs.js';

export function mountToday(root, ctx) {
  root.innerHTML = `
  <div class="today">
    <section class="hero" id="hero" aria-label="Таймер">
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
      <section class="surface plan-card" aria-labelledby="plan-h">
        <div class="head">
          <div class="row1"><h2 id="plan-h">План дня</h2><button class="kind-chip interactive" id="kind" data-act="kind" aria-haspopup="menu"></button><button class="btn text interactive" id="edit-plan">Изменить</button></div>
          <div class="total tnum" id="total"></div>
          <div class="linear" id="total-bar"></div>
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
  ctx.setActions(`<button class="btn tonal interactive" data-top="mini">${icon('pip')}Мини-таймер</button>`);
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
    art.className = `art ${{ await: 'ringing', lunch: 'steaming', idle: 'writing' }[kind] || ''}`;
    play(art);
  }

  function lockLine(v, meta) {
    const L = v.lock;
    let ic = 'lock_open', text = '', when = '', cls = '', action = '';
    const left = L.until ? mmss(L.until - v.now) : '';
    // The day end is a quiet button: click → move it later for today.
    const endBtn = (label) => (v.can.extend_day_end
      ? `<button class="when-btn interactive" data-act="dayend" aria-haspopup="menu" data-tip="Продлить учёбу сегодня — только позже">${esc(label)}${icon('expand')}</button>`
      : esc(label));
    const offText = v.kind === 'off' ? 'Выходной — без блокировки' : `${kindLabel(v.kind)} день без блокировки`;
    switch (L.reason) {
      case 'study': ic = 'lock'; text = 'Блокировка включена'; when = `${endBtn(`до ${v.day_end}`)} или до конца плана`; break;
      case 'single': ic = 'lock'; text = 'Блокировка на время таймера'; break;
      case 'pause_access': text = 'Доступ открыт на паузе'; when = left; cls = 'open'; action = 'end'; break;
      case 'emergency': text = 'Аварийный доступ'; when = left; cls = 'open'; action = 'end'; break;
      case 'lunch_at_pc': text = 'Обед за ПК — доступ открыт'; when = left; cls = 'open'; break;
      case 'not_started':
        text = v.study_day ? 'Блокировка включится по кнопке «Начать день»' : offText;
        if (v.study_day) when = `и продержится ${endBtn(`до ${v.day_end}`)}`;
        break;
      case 'completed': text = 'Все блоки отсижены — блокировка снята'; break;
      case 'day_end':
        text = `После ${v.day_end} блокировки нет`;
        if (v.started && v.can.extend_day_end) when = endBtn('Вернуть блокировку');
        break;
      case 'not_study_day': text = offText; break;
      default: text = '';
    }
    if (L.base && !['pause_access', 'emergency'].includes(L.reason) && v.can.emergency) action = 'emergency';
    if (!meta.admin && L.base) { text += ' · нет прав администратора'; cls = 'open'; }
    const btn = action === 'end'
      ? '<button class="btn text interactive" data-act="end_access">Закрыть доступ</button>'
      : action === 'emergency' ? `<button class="sos interactive" data-act="emergency" aria-label="Аварийный доступ">${icon('warning')}<span class="lbl">Аварийно</span></button>` : '';
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
    b.dataset.tip = v.can.lighter_kind ? 'Тип сегодняшнего дня: свой план и блокировка' : 'Во время учёбы день можно сделать только плотнее';
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
        sub: `${planSummary(p.plan)}${p.block ? '' : ' · без блокировки'}`,
        disabled: lighter && !v.can.lighter_kind,
        tip: lighter && !v.can.lighter_kind ? 'Во время учёбы — только плотнее' : '',
      };
    }), {
      title: 'Тип дня',
      note: v.started ? 'День уже идёт: блоки нового типа добавятся к плану, начатое не пропадёт.' : 'План на сегодня заменится шаблоном этого типа.',
    });
    if (!kind || kind === cur) return;
    const ok = await run(() => call('set_day_kind', { kind }).then(() => true), b);
    if (ok) snack(`Сегодня — ${kindLabel(kind).toLowerCase()} день`);
  }

  async function extendDayEnd(b) {
    const v = last.view;
    const base = Math.max(v.day_end_min, v.now_min);
    const LAST = 23 * 60 + 59;
    const opts = [];
    for (const add of [30, 60, 120]) {
      const m = Math.ceil((base + add) / 5) * 5;
      if (m <= LAST && m > v.day_end_min && !opts.includes(m)) opts.push(m);
    }
    if (!opts.includes(LAST) && LAST > v.day_end_min) opts.push(LAST);
    const minutes = await menu(b, opts.map((m) => ({
      value: m, label: `до ${hm(m)}`, icon: icon('schedule'),
      sub: m === LAST ? 'до конца суток' : `+${m - v.day_end_min >= 60 ? `${Math.floor((m - v.day_end_min) / 60)} ч ` : ''}${(m - v.day_end_min) % 60 ? `${(m - v.day_end_min) % 60} мин` : ''}`.trim(),
    })), { title: v.after_day_end ? 'Вернуть блокировку до' : 'Продлить учёбу сегодня', note: 'Только на сегодня. Сдвинуть обратно на раньше нельзя.' });
    if (!minutes) return;
    const ok = await run(() => call('extend_day_end', { minutes }).then(() => true), b);
    if (ok) snack(`Учёба сегодня — до ${hm(minutes)}`);
  }

  function controls(v) {
    const p = v.phase;
    const c = v.can;
    const items = [];
    const B = (act, label, cls, ic) => items.push({ act, label, cls, ic });
    switch (p.kind) {
      case 'idle':
        if (c.start_day) B('start_day', 'Начать день', 'filled xl', 'play');
        if (c.single) B('single', 'Один таймер', c.start_day ? 'tonal lg' : 'filled lg', 'timer');
        break;
      case 'work':
        if (c.resume) B('resume', 'Продолжить', 'filled xl', 'play');
        else if (c.pause) B('pause', 'Пауза', 'filled xl', 'pause');
        break;
      case 'break':
        if (c.resume) B('resume', 'Продолжить перерыв', 'filled lg', 'play');
        else if (c.pause) B('pause', 'Пауза', 'tonal lg', 'pause');
        B('start_next', 'Начать сейчас', c.resume ? 'tonal lg' : 'filled lg', 'skip');
        if (c.lunch) B('lunch', 'Обед', 'outlined lg', 'restaurant');
        break;
      case 'lunch_break':
        B('start_next', 'Закончить обед раньше', 'tonal lg', 'skip');
        break;
      case 'await':
        B('start_next', startLabel(p), 'filled xl', 'play');
        if (c.lunch) B('lunch', 'Обед', 'tonal lg', 'restaurant');
        break;
      case 'lunch':
        B('start_next', 'Пообедал — начать', 'filled xl', 'play');
        break;
      case 'done':
        if (c.single) B('single', 'Один таймер', 'outlined lg', 'timer');
        break;
    }
    if (c.stop_single) B('stop_single', 'Стоп', 'outlined lg', 'stop');
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
    const m = p.subtitle.match(/часть (\d+) из (\d+)/);
    if (!m) return 'Начать';
    return m[1] === '1' ? `Начать «${p.subtitle.split(' · ')[0]}»` : `Начать часть ${m[1]}`;
  }

  function access(v, meta) {
    const el = $('access');
    const p = v.pause;
    let html = '';
    let warn = false;
    if (p && v.lock.base) {
      if (meta.pause_access) {
        const open = p.access_left_ms > 0;
        warn = !open;
        html = `<div class="row1">${icon(open ? 'lock_open' : 'lock')}
          <div class="grow"><div class="title-m">${open ? 'Доступ на паузе открыт' : 'Доступ на паузе закрыт'}</div>
          <div class="body-m muted">${open ? 'Потом блокировка вернётся, даже если пауза продолжится' : `Пауза идёт ${dur(p.paused_ms)}. Продлить — через мини-капчу`}${p.extensions ? ` · продлений: ${p.extensions}` : ''}</div></div>
          ${open ? `<div class="count tnum">${mmss(p.access_left_ms)}</div>` : ''}</div>
          <div class="hstack"><button class="btn tonal interactive" data-act="extend">${icon('add')}Продлить на ${meta.pause_access_min} мин</button></div>`;
      } else {
        html = `<div class="row1">${icon('pause')}<div class="grow"><div class="title-m">Пауза ${dur(p.paused_ms)}</div>
          <div class="body-m muted">Заблокированное остаётся закрытым. Доступ на паузе включается в настройках блокировки — вне учебного дня.</div></div></div>`;
      }
    }
    // Only rebuild when the structure changes; the countdown text updates in place.
    const sig = html.replace(/\d+:\d+|\d+ (ч|мин)/g, '#');
    el.hidden = !html;
    el.classList.toggle('warn', warn);
    if (sig !== accessSig) { el.innerHTML = html; accessSig = sig; }
    else {
      const cnt = el.querySelector('.count');
      if (cnt && p) cnt.textContent = mmss(p.access_left_ms);
      const d = el.querySelector('.body-m');
      const tmp = document.createElement('div');
      tmp.innerHTML = html;
      const nd = tmp.querySelector('.body-m');
      if (d && nd && d.textContent !== nd.textContent) d.textContent = nd.textContent;
    }
  }

  function blocks(v) {
    const ol = $('blocks');
    $('total').textContent = v.blocks.length ? `${dur(v.work_ms)} из ${dur(v.planned_ms)}` : '';
    $('total-bar').style.setProperty('--v', v.planned_ms ? Math.min(1, v.work_ms / v.planned_ms) : 0);
    $('total-bar').hidden = !v.blocks.length;
    if (!v.blocks.length) {
      ol.innerHTML = `<li class="empty-plan"><p class="body-m muted">План на сегодня пуст.</p><button class="btn tonal interactive" data-act="goplan" style="margin-top:12px">${icon('add')}Составить план</button></li>`;
      return;
    }
    const html = v.blocks.map((b) => {
      const st = b.done ? icon('check', 's20') : '<span class="dot"></span>';
      const meta = `${Math.floor(b.work_ms / 60000)} из ${b.minutes} мин · ${b.done ? 'готово' : `часть ${Math.min(b.parts_done + 1, b.parts)}/${b.parts}`}`;
      return `<li class="blk ${b.done ? 'done' : ''} ${b.current ? 'current' : ''}">
        <span class="st">${st}</span><span class="name ellipsis">${esc(b.name)}</span><span class="meta tnum">${meta}</span>
        <div class="linear" style="--v:${Math.min(1, b.work_ms / (b.minutes * 60000))}"></div></li>`;
    }).join('');
    if (ol.dataset.html !== html) {
      ol.innerHTML = html;
      ol.dataset.html = html;
      v.blocks.forEach((b, i) => {
        if (b.done && prevDone[i] === false) ol.children[i]?.classList.add('just-done');
      });
    }
    prevDone = v.blocks.map((b) => b.done);
  }

  function update(s) {
    last = s;
    const v = s.view;
    const p = v.phase;
    const hero = $('hero');
    hero.dataset.kind = p.kind;
    hero.dataset.paused = String(!!p.paused);
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
        state = p.paused ? 'пауза' : p.kind === 'work' ? 'работа' : 'перерыв';
        break;
      case 'await':
        art = 'await';
        sub = `${p.subtitle} · ждём ${mmss(p.waiting_ms)}`;
        break;
      case 'lunch':
        art = 'lunch';
        sub = `Обед без таймера · уже ${mmss(p.elapsed_ms)}`;
        break;
      case 'done':
        art = 'day';
        sub = `${dur(v.work_ms)} учёбы`;
        break;
      default:
        big = v.planned_ms ? dur(v.planned_ms) : '—';
        small = true;
        state = v.mode === 'single' ? '' : 'в плане';
        title = v.started ? p.title : (v.study_day ? 'Готов начать?' : 'Выходной');
        sub = v.started ? '' : (v.blocks.length ? `${dur(v.planned_ms)} · ${v.blocks.map((b) => b.name).join(' · ')}` : 'Добавь блоки в план');
        if (!v.started && v.mode !== 'single') art = 'idle';
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
    blocks(v);
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
      case 'start_day': await run(() => call('start_day'), b); break;
      case 'pause': await run(() => call('pause'), b); break;
      case 'resume': await run(() => call('resume'), b); break;
      // `expect`: the backend refuses if the phase moved on since this button was drawn.
      case 'start_next': await run(() => call('start_next', { expect: v.phase.kind }), b); break;
      case 'stop_single': await run(() => call('stop_single'), b); break;
      case 'end_access': await run(() => call('end_access'), b); break;
      case 'mini': await call('toggle_mini'); break;
      case 'goplan': ctx.navigate('plan'); break;
      case 'kind': await pickKind(b); break;
      case 'dayend': await extendDayEnd(b); break;
      case 'lunch': await lunchDialog(last.meta.lunch_min); break;
      case 'single': await singleDialog(); break;
      case 'emergency': {
        const cfg = await call('get_config');
        const ok = await emergencyDialog(cfg, v.emergency_count);
        if (ok) snack(`Аварийный доступ на ${cfg.emergency_min} мин. Записано в лог.`);
        break;
      }
      case 'extend': {
        const ok = await captchaDialog(last.meta.pause_access_min);
        if (ok) snack(`Доступ продлён на ${last.meta.pause_access_min} мин`);
        break;
      }
    }
  });

  return { update };
}

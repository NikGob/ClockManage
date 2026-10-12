import { call, mmss, dur, esc } from '../api.js';
import { icon, morphIcon } from '../icons.js';
import { doodle, play } from '../doodles.js';
import { WaveRing } from '../wave.js';
import { run, snack, menu, burst, KINDS, KIND_RANK, kindLabel, planSummary, hm, tsHm } from '../ui.js';
import { L } from '../i18n.js';
import { singleDialog, emergencyDialog, captchaDialog, dayEndDialog, finishDialog, segmentDialog } from './dialogs.js';

export function endLabel(v) {
  return v.day_end + (v.day_end_next_day ? NIGHT() : '');
}

/** " ночи": a time after midnight that still belongs to today. */
const NIGHT = () => L(' ночи', ' (night)');

/** Minutes after the day's midnight (may run past 24:00) -> "01:00". */
const dayHm = (m) => hm(m % 1440);

export function mountToday(root, ctx) {
  root.innerHTML = `
  <div class="today">
    <section class="hero" id="hero" aria-label="${L('Таймер', 'Timer')}">
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
      <section class="surface note-card" id="note-card" hidden></section>
      <section class="surface plan-card" aria-labelledby="plan-h">
        <div class="head">
          <div class="row1"><h2 id="plan-h">${L('План дня', 'Day plan')}</h2><button class="kind-chip interactive" id="kind" data-act="kind" aria-haspopup="menu"></button><button class="btn text interactive" id="edit-plan">${L('Изменить', 'Edit')}</button></div>
          <div class="total tnum" id="total"></div>
          <div class="linear" id="total-bar"></div>
          <div class="dayend" id="dayend"></div>
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
  ctx.setActions(`<button class="btn tonal interactive" data-top="mini">${icon('pip')}${L('Мини-таймер', 'Mini timer')}</button>`);
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
    art.className = `art ${{ await: 'ringing', lunch: 'steaming', idle: 'writing', rest: 'resting' }[kind] || ''}`;
    play(art);
  }

  function lockLine(v, meta) {
    const lk = v.lock;
    let ic = 'lock_open', text = '', when = '', cls = '', action = '';
    const left = lk.until ? mmss(lk.until - v.now) : '';
    // The day end is a quiet button: click → move it for today (later, or earlier via "другое время").
    const endBtn = (label) => (v.can.set_day_end
      ? `<button class="when-btn interactive" data-act="dayend" aria-haspopup="menu" data-tip="${L('Конец дня на сегодня — позже или раньше', 'Today’s day end — later or earlier')}">${esc(label)}${icon('expand')}</button>`
      : esc(label));
    const offText = v.kind === 'off' ? L('Выходной — без блокировки', 'Day off — no lock') : L(`${kindLabel(v.kind)} день без блокировки`, `${kindLabel(v.kind)} day without a lock`);
    switch (lk.reason) {
      case 'study': ic = 'lock'; text = L('Блокировка включена', 'Lock on'); when = `${endBtn(`${L('до', 'until')} ${endLabel(v)}`)} ${L('или до конца плана', 'or until the plan is done')}`; break;
      case 'single': ic = 'lock'; text = L('Блокировка на время таймера', 'Locked for the timer'); break;
      case 'focus': ic = 'lock'; text = L('Фокус-блокировка', 'Focus lock'); when = `${L('до', 'until')} ${tsHm(lk.focus_until, meta.tz_offset_min)}`; break;
      case 'pause_access': text = L('Доступ открыт на паузе', 'Access open on pause'); when = left; cls = 'open'; action = 'end'; break;
      case 'emergency': text = L('Аварийный доступ', 'Emergency access'); when = left; cls = 'open'; action = 'end'; break;
      case 'lunch_at_pc': text = L('Обед за ПК — доступ открыт', 'Lunch at the PC — access open'); when = left; cls = 'open'; break;
      case 'segment_access': text = `${v.phase.title} — ${L('доступ открыт', 'access open')}`; when = left; cls = 'open'; break;
      case 'not_started':
        text = v.study_day ? L('Блокировка включится по кнопке «Начать день»', 'The lock turns on with “Start day”') : offText;
        if (v.study_day) when = `${L('и продержится', 'and holds')} ${endBtn(`${L('до', 'until')} ${endLabel(v)}`)}`;
        break;
      case 'completed': text = L('Все блоки отсижены — блокировка снята', 'All blocks done — the lock is off'); break;
      case 'day_end':
        text = L(`После ${endLabel(v)} блокировки нет`, `No lock after ${endLabel(v)}`);
        if (v.started && v.can.extend_day_end) when = endBtn(L('Вернуть блокировку', 'Bring the lock back'));
        break;
      case 'not_study_day': text = offText; break;
      default: text = '';
    }
    if (lk.focus_until && (lk.reason === 'study' || lk.reason === 'single')) when += ` · ${L('фокус до', 'focus until')} ${tsHm(lk.focus_until, meta.tz_offset_min)}`;
    if (lk.base && !['pause_access', 'emergency'].includes(lk.reason) && v.can.emergency) action = 'emergency';
    if (!meta.admin && lk.base) { text += L(' · нет прав администратора', ' · no administrator rights'); cls = 'open'; }
    const btn = action === 'end'
      ? `<button class="btn text interactive" data-act="end_access">${L('Закрыть доступ', 'Close access')}</button>`
      : action === 'emergency' ? `<button class="sos interactive" data-act="emergency" aria-label="${L('Аварийный доступ', 'Emergency access')}">${icon('warning')}<span class="lbl">${L('Аварийно', 'Emergency')}</span></button>` : '';
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
    b.dataset.tip = v.can.lighter_kind ? L('Тип сегодняшнего дня: свой план и блокировка', 'Today’s day type: its own plan and lock') : L('Во время учёбы день можно сделать только плотнее', 'During study the day can only get denser');
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
        sub: `${planSummary(p.plan)}${p.block ? '' : L(' · без блокировки', ' · no lock')}`,
        disabled: lighter && !v.can.lighter_kind,
        tip: lighter && !v.can.lighter_kind ? L('Во время учёбы — только плотнее', 'During study — only denser') : '',
      };
    }), {
      title: L('Тип дня', 'Day type'),
      note: v.completed ? L('День уже закрыт: при более плотном типе он откроется снова — новые блоки добавятся, блокировка вернётся.', 'The day is closed: a denser type reopens it — the new blocks are added and the lock comes back.')
        : v.started ? L('День уже идёт: блоки нового типа добавятся к плану, начатое не пропадёт.', 'The day is running: the new type’s blocks are added to the plan, nothing started is lost.') : L('План на сегодня заменится шаблоном этого типа.', 'Today’s plan is replaced with this type’s template.'),
    });
    if (!kind || kind === cur) return;
    const ok = await run(() => call('set_day_kind', { kind }).then(() => true), b);
    if (ok) snack(L(`Сегодня — ${kindLabel(kind).toLowerCase()} день`, `Today is a ${kindLabel(kind).toLowerCase()} day`));
  }

  async function extendDayEnd(b) {
    const v = last.view;
    const MAX = 26 * 60; // 02:00 of the night after
    const opts = [];
    if (v.can.extend_day_end) {
      const base = Math.max(v.day_end_min, v.now_min);
      for (const add of [30, 60, 120]) {
        const m = Math.ceil((base + add) / 5) * 5;
        if (m <= MAX && m > v.day_end_min && !opts.includes(m)) opts.push(m);
      }
    }
    const plus = (d) => `+${d >= 60 ? `${Math.floor(d / 60)} ${L('ч', 'h')} ` : ''}${d % 60 ? `${d % 60} ${L('мин', 'min')}` : ''}`.trim();
    const items = opts.map((m) => ({ value: m, label: `${L('до', 'until')} ${dayHm(m)}${m >= 1440 ? NIGHT() : ''}`, icon: icon('schedule'), sub: plus(m - v.day_end_min) }));
    items.push({ value: 'custom', label: L('Другое время…', 'Other time…'), icon: icon('edit'), sub: L('Позже или раньше, до 02:00 ночи, с причиной', 'Later or earlier, up to 02:00 at night, with a reason') });
    const choice = await menu(b, items, {
      title: v.after_day_end ? L('Вернуть блокировку до', 'Bring the lock back until') : L('Конец дня сегодня', 'Today’s day end'),
      note: L(`Только на сегодня — в настройках остаётся ${v.day_end_base}. Каждый сдвиг пишется в лог.`, `Today only — the setting stays ${v.day_end_base}. Every shift goes to the log.`),
    });
    if (!choice) return;
    if (choice === 'custom') {
      const r = await dayEndDialog(v);
      if (r) snack(r.changed ? L(`Конец дня сегодня: ${r.new}${r.new_is_next_day ? NIGHT() : ''}. Настройки не тронуты.`, `Today’s day end: ${r.new}${r.new_is_next_day ? NIGHT() : ''}. Settings untouched.`) : L('Конец дня не изменился', 'The day end didn’t change'));
      return;
    }
    const ok = await run(() => call('extend_day_end', { minutes: choice }).then(() => true), b);
    if (ok) snack(L(`Учёба сегодня — до ${dayHm(choice)}`, `Studying today until ${dayHm(choice)}`) + (choice >= 1440 ? NIGHT() : ''));
  }

  function controls(v) {
    const p = v.phase;
    const c = v.can;
    const items = [];
    const B = (act, label, cls, ic) => items.push({ act, label, cls, ic });
    switch (p.kind) {
      case 'idle':
        if (c.start_day) B('start_day', L('Начать день', 'Start day'), 'filled xl', 'play');
        if (c.single) B('single', L('Один таймер', 'Single timer'), c.start_day ? 'tonal lg' : 'filled lg', 'timer');
        break;
      case 'work':
        if (c.resume) B('resume', L('Продолжить', 'Resume'), 'filled xl', 'play');
        else if (c.pause) B('pause', L('Пауза', 'Pause'), 'filled xl', 'pause');
        break;
      case 'break':
        if (c.resume) B('resume', L('Продолжить перерыв', 'Resume the break'), 'filled lg', 'play');
        else if (c.pause) B('pause', L('Пауза', 'Pause'), 'tonal lg', 'pause');
        B('start_next', L('Начать сейчас', 'Start now'), c.resume ? 'tonal lg' : 'filled lg', 'skip');
        if (c.segment) B('segment', L('Отрезок', 'Segment'), 'outlined lg', 'restaurant');
        break;
      case 'lunch_break':
        B('start_next', L('Закончить обед раньше', 'End lunch early'), 'tonal lg', 'skip');
        break;
      case 'await':
        B('start_next', startLabel(p), 'filled xl', 'play');
        if (c.segment) B('segment', L('Отрезок', 'Segment'), 'tonal lg', 'restaurant');
        break;
      case 'segment':
        if (c.lay_down) {
          // Getting ready: the countdown and the alarm start only from "Лёг".
          B('lay_down', L('Лёг', 'In bed'), 'filled xl', 'bed');
          B('end_segment', p.alarm ? L('Не буду спать', 'Skip the nap') : L('Отменить', 'Cancel'), 'text lg', 'close');
        } else {
          B('end_segment', p.alarm ? L('Встал', 'I’m up') : L(`Закончил ${p.title.toLowerCase()}`, `Done with ${p.title.toLowerCase()}`), 'filled xl', p.alarm ? 'alarm' : 'check');
        }
        if (c.segment) B('segment', L('В очередь', 'Queue'), 'outlined lg', 'add');
        if (c.segment_mode) B('seg_mode', p.stopwatch ? L('По таймеру', 'Timed') : L('Без времени', 'Untimed'), 'text lg', p.stopwatch ? 'schedule' : 'timer');
        break;
      case 'lunch':
        B('start_next', L('Пообедал — начать', 'Done eating — start'), 'filled xl', 'play');
        break;
      case 'done':
        if (c.single) B('single', L('Один таймер', 'Single timer'), 'outlined lg', 'timer');
        break;
    }
    // Quiet on purpose: a text button after the main ones, behind a hold-to-confirm dialog.
    if (c.finish_block) B('finish', L('Закрыть блок…', 'Close block…'), 'text lg', 'check');
    if (c.stop_single) B('stop_single', L('Стоп', 'Stop'), 'outlined lg', 'stop');
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
    const m = p.subtitle.match(/(?:часть|part) (\d+) (?:из|of) (\d+)/i);
    if (!m) return L('Начать', 'Start');
    return m[1] === '1' ? L(`Начать «${p.subtitle.split(' · ')[0]}»`, `Start “${p.subtitle.split(' · ')[0]}”`) : L(`Начать часть ${m[1]}`, `Start part ${m[1]}`);
  }

  function access(v, meta) {
    const el = $('access');
    const p = v.pause;
    let html = '';
    let warn = false;
    if (p && v.lock.base) {
      if (meta.pause_access && !v.lock.focus_until) {
        const open = p.access_left_ms > 0;
        warn = !open;
        html = `<div class="row1">${icon(open ? 'lock_open' : 'lock')}
          <div class="grow"><div class="title-m">${open ? L('Доступ на паузе открыт', 'Access on pause is open') : L('Доступ на паузе закрыт', 'Access on pause is closed')}</div>
          <div class="body-m muted">${open ? L('Потом блокировка вернётся, даже если пауза продолжится', 'Then the lock comes back, even if the pause goes on') : L(`Пауза идёт ${dur(p.paused_ms)}. Продлить — через мини-капчу`, `Paused for ${dur(p.paused_ms)}. Extend with a mini captcha`)}${p.extensions ? ` · ${L('продлений', 'extensions')}: ${p.extensions}` : ''}</div></div>
          ${open ? `<div class="count tnum">${mmss(p.access_left_ms)}</div>` : ''}</div>
          <div class="hstack"><button class="btn tonal interactive" data-act="extend">${icon('add')}${L(`Продлить на ${meta.pause_access_min} мин`, `Extend by ${meta.pause_access_min} min`)}</button></div>`;
      } else {
        html = `<div class="row1">${icon('pause')}<div class="grow"><div class="title-m">${L('Пауза', 'Paused')} ${dur(p.paused_ms)}</div>
          <div class="body-m muted">${L('Заблокированное остаётся закрытым. Доступ на паузе включается в настройках блокировки — вне учебного дня.', 'Blocked things stay closed. Access on pause is turned on in the blocking settings — outside a study day.')}</div></div></div>`;
      }
    }
    // Only rebuild when the structure changes; the countdown text updates in place.
    const sig = html.replace(/\d+:\d+|\d+ (ч|мин|h|min)/g, '#');
    el.hidden = !html;
    el.classList.toggle('warn', warn);
    if (sig !== accessSig) { el.innerHTML = html; accessSig = sig; }
    else {
      // Same structure: refresh every live text in place (title "Пауза 12 мин", note, countdown).
      const tmp = document.createElement('div');
      tmp.innerHTML = html;
      for (const sel of ['.title-m', '.body-m', '.count']) {
        const cur = el.querySelector(sel);
        const next = tmp.querySelector(sel);
        if (cur && next && cur.textContent !== next.textContent) cur.textContent = next.textContent;
      }
    }
  }

  // Today's day end: one-off shift, and whether the rest of the plan fits before it.
  function dayEnd(v) {
    const el = $('dayend');
    const f = v.forecast;
    let fit = '';
    if (v.mode === 'plan' && !v.completed && f.work_left_ms > 0) {
      fit = v.after_day_end
        ? ''
        : f.fits
          ? `<span class="ok">${L(`План влезает, запас ${dur(f.margin_ms)}`, `The plan fits, ${dur(f.margin_ms)} to spare`)}</span>`
          : `<span class="bad">${L(`План не влезает на ${dur(-f.margin_ms)}`, `The plan is ${dur(-f.margin_ms)} too long`)}</span>`;
    }
    const html = `${icon('alarm')}<span class="grow">${L('Конец дня', 'Day end')} <b class="tnum">${esc(endLabel(v))}</b>${v.day_end_changed ? ` <span class="was">· ${L('обычно', 'usually')} ${esc(v.day_end_base)}</span>` : ''}${fit ? `<br>${fit}` : ''}</span>
      ${v.can.set_day_end ? `<button class="btn text interactive" data-act="dayend" aria-haspopup="menu">${L('Изменить', 'Change')}</button>` : ''}`;
    if (el.dataset.html !== html) { el.innerHTML = html; el.dataset.html = html; }
  }

  function blocks(v) {
    const ol = $('blocks');
    $('total').textContent = v.blocks.length ? L(`${dur(v.work_ms)} из ${dur(v.planned_ms)} учёбы`, `${dur(v.work_ms)} of ${dur(v.planned_ms)} of study`) : '';
    $('total-bar').style.setProperty('--v', v.planned_ms ? Math.min(1, v.work_ms / v.planned_ms) : 0);
    $('total-bar').hidden = !v.blocks.length;
    if (!v.blocks.length) {
      ol.innerHTML = `<li class="empty-plan"><p class="body-m muted">${L('План на сегодня пуст.', 'Today’s plan is empty.')}</p><button class="btn tonal interactive" data-act="goplan" style="margin-top:12px">${icon('add')}${L('Составить план', 'Make a plan')}</button></li>`;
      return;
    }
    const html = v.blocks.map((b) => {
      if (b.kind === 'break') {
        const st = b.done ? (b.started ? L('прошёл', 'done') : L('пропущен', 'skipped')) : b.started ? L('идёт', 'running') : b.queued ? L('в очереди', 'queued') : L('отрезок', 'segment');
        return `<li class="blk seg ${b.done ? 'done' : ''}"><span class="st">${b.done ? icon('check', 's20') : icon('coffee', 's20')}</span>
          <span class="name ellipsis">${esc(b.name)}</span><span class="meta tnum">${b.minutes} ${L('мин', 'min')} · ${st}</span></li>`;
      }
      const st = b.done ? icon('check', 's20') : '<span class="dot"></span>';
      const meta = `${Math.floor(b.work_ms / 60000)} ${L('из', 'of')} ${b.minutes} ${L('мин', 'min')} · ${b.done ? L('готово', 'done') : `${L('часть', 'part')} ${Math.min(b.parts_done + 1, b.parts)}/${b.parts}`}`;
      return `<li class="blk ${b.done ? 'done' : ''} ${b.current ? 'current' : ''}">
        <span class="st">${st}</span><span class="name ellipsis">${esc(b.name)}</span><span class="meta tnum">${meta}</span>
        <div class="linear" style="--v:${Math.min(1, b.work_ms / (b.minutes * 60000))}"></div></li>`;
    }).join('');
    if (ol.dataset.html !== html) {
      ol.innerHTML = html;
      ol.dataset.html = html;
      v.blocks.forEach((b, i) => {
        if (b.done && prevDone[i] === false) {
          ol.children[i]?.classList.add('just-done');
          burst(ol.children[i]?.querySelector('.st'));
        }
      });
    }
    prevDone = v.blocks.map((b) => b.done);
  }

  // End-of-block line, for when the overlay card is off or was dismissed.
  let noteFor = null;
  function noteCard(v) {
    const el = $('note-card');
    const pn = v.pending_note;
    el.hidden = !pn;
    if (!pn) { noteFor = null; return; }
    if (noteFor === pn.block) return; // keep what is being typed
    noteFor = pn.block;
    el.innerHTML = `<div class="title-m">${L(`«${esc(pn.name)}» закрыт`, `“${esc(pn.name)}” closed`)}</div>
      <div class="field"><label for="note-in">${L('Что было скучно, куда отвлекался?', 'What was boring, what distracted you?')}</label><input id="note-in" maxlength="300" autocomplete="off"></div>
      <div class="hstack"><button class="btn tonal interactive" data-act="note_save">${icon('check')}${L('Сохранить', 'Save')}</button>
        <button class="btn text interactive" data-act="note_skip">${L('Пропустить', 'Skip')}</button></div>`;
    el.querySelector('#note-in').addEventListener('keydown', (e) => { if (e.key === 'Enter') el.querySelector('[data-act="note_save"]').click(); });
  }

  function update(s) {
    last = s;
    const v = s.view;
    const p = v.phase;
    const hero = $('hero');
    hero.dataset.kind = p.kind;
    hero.dataset.paused = String(!!p.paused);
    hero.dataset.over = String(p.kind === 'segment' && p.remaining_ms < 0 && !p.stopwatch);
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
        state = p.paused ? L('пауза', 'paused') : p.kind === 'work' ? L('работа', 'work') : L('перерыв', 'break');
        break;
      case 'segment': {
        const over = p.remaining_ms < 0;
        if (p.stopwatch) {
          // "Без времени": counts up; the ring fills to the usual length and stays full.
          frac = p.dur_ms ? Math.min(1, p.elapsed_ms / p.dur_ms) : 0;
          running = true;
          big = mmss(p.elapsed_ms);
          state = over ? L('без времени · дольше обычного', 'untimed · longer than usual') : L('без времени', 'untimed');
          break;
        }
        frac = over ? 1 : p.elapsed_ms / p.dur_ms;
        running = !over;
        big = over ? `+${mmss(-p.remaining_ms)}` : mmss(p.remaining_ms);
        state = p.prep ? (over ? L('пора ложиться', 'time to lie down') : L('подготовка', 'getting ready')) : over ? L('превышено', 'over') : p.alarm ? L('сон · будильник', 'nap · alarm') : L('отрезок', 'segment');
        break;
      }
      case 'await':
        art = 'await';
        sub = `${p.subtitle} · ${L('ждём', 'waiting')} ${mmss(p.waiting_ms)}`;
        break;
      case 'lunch':
        art = 'lunch';
        sub = `${L('Обед без таймера · уже', 'Lunch without a timer ·')} ${mmss(p.elapsed_ms)}`;
        break;
      case 'done':
        art = 'day';
        sub = L(`${dur(v.work_ms)} учёбы`, `${dur(v.work_ms)} of study`);
        break;
      default:
        big = v.planned_ms ? dur(v.planned_ms) : '—';
        small = true;
        state = v.mode === 'single' ? '' : L('в плане', 'planned');
        title = v.started ? p.title : (v.study_day ? L('Готов начать?', 'Ready to start?') : L('Выходной', 'Day off'));
        sub = v.started ? '' : (v.blocks.length ? `${L(`${dur(v.planned_ms)} учёбы`, `${dur(v.planned_ms)} of study`)} · ${v.blocks.filter((b) => b.kind !== 'break').map((b) => b.name).join(' · ')}` : L('Добавь блоки в план', 'Add blocks to the plan'));
        if (!v.started && v.mode !== 'single') art = v.study_day ? 'idle' : 'rest';
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
    noteCard(v);
    blocks(v);
    dayEnd(v);
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
      case 'start_next': {
        const wasBreak = v.phase.kind === 'break' || v.phase.kind === 'lunch_break';
        const ok = await run(() => call('start_next', { expect: v.phase.kind }).then(() => true), b);
        // Skipping a break is one click; a misclick is taken back within 10 seconds.
        if (ok && wasBreak) {
          snack(v.phase.kind === 'lunch_break' ? L('Обед закончен раньше', 'Lunch ended early') : L('Перерыв пропущен', 'Break skipped'), 10000, {
            label: L('Отменить', 'Cancel'),
            run: () => run(() => call('undo_skip').then(() => snack(L('Перерыв продолжается', 'The break goes on')))),
          });
        }
        break;
      }
      case 'stop_single': await run(() => call('stop_single'), b); break;
      case 'end_access': await run(() => call('end_access'), b); break;
      case 'mini': await call('toggle_mini'); break;
      case 'goplan': ctx.navigate('plan'); break;
      case 'kind': await pickKind(b); break;
      case 'dayend': await extendDayEnd(b); break;
      case 'segment': {
        const picked = await segmentDialog(last.meta.segments || [], v.phase.kind === 'segment', v.phase.kind === 'segment' ? v.phase.queue : []);
        if (picked?.length) snack(picked.map((t) => t.name).join(' → ') + (v.phase.kind === 'segment' ? L(' — в очереди', ' — queued') : L(' — пошёл отсчёт', ' — countdown started')));
        break;
      }
      case 'end_segment': await run(() => call('end_segment'), b); break;
      case 'lay_down': {
        const ok = await run(() => call('lay_down').then(() => true), b);
        if (ok) snack(`${v.phase.title}: ${L('отсчёт пошёл', 'countdown started')}${v.phase.alarm ? L(', в конце будильник', ', alarm at the end') : ''}`);
        break;
      }
      case 'seg_mode': {
        const ok = await run(() => call('set_segment_mode', { stopwatch: !v.phase.stopwatch }).then(() => true), b);
        if (ok) snack(v.phase.stopwatch ? L('Снова обратный отсчёт', 'Counting down again') : L('Без времени — секундомер, без напоминаний о конце', 'Untimed — a stopwatch, no end reminders'));
        break;
      }
      case 'single': await singleDialog(); break;
      case 'note_save':
      case 'note_skip': {
        const text = act === 'note_save' ? root.querySelector('#note-in')?.value.trim() : '';
        const ok = await run(() => call('set_block_note', { block: v.pending_note.block, note: text || null }).then(() => true), b);
        if (ok && text) snack(L('Строка сохранена в блок', 'Line saved to the block'));
        break;
      }
      case 'finish': {
        const ok = await finishDialog(v);
        if (ok) snack(L('Блок закрыт на отработанном — записано в лог', 'Block closed at the time worked — logged'));
        break;
      }
      case 'emergency': {
        const cfg = await call('get_config');
        const ok = await emergencyDialog(cfg, v.emergency_count);
        if (ok) snack(L(`Аварийный доступ на ${cfg.emergency_min} мин. Записано в лог.`, `Emergency access for ${cfg.emergency_min} min. Logged.`));
        break;
      }
      case 'extend': {
        const ok = await captchaDialog(last.meta.pause_access_min);
        if (ok) snack(L(`Доступ продлён на ${last.meta.pause_access_min} мин`, `Access extended by ${last.meta.pause_access_min} min`));
        break;
      }
    }
  });

  return { update };
}

import { call, esc } from '../api.js';
import { icon, CHECK } from '../icons.js';
import { run, snack, ask, hoursLabel, partsPreview, KINDS, kindLabel, menu } from '../ui.js';
import { L } from '../i18n.js';

const STEP = 15;
const SEG_STEP = 5;
const isSeg = (r) => r.type === 'break';
/** What goes to the backend: `type` only for segments. */
const item = (r) => (isSeg(r) ? { name: r.name.trim(), minutes: r.minutes, type: 'break' } : { name: r.name.trim(), minutes: r.minutes });

export function mountPlan(root, ctx) {
  let rows = [];        // editable copy [{name, minutes, orig?:{name, minutes}, progress?}]
  let base = [];        // plan as it was loaded (for dirty check / locked rules)
  let locked = false;
  let started = false;
  let seg = 45;
  let dirty = false;
  let loadedFor = '';
  // 'today' edits today's plan; a kind ('full' | 'light' | 'off') edits that kind's template.
  let target = ctx.planTab || 'today';
  let todayKind = 'full';
  let cfg = null;
  ctx.planTab = null;

  root.innerHTML = `
    <div class="readable">
      <div class="plan-tabs">
        <div class="segmented" role="tablist" id="tabs">
          <button class="interactive" data-tab="today" role="tab" data-tip="${L('План только на сегодня', 'Plan for today only')}">${CHECK}${L('Сегодня', 'Today')}</button>
          ${KINDS.map((k) => `<button class="interactive" data-tab="${k.id}" role="tab" data-tip="${L(`Шаблон: так начинается каждый ${k.label.toLowerCase()} день`, `Template: every ${k.label.toLowerCase()} day starts like this`)}">${CHECK}${k.label}</button>`).join('')}
        </div>
      </div>
      <div id="lock-note"></div>
      <p class="body-m muted" style="margin-bottom:16px" id="hint"></p>
      <div class="plan-editor" id="rows"></div>
      <div class="hstack" style="margin-top:12px">
        <button class="btn outlined interactive" id="add">${icon('add')}${L('Добавить блок', 'Add block')}</button>
        <button class="btn text interactive" id="add-seg" data-tip="${L('Обед, сон, прогулка — стоит на своём месте в дне, в учебные часы не входит', 'Lunch, a nap, a walk — stands in its place in the day, not counted as study')}">${icon('coffee')}${L('Добавить отрезок', 'Add segment')}</button>
      </div>
      <div class="plan-footer">
        <label id="tpl-wrap" data-tip="${L('Следующие такие дни начнутся с этого плана', 'The next days of this type start with this plan')}"><input type="checkbox" class="check" id="tpl"> <span id="tpl-l">${L('Сохранить и как шаблон', 'Save, also as template')}</span></label>
        <span class="grow"></span>
        <button class="btn text interactive" id="reset">${L('Сбросить', 'Reset')}</button>
        <button class="btn filled interactive" id="save" disabled>${icon('check')}${L('Сохранить план', 'Save plan')}</button>
      </div>
    </div>`;
  const $ = (id) => root.querySelector('#' + id);

  function render() {
    const tpl = target !== 'today';
    root.querySelectorAll('[data-tab]').forEach((b) => {
      b.setAttribute('aria-pressed', String(b.dataset.tab === target));
      b.setAttribute('aria-selected', String(b.dataset.tab === target));
    });
    $('hint').innerHTML = tpl
      ? L(
        `Шаблон дня «${kindLabel(target)}»: с него начинается каждый такой день. Сегодняшний план он не меняет${target === todayKind && !started ? ' — кроме случая, когда сегодня ещё не начат и план не правился' : ''}.`,
        `The “${kindLabel(target)}” day template: every such day starts with it. It doesn’t change today’s plan${target === todayKind && !started ? ' — unless today hasn’t started and its plan wasn’t edited' : ''}.`,
      )
      : L(`Каждый блок идёт частями по <b>${seg}</b> мин с перерывами между ними. Перерывы не съедают учебное время.`, `Every block runs in parts of <b>${seg}</b> min with breaks between them. Breaks don’t eat into study time.`);
    $('tpl-wrap').hidden = tpl;
    $('tpl-l').textContent = L(`Сохранить и как шаблон «${kindLabel(todayKind)}»`, `Save, also as the “${kindLabel(todayKind)}” template`);
    $('save').lastChild.textContent = tpl ? L('Сохранить шаблон', 'Save template') : L('Сохранить план', 'Save plan');
    $('lock-note').innerHTML = locked && !tpl
      ? `<div class="lock-note">${icon('lock')}<div><div class="title-s">${L('Идёт учебный день', 'A study day is running')}</div><div class="body-m">${L('Не начатые блоки можно урезать и удалять, начатый — урезать не меньше уже отработанного. Отработанное время не стирается.', 'Blocks not started can be shortened and removed, a started one — shortened down to the time already worked. Worked time is never erased.')}</div></div></div>`
      : '';
    $('rows').innerHTML = rows.map((r, i) => {
      if (isSeg(r)) {
        const taken = !tpl && !!(r.progress?.started);
        return `<div class="prow seg${r.fresh ? ' fresh' : ''}" data-i="${i}">
        <input class="name" value="${esc(r.name)}" aria-label="${L('Отрезок', 'Segment')} ${i + 1}" maxlength="24" ${taken ? 'disabled' : ''}>
        <div class="stepper" role="group" aria-label="${L('Длительность', 'Duration')}">
          <button class="icon-btn interactive" data-a="minus" aria-label="${L('Меньше на', 'Less by')} ${SEG_STEP} ${L('мин', 'min')}" ${r.minutes <= SEG_STEP || taken ? 'disabled' : ''}>${icon('remove')}</button>
          <div class="val tnum${r.bump ? ' bump' : ''}">${r.minutes} ${L('мин', 'min')}<small>${taken ? (r.progress.done ? L('прошёл', 'done') : L('идёт', 'running')) : L('отрезок · не учёба', 'segment · not study')}</small></div>
          <button class="icon-btn interactive" data-a="plus" aria-label="${L('Больше на', 'More by')} ${SEG_STEP} ${L('мин', 'min')}" ${r.minutes + SEG_STEP > 240 || taken ? 'disabled' : ''}>${icon('add')}</button>
        </div>
        <div class="tools">
          <button class="icon-btn interactive" data-a="up" aria-label="${L('Выше', 'Up')}" ${i === 0 ? 'disabled' : ''}>${icon('up')}</button>
          <button class="icon-btn interactive" data-a="down" aria-label="${L('Ниже', 'Down')}" ${i === rows.length - 1 ? 'disabled' : ''}>${icon('down')}</button>
          <button class="icon-btn interactive" data-a="del" aria-label="${L('Удалить', 'Remove')}" data-tip="${taken ? L('Из плана уйдёт, а в журнале отрезок останется', 'It leaves the plan, but stays in the log') : ''}">${icon('delete')}</button>
        </div></div>`;
      }
      const o = tpl ? null : r.orig;
      // Started block during the lock: not below the whole minutes worked. A block with less than
      // a minute (a misclick start) may go; any other started block stays — close it instead.
      const minMinutes = o ? minFor(r) : STEP;
      const noise = !(r.progress?.work_ms >= 60000);
      const canDelete = tpl || !(r.progress?.started || r.progress?.work_ms > 0) || noise;
      const parts = partsPreview(r.minutes, seg);
      const doneMin = r.progress ? Math.floor(r.progress.work_ms / 60000) : 0;
      const cls = r.fresh ? 'prow fresh' : 'prow';
      return `<div class="${cls}" data-i="${i}">
        <input class="name" value="${esc(r.name)}" aria-label="${L('Название блока', 'Block name')} ${i + 1}" maxlength="40" ${o && r.progress?.started && !noise ? 'disabled' : ''}>
        <div class="stepper" role="group" aria-label="${L('Длительность', 'Duration')}">
          <button class="icon-btn interactive" data-a="minus" aria-label="${L('Меньше на', 'Less by')} ${STEP} ${L('мин', 'min')}" ${r.minutes <= minMinutes ? 'disabled' : ''}>${icon('remove')}</button>
          <div class="val tnum${r.bump ? ' bump' : ''}">${hoursLabel(r.minutes)}<small>${parts.join(' + ')} ${L('мин', 'min')}</small></div>
          <button class="icon-btn interactive" data-a="plus" aria-label="${L('Больше на', 'More by')} ${STEP} ${L('мин', 'min')}" ${r.minutes + STEP > 480 ? 'disabled' : ''}>${icon('add')}</button>
        </div>
        <div class="tools">
          <button class="icon-btn interactive" data-a="up" aria-label="${L('Выше', 'Up')}" ${i === 0 ? 'disabled' : ''}>${icon('up')}</button>
          <button class="icon-btn interactive" data-a="down" aria-label="${L('Ниже', 'Down')}" ${i === rows.length - 1 ? 'disabled' : ''}>${icon('down')}</button>
          <button class="icon-btn interactive" data-a="del" aria-label="${L('Удалить', 'Remove')}" ${canDelete ? '' : 'disabled'}>${icon('delete')}</button>
        </div>
        ${r.progress?.started ? `<div class="progress"><div class="linear" style="--v:${Math.min(1, doneMin / r.minutes)}"></div><span class="tnum">${doneMin} ${L('из', 'of')} ${r.minutes} ${L('мин', 'min')}${r.progress.done ? L(' · готово', ' · done') : ''}</span></div>` : ''}
      </div>`;
    }).join('') || `<p class="body-m muted">${tpl && target === 'off' ? L('В выходной плана нет — и это нормально. Можно добавить что-то лёгкое.', 'No plan on a day off — and that’s fine. You can add something light.') : L('Пусто. Добавь первый блок — например «Математика, 1,5 ч».', 'Empty. Add the first block — e.g. “Math, 1.5 h”.')}</p>`;
    rows.forEach((r) => { r.fresh = false; r.bump = false; });
    markDirty();
  }

  function minFor(r) {
    const touched = !!(r.progress?.started || r.progress?.work_ms > 0);
    // Whole minutes worked, rounded down — the same limit the backend and the agent see.
    return locked && touched ? Math.max(1, Math.floor(r.progress.work_ms / 60000)) : STEP;
  }

  function markDirty() {
    const cur = JSON.stringify(rows.map(item));
    dirty = cur !== JSON.stringify(base) || (target === 'today' && $('tpl').checked);
    $('save').disabled = !dirty || rows.some((r) => !r.name.trim()) || (target === 'today' && started && !rows.length);
    $('reset').disabled = !dirty;
  }

  async function load(force = false) {
    const [st, c] = await Promise.all([call('get_state'), call('get_config')]);
    cfg = c;
    const key = `${st.view.date}|${target}`;
    if (!force && dirty && loadedFor === key) return;
    loadedFor = key;
    seg = cfg.timing.work_segment_min;
    todayKind = st.view.kind;
    started = st.view.started;
    locked = st.view.lock.base && st.view.started && st.view.mode === 'plan' && st.view.lock.reason !== 'single';
    if (target === 'today') {
      base = st.view.blocks.map((b) => item({ name: b.name, minutes: b.minutes, type: b.kind }));
      rows = st.view.blocks.map((b) => ({
        name: b.name, minutes: b.minutes, type: b.kind, orig: { name: b.name, minutes: b.minutes },
        progress: { work_ms: b.work_ms, started: b.started, done: b.done },
      }));
    } else {
      base = cfg.profiles[target].plan.map(item);
      rows = base.map((b) => ({ ...b }));
    }
    $('tpl').checked = false;
    render();
  }

  $('rows').addEventListener('click', (e) => {
    const b = e.target.closest('[data-a]');
    if (!b) return;
    const i = Number(b.closest('.prow').dataset.i);
    const r = rows[i];
    switch (b.dataset.a) {
      case 'plus': r.minutes = isSeg(r) ? Math.min(240, r.minutes + SEG_STEP) : Math.min(480, r.minutes + STEP); r.bump = true; break;
      case 'minus': r.minutes = isSeg(r) ? Math.max(SEG_STEP, r.minutes - SEG_STEP) : Math.max(minFor(r), r.minutes - STEP); r.bump = true; break;
      case 'up': [rows[i - 1], rows[i]] = [rows[i], rows[i - 1]]; break;
      case 'down': [rows[i + 1], rows[i]] = [rows[i], rows[i + 1]]; break;
      case 'del': {
        const row = b.closest('.prow');
        row.classList.add('removing');
        setTimeout(() => { rows.splice(i, 1); render(); }, 140);
        return;
      }
    }
    render();
    // Moved rows slide into their new places instead of jumping (FLIP).
    if (b.dataset.a === 'up' || b.dataset.a === 'down') {
      const j = b.dataset.a === 'up' ? i - 1 : i + 1;
      const a = root.querySelector(`.prow[data-i="${j}"]`);
      const o = root.querySelector(`.prow[data-i="${i}"]`);
      if (a && o) {
        const dy = o.getBoundingClientRect().top - a.getBoundingClientRect().top;
        a.animate([{ transform: `translateY(${dy}px)` }, { transform: 'none' }], { duration: 380, easing: 'cubic-bezier(.2,0,0,1)' });
        o.animate([{ transform: `translateY(${-dy}px)` }, { transform: 'none' }], { duration: 380, easing: 'cubic-bezier(.2,0,0,1)' });
        a.animate([{ boxShadow: '0 6px 18px rgb(0 0 0 / .18)', zIndex: 2 }, { boxShadow: 'none', zIndex: 2 }], { duration: 380 });
      }
    }
    root.querySelector(`.prow[data-i="${b.dataset.a === 'up' ? i - 1 : b.dataset.a === 'down' ? i + 1 : i}"] [data-a="${b.dataset.a}"]`)?.focus();
  });
  $('rows').addEventListener('input', (e) => {
    if (!e.target.classList.contains('name')) return;
    rows[Number(e.target.closest('.prow').dataset.i)].name = e.target.value;
    markDirty();
  });
  $('tpl').addEventListener('change', markDirty);
  $('add').addEventListener('click', () => {
    rows.push({ name: '', minutes: 90, fresh: true });
    render();
    root.querySelector('.prow:last-child input.name')?.focus();
  });
  $('add-seg').addEventListener('click', async (e) => {
    const types = (cfg?.segments || []);
    const pick = await menu(e.currentTarget, types.map((t, i) => ({ value: i, label: t.name, sub: `${t.minutes} ${L('мин', 'min')}${t.alarm ? L(' · будильник', ' · alarm') : ''}`, icon: icon(t.alarm ? 'alarm' : 'coffee') })), { title: L('Отрезок в плане', 'A segment in the plan'), note: L('Запустится сам, когда закроется блок перед ним.', 'Starts by itself when the block before it closes.') });
    if (pick === undefined) return;
    rows.push({ name: types[pick].name, minutes: types[pick].minutes, type: 'break', fresh: true });
    render();
  });
  $('reset').addEventListener('click', () => load(true));
  $('save').addEventListener('click', async (e) => {
    const blocks = rows.map(item);
    let ok;
    if (target === 'today') {
      ok = await run(() => call('set_plan', { blocks, saveTemplate: $('tpl').checked }).then(() => true), e.currentTarget);
      if (ok) snack($('tpl').checked ? L(`План сохранён и стал шаблоном «${kindLabel(todayKind)}»`, `Plan saved and made the “${kindLabel(todayKind)}” template`) : L('План на сегодня сохранён', 'Today’s plan saved'));
    } else {
      // Fresh config: the one loaded with this screen may be stale (MCP, another save).
      const next = await call('get_config');
      next.profiles[target].plan = blocks;
      ok = await run(() => call('save_config', { cfg: next }).then(() => true), e.currentTarget);
      if (ok) snack(L(`Шаблон «${kindLabel(target)}» сохранён`, `The “${kindLabel(target)}” template saved`));
    }
    if (ok) {
      dirty = false;
      await load(true);
    }
  });
  $('tabs').addEventListener('click', async (e) => {
    const b = e.target.closest('[data-tab]');
    if (!b || b.dataset.tab === target) return;
    if (dirty && !(await ask(L('Не сохранено', 'Not saved'), L('Изменения в этом плане пропадут.', 'Changes to this plan will be lost.'), L('Переключиться', 'Switch')))) return;
    target = b.dataset.tab;
    b.classList.add('just');
    setTimeout(() => b.classList.remove('just'), 600);
    dirty = false;
    load(true);
  });

  return {
    update() {},
    show() { load(false); },
    config() { load(false); },
  };
}

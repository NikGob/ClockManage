import { call, esc } from '../api.js';
import { icon, CHECK } from '../icons.js';
import { run, snack, ask, hoursLabel, partsPreview, KINDS, kindLabel, menu } from '../ui.js';

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
          <button class="interactive" data-tab="today" role="tab" data-tip="План только на сегодня">${CHECK}Сегодня</button>
          ${KINDS.map((k) => `<button class="interactive" data-tab="${k.id}" role="tab" data-tip="Шаблон: так начинается каждый ${k.label.toLowerCase()} день">${CHECK}${k.label}</button>`).join('')}
        </div>
      </div>
      <div id="lock-note"></div>
      <p class="body-m muted" style="margin-bottom:16px" id="hint"></p>
      <div class="plan-editor" id="rows"></div>
      <div class="hstack" style="margin-top:12px">
        <button class="btn outlined interactive" id="add">${icon('add')}Добавить блок</button>
        <button class="btn text interactive" id="add-seg" data-tip="Обед, сон, прогулка — стоит на своём месте в дне, в учебные часы не входит">${icon('coffee')}Добавить отрезок</button>
      </div>
      <div class="plan-footer">
        <label id="tpl-wrap" data-tip="Следующие такие дни начнутся с этого плана"><input type="checkbox" class="check" id="tpl"> <span id="tpl-l">Сохранить и как шаблон</span></label>
        <span class="grow"></span>
        <button class="btn text interactive" id="reset">Сбросить</button>
        <button class="btn filled interactive" id="save" disabled>${icon('check')}Сохранить план</button>
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
      ? `Шаблон дня «${kindLabel(target)}»: с него начинается каждый такой день. Сегодняшний план он не меняет${target === todayKind && !started ? ' — кроме случая, когда сегодня ещё не начат и план не правился' : ''}.`
      : `Каждый блок идёт частями по <b>${seg}</b> мин с перерывами между ними. Перерывы не съедают учебное время.`;
    $('tpl-wrap').hidden = tpl;
    $('tpl-l').textContent = `Сохранить и как шаблон «${kindLabel(todayKind)}»`;
    $('save').lastChild.textContent = tpl ? 'Сохранить шаблон' : 'Сохранить план';
    $('lock-note').innerHTML = locked && !tpl
      ? `<div class="lock-note">${icon('lock')}<div><div class="title-s">Идёт учебный день</div><div class="body-m">Не начатые блоки можно урезать и удалять, начатый — урезать не меньше уже отработанного. Отработанное время не стирается.</div></div></div>`
      : '';
    $('rows').innerHTML = rows.map((r, i) => {
      if (isSeg(r)) {
        const taken = !tpl && !!(r.progress?.started);
        return `<div class="prow seg${r.fresh ? ' fresh' : ''}" data-i="${i}">
        <input class="name" value="${esc(r.name)}" aria-label="Отрезок ${i + 1}" maxlength="24" ${taken ? 'disabled' : ''}>
        <div class="stepper" role="group" aria-label="Длительность">
          <button class="icon-btn interactive" data-a="minus" aria-label="Меньше на ${SEG_STEP} мин" ${r.minutes <= SEG_STEP || taken ? 'disabled' : ''}>${icon('remove')}</button>
          <div class="val tnum${r.bump ? ' bump' : ''}">${r.minutes} мин<small>${taken ? (r.progress.done ? 'прошёл' : 'идёт') : 'отрезок · не учёба'}</small></div>
          <button class="icon-btn interactive" data-a="plus" aria-label="Больше на ${SEG_STEP} мин" ${r.minutes + SEG_STEP > 240 || taken ? 'disabled' : ''}>${icon('add')}</button>
        </div>
        <div class="tools">
          <button class="icon-btn interactive" data-a="up" aria-label="Выше" ${i === 0 ? 'disabled' : ''}>${icon('up')}</button>
          <button class="icon-btn interactive" data-a="down" aria-label="Ниже" ${i === rows.length - 1 ? 'disabled' : ''}>${icon('down')}</button>
          <button class="icon-btn interactive" data-a="del" aria-label="Удалить" data-tip="${taken ? 'Из плана уйдёт, а в журнале отрезок останется' : ''}">${icon('delete')}</button>
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
        <input class="name" value="${esc(r.name)}" aria-label="Название блока ${i + 1}" maxlength="40" ${o && r.progress?.started && !noise ? 'disabled' : ''}>
        <div class="stepper" role="group" aria-label="Длительность">
          <button class="icon-btn interactive" data-a="minus" aria-label="Меньше на ${STEP} мин" ${r.minutes <= minMinutes ? 'disabled' : ''}>${icon('remove')}</button>
          <div class="val tnum${r.bump ? ' bump' : ''}">${hoursLabel(r.minutes)}<small>${parts.join(' + ')} мин</small></div>
          <button class="icon-btn interactive" data-a="plus" aria-label="Больше на ${STEP} мин" ${r.minutes + STEP > 480 ? 'disabled' : ''}>${icon('add')}</button>
        </div>
        <div class="tools">
          <button class="icon-btn interactive" data-a="up" aria-label="Выше" ${i === 0 ? 'disabled' : ''}>${icon('up')}</button>
          <button class="icon-btn interactive" data-a="down" aria-label="Ниже" ${i === rows.length - 1 ? 'disabled' : ''}>${icon('down')}</button>
          <button class="icon-btn interactive" data-a="del" aria-label="Удалить" ${canDelete ? '' : 'disabled'}>${icon('delete')}</button>
        </div>
        ${r.progress?.started ? `<div class="progress"><div class="linear" style="--v:${Math.min(1, doneMin / r.minutes)}"></div><span class="tnum">${doneMin} из ${r.minutes} мин${r.progress.done ? ' · готово' : ''}</span></div>` : ''}
      </div>`;
    }).join('') || `<p class="body-m muted">${tpl && target === 'off' ? 'В выходной плана нет — и это нормально. Можно добавить что-то лёгкое.' : 'Пусто. Добавь первый блок — например «Математика, 1,5 ч».'}</p>`;
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
    const pick = await menu(e.currentTarget, types.map((t, i) => ({ value: i, label: t.name, sub: `${t.minutes} мин${t.alarm ? ' · будильник' : ''}`, icon: icon(t.alarm ? 'alarm' : 'coffee') })), { title: 'Отрезок в плане', note: 'Запустится сам, когда закроется блок перед ним.' });
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
      if (ok) snack($('tpl').checked ? `План сохранён и стал шаблоном «${kindLabel(todayKind)}»` : 'План на сегодня сохранён');
    } else {
      // Fresh config: the one loaded with this screen may be stale (MCP, another save).
      const next = await call('get_config');
      next.profiles[target].plan = blocks;
      ok = await run(() => call('save_config', { cfg: next }).then(() => true), e.currentTarget);
      if (ok) snack(`Шаблон «${kindLabel(target)}» сохранён`);
    }
    if (ok) {
      dirty = false;
      await load(true);
    }
  });
  $('tabs').addEventListener('click', async (e) => {
    const b = e.target.closest('[data-tab]');
    if (!b || b.dataset.tab === target) return;
    if (dirty && !(await ask('Не сохранено', 'Изменения в этом плане пропадут.', 'Переключиться'))) return;
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

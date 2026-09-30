import { call, esc } from '../api.js';
import { icon, CHECK } from '../icons.js';
import { run, snack, ask, hoursLabel, partsPreview, KINDS, kindLabel } from '../ui.js';

const STEP = 15;

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
      <button class="btn outlined interactive" id="add" style="margin-top:12px">${icon('add')}Добавить блок</button>
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
      : `Каждый блок идёт отрезками по <b>${seg}</b> мин с перерывами между ними. Перерывы не съедают учебное время.`;
    $('tpl-wrap').hidden = tpl;
    $('tpl-l').textContent = `Сохранить и как шаблон «${kindLabel(todayKind)}»`;
    $('save').lastChild.textContent = tpl ? 'Сохранить шаблон' : 'Сохранить план';
    $('lock-note').innerHTML = locked && !tpl
      ? `<div class="lock-note">${icon('lock')}<div><div class="title-s">Идёт учебный день</div><div class="body-m">Блоки можно только добавлять и удлинять. Сократить или удалить — нельзя до конца дня.</div></div></div>`
      : '';
    $('rows').innerHTML = rows.map((r, i) => {
      const o = tpl ? null : r.orig;
      const minMinutes = locked && o ? o.minutes : STEP;
      const canDelete = tpl || (!(locked && o) && !(r.progress?.work_ms > 0));
      const parts = partsPreview(r.minutes, seg);
      const doneMin = r.progress ? Math.floor(r.progress.work_ms / 60000) : 0;
      const cls = r.fresh ? 'prow fresh' : 'prow';
      return `<div class="${cls}" data-i="${i}">
        <input class="name" value="${esc(r.name)}" aria-label="Название блока ${i + 1}" maxlength="40" ${locked && o && r.progress?.started ? 'disabled' : ''}>
        <div class="stepper" role="group" aria-label="Длительность">
          <button class="icon-btn interactive" data-a="minus" aria-label="Меньше на ${STEP} мин" ${r.minutes - STEP < minMinutes ? 'disabled' : ''}>${icon('remove')}</button>
          <div class="val tnum${r.bump ? ' bump' : ''}">${hoursLabel(r.minutes)}<small>${parts.join(' + ')} мин</small></div>
          <button class="icon-btn interactive" data-a="plus" aria-label="Больше на ${STEP} мин" ${r.minutes + STEP > 480 ? 'disabled' : ''}>${icon('add')}</button>
        </div>
        <div class="tools">
          <button class="icon-btn interactive" data-a="up" aria-label="Выше" ${i === 0 || (locked && !tpl) ? 'disabled' : ''}>${icon('up')}</button>
          <button class="icon-btn interactive" data-a="down" aria-label="Ниже" ${i === rows.length - 1 || (locked && !tpl) ? 'disabled' : ''}>${icon('down')}</button>
          <button class="icon-btn interactive" data-a="del" aria-label="Удалить" ${canDelete ? '' : 'disabled'}>${icon('delete')}</button>
        </div>
        ${r.progress?.started ? `<div class="progress"><div class="linear" style="--v:${Math.min(1, doneMin / r.minutes)}"></div><span class="tnum">${doneMin} из ${r.minutes} мин${r.progress.done ? ' · готово' : ''}</span></div>` : ''}
      </div>`;
    }).join('') || `<p class="body-m muted">${tpl && target === 'off' ? 'В выходной плана нет — и это нормально. Можно добавить что-то лёгкое.' : 'Пусто. Добавь первый блок — например «Математика, 1,5 ч».'}</p>`;
    rows.forEach((r) => { r.fresh = false; r.bump = false; });
    markDirty();
  }

  function markDirty() {
    const cur = JSON.stringify(rows.map((r) => ({ name: r.name.trim(), minutes: r.minutes })));
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
      base = st.view.blocks.map((b) => ({ name: b.name, minutes: b.minutes }));
      rows = st.view.blocks.map((b) => ({
        name: b.name, minutes: b.minutes, orig: { name: b.name, minutes: b.minutes },
        progress: { work_ms: b.work_ms, started: b.started, done: b.done },
      }));
    } else {
      base = cfg.profiles[target].plan.map((b) => ({ name: b.name, minutes: b.minutes }));
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
      case 'plus': r.minutes = Math.min(480, r.minutes + STEP); r.bump = true; break;
      case 'minus': r.minutes = Math.max(STEP, r.minutes - STEP); r.bump = true; break;
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
  $('reset').addEventListener('click', () => load(true));
  $('save').addEventListener('click', async (e) => {
    const blocks = rows.map((r) => ({ name: r.name.trim(), minutes: r.minutes }));
    let ok;
    if (target === 'today') {
      ok = await run(() => call('set_plan', { blocks, saveTemplate: $('tpl').checked }).then(() => true), e.currentTarget);
      if (ok) snack($('tpl').checked ? `План сохранён и стал шаблоном «${kindLabel(todayKind)}»` : 'План на сегодня сохранён');
    } else {
      const next = structuredClone(cfg);
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

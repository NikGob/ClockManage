import { call, esc } from '../api.js';
import { icon } from '../icons.js';
import { run, snack, hoursLabel, partsPreview } from '../ui.js';

const STEP = 15;

export function mountPlan(root, ctx) {
  let rows = [];        // editable copy [{name, minutes, orig?:{name, minutes}, progress?}]
  let base = [];        // plan as it was loaded (for dirty check / locked rules)
  let locked = false;
  let started = false;
  let seg = 45;
  let dirty = false;
  let loadedFor = '';

  root.innerHTML = `
    <div class="readable">
      <div id="lock-note"></div>
      <p class="body-m muted" style="margin-bottom:16px">Каждый блок идёт отрезками по <b id="seg">45</b> мин с перерывами между ними. Перерывы не съедают учебное время.</p>
      <div class="plan-editor" id="rows"></div>
      <button class="btn outlined interactive" id="add" style="margin-top:12px">${icon('add')}Добавить блок</button>
      <div class="plan-footer">
        <label><input type="checkbox" class="check" id="tpl"> Сделать шаблоном для следующих дней</label>
        <span class="grow"></span>
        <button class="btn text interactive" id="reset">Сбросить</button>
        <button class="btn filled interactive" id="save" disabled>${icon('check')}Сохранить план</button>
      </div>
    </div>`;
  const $ = (id) => root.querySelector('#' + id);

  function render() {
    $('seg').textContent = seg;
    $('lock-note').innerHTML = locked
      ? `<div class="lock-note">${icon('lock')}<div><div class="title-s">Идёт учебный день</div><div class="body-m">Блоки можно только добавлять и удлинять. Сократить или удалить — нельзя до конца дня.</div></div></div>`
      : '';
    $('rows').innerHTML = rows.map((r, i) => {
      const o = r.orig;
      const minMinutes = locked && o ? o.minutes : STEP;
      const canDelete = !(locked && o) && !(r.progress?.work_ms > 0);
      const parts = partsPreview(r.minutes, seg);
      const doneMin = r.progress ? Math.floor(r.progress.work_ms / 60000) : 0;
      return `<div class="prow" data-i="${i}">
        <input class="name" value="${esc(r.name)}" aria-label="Название блока ${i + 1}" maxlength="40" ${locked && o && r.progress?.started ? 'disabled' : ''}>
        <div class="stepper" role="group" aria-label="Длительность">
          <button class="icon-btn interactive" data-a="minus" aria-label="Меньше на ${STEP} мин" ${r.minutes - STEP < minMinutes ? 'disabled' : ''}>${icon('remove')}</button>
          <div class="val tnum">${hoursLabel(r.minutes)}<small>${parts.join(' + ')} мин</small></div>
          <button class="icon-btn interactive" data-a="plus" aria-label="Больше на ${STEP} мин" ${r.minutes + STEP > 480 ? 'disabled' : ''}>${icon('add')}</button>
        </div>
        <div class="tools">
          <button class="icon-btn interactive" data-a="up" aria-label="Выше" ${i === 0 || locked ? 'disabled' : ''}>${icon('up')}</button>
          <button class="icon-btn interactive" data-a="down" aria-label="Ниже" ${i === rows.length - 1 || locked ? 'disabled' : ''}>${icon('down')}</button>
          <button class="icon-btn interactive" data-a="del" aria-label="Удалить" ${canDelete ? '' : 'disabled'}>${icon('delete')}</button>
        </div>
        ${r.progress?.started ? `<div class="progress"><div class="linear" style="--v:${Math.min(1, doneMin / r.minutes)}"></div><span class="tnum">${doneMin} из ${r.minutes} мин${r.progress.done ? ' · готово' : ''}</span></div>` : ''}
      </div>`;
    }).join('') || '<p class="body-m muted">Пусто. Добавь первый блок — например «Математика, 1,5 ч».</p>';
    markDirty();
  }

  function markDirty() {
    const cur = JSON.stringify(rows.map((r) => ({ name: r.name.trim(), minutes: r.minutes })));
    dirty = cur !== JSON.stringify(base) || $('tpl').checked;
    $('save').disabled = !dirty || rows.some((r) => !r.name.trim());
    $('reset').disabled = !dirty;
  }

  async function load(force = false) {
    const [s, cfg] = await Promise.all([call('get_state'), call('get_config')]);
    const key = s.view.date;
    if (!force && dirty && loadedFor === key) return;
    loadedFor = key;
    seg = cfg.timing.work_segment_min;
    locked = s.view.lock.base && s.view.started && s.view.mode === 'plan' && s.view.lock.reason !== 'single';
    started = s.view.started;
    base = s.view.blocks.map((b) => ({ name: b.name, minutes: b.minutes }));
    rows = s.view.blocks.map((b) => ({
      name: b.name, minutes: b.minutes, orig: { name: b.name, minutes: b.minutes },
      progress: { work_ms: b.work_ms, started: b.started, done: b.done },
    }));
    $('tpl').checked = false;
    render();
  }

  $('rows').addEventListener('click', (e) => {
    const b = e.target.closest('[data-a]');
    if (!b) return;
    const i = Number(b.closest('.prow').dataset.i);
    const r = rows[i];
    switch (b.dataset.a) {
      case 'plus': r.minutes = Math.min(480, r.minutes + STEP); break;
      case 'minus': r.minutes = Math.max(STEP, r.minutes - STEP); break;
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
    rows.push({ name: '', minutes: 90 });
    render();
    root.querySelector('.prow:last-child input.name')?.focus();
  });
  $('reset').addEventListener('click', () => load(true));
  $('save').addEventListener('click', async (e) => {
    const blocks = rows.map((r) => ({ name: r.name.trim(), minutes: r.minutes }));
    const ok = await run(() => call('set_plan', { blocks, saveTemplate: $('tpl').checked }).then(() => true), e.currentTarget);
    if (ok) {
      snack($('tpl').checked ? 'План сохранён и стал шаблоном' : 'План на сегодня сохранён');
      dirty = false;
      await load(true);
    }
  });

  return {
    update() {},
    show() { load(false); },
    config() { load(false); },
  };
}

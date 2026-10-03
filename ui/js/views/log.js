import { call, esc, minutes } from '../api.js';
import { icon } from '../icons.js';
import { doodle, play } from '../doodles.js';
import { run, snack, dateLabel, kindLabel } from '../ui.js';

const fmtMin = (m) => minutes(Math.round(m));
const hmOf = (iso) => (iso && iso.length >= 16 ? iso.slice(11, 16) : '—');

export function mountLog(root, ctx) {
  let days = [];
  let selected = null;

  root.innerHTML = `<div class="log" id="log">
      <section class="surface days" aria-label="Дни"><div id="days" role="listbox" aria-label="Дни"></div></section>
      <section class="surface detail" id="detail" aria-live="polite"></section>
    </div>`;
  const $ = (id) => root.querySelector('#' + id);

  ctx.setActions(`
    <button class="btn text interactive" data-exp="json">${icon('download')}JSON</button>
    <button class="btn text interactive" data-exp="csv">${icon('download')}CSV</button>`);

  function renderDays() {
    if (!days.some((d) => d.started || d.actual_min > 0)) {
      $('log').innerHTML = `<div class="empty" style="grid-column:1/-1">${doodle('empty', 180)}<h2 class="headline-s">Журнал пока пуст</h2>
        <p class="body-l muted">Здесь появится фактическое время по каждому дню: блоки, паузы и аварийные доступы.</p>
        <button class="btn filled interactive" data-go="today">${icon('play')}К таймеру</button></div>`;
      play($('log'));
      return;
    }
    $('days').innerHTML = days.map((d) => {
      const frac = d.planned_min ? Math.min(1, d.actual_min / d.planned_min) : 0;
      const flags = [
        d.kind && d.kind !== 'full' ? kindLabel(d.kind).toLowerCase() : '',
        d.emergencies ? `<span class="flag">аварийно ×${d.emergencies}</span>` : '',
        d.pauses ? `пауз ${d.pauses}` : '',
      ].filter(Boolean).join(' · ');
      return `<button class="day-row interactive" role="option" aria-selected="${d.date === selected}" data-date="${d.date}">
        <span class="d">${esc(dateLabel(d.date))}</span>
        <span class="n tnum">${fmtMin(d.actual_min)} / ${fmtMin(d.planned_min)}</span>
        <span class="linear" style="--v:${frac}"></span>
        <span class="body-s muted">${d.blocks_done}/${d.blocks} блоков${flags ? ' · ' + flags : ''}</span>
      </button>`;
    }).join('');
  }

  async function renderDetail() {
    const el = $('detail');
    if (!el) return;
    if (!selected) { el.innerHTML = '<p class="body-l muted">Выбери день слева.</p>'; return; }
    let st;
    try { st = await call('day_stats', { date: selected }); } catch (e) { el.innerHTML = `<p class="body-l">${esc(e)}</p>`; return; }
    const blocks = st.blocks.map((b) => `<tr><td>${esc(b.name)}${b.done ? ' ' + icon('check', 's18') : ''}${b.note ? `<div class="body-s muted">${esc(b.note)}</div>` : ''}</td>
        <td class="r tnum">${fmtMin(b.planned_min)}</td><td class="r tnum">${fmtMin(b.actual_min)}</td>
        <td class="r tnum">${b.pauses ? `${b.pauses} · ${fmtMin(b.pause_min)}` : '—'}</td></tr>`).join('');
    const pauses = st.pauses.map((p) => `<tr><td class="tnum">${hmOf(p.start)}–${p.end === 'идёт' ? 'идёт' : hmOf(p.end)}</td>
        <td>${esc(p.block || '—')} · ${{ work: 'работа', break: 'перерыв', lunch: 'обед' }[p.during] || p.during}</td>
        <td class="r tnum">${fmtMin(p.minutes)}</td><td class="r tnum">${p.access_min ? fmtMin(p.access_min) + (p.extensions ? ` · +${p.extensions}` : '') : '—'}</td></tr>`).join('');
    const em = st.emergencies.map((e) => `<tr><td class="tnum">${hmOf(e.at)}–${hmOf(e.until)}</td><td class="r tnum">${fmtMin(e.minutes)}${e.ended_early ? ' · закрыт раньше' : ''}</td></tr>`).join('');
    el.innerHTML = `
      <button class="btn text interactive back" data-back>${icon('up')}Все дни</button>
      <h2>${esc(dateLabel(st.date))}</h2>
      ${st.kind ? `<p class="body-m muted" style="margin-top:-12px">${esc(kindLabel(st.kind))} день${st.study_day ? '' : ' · без блокировки'}</p>` : ''}
      <div class="facts">
        <div class="fact"><div class="v tnum">${fmtMin(st.actual_min)}</div><div class="k">ФАКТ ИЗ ${fmtMin(st.planned_min).toUpperCase()}</div></div>
        <div class="fact"><div class="v tnum">${st.pauses_count}</div><div class="k">ПАУЗ · ${fmtMin(st.pauses_min).toUpperCase()}</div></div>
        <div class="fact"><div class="v tnum">${fmtMin(st.pause_access_min)}</div><div class="k">ДОСТУП НА ПАУЗЕ${st.pause_access_extensions ? ` · +${st.pause_access_extensions}` : ''}</div></div>
        <div class="fact ${st.emergency_count ? 'alert' : ''}"><div class="v tnum">${st.emergency_count}</div><div class="k">АВАРИЙНЫХ ДОСТУПОВ</div></div>
      </div>
      ${st.blocks.length ? `<table class="t"><caption class="sr-only">Блоки</caption><thead><tr><th>Блок</th><th class="r">План</th><th class="r">Факт</th><th class="r">Паузы</th></tr></thead><tbody>${blocks}</tbody></table>` : ''}
      ${st.pauses.length ? `<div><h3 class="title-m" style="margin-bottom:8px">Паузы</h3><table class="t"><thead><tr><th>Время</th><th>Где</th><th class="r">Длилась</th><th class="r">Доступ</th></tr></thead><tbody>${pauses}</tbody></table></div>` : ''}
      ${st.emergencies.length ? `<div><h3 class="title-m" style="margin-bottom:8px">Аварийные доступы</h3><table class="t"><thead><tr><th>Время</th><th class="r">Длительность</th></tr></thead><tbody>${em}</tbody></table></div>` : ''}
      ${st.lunch ? `<p class="body-m"><b>Обед:</b> ${esc(st.lunch)}</p>` : ''}
      ${st.single_timer_min ? `<p class="body-m"><b>Одиночный таймер:</b> ${fmtMin(st.single_timer_min)}</p>` : ''}
      <p class="body-s muted">Начат: ${hmOf(st.started_at)} · Закрыт: ${hmOf(st.completed_at)}</p>`;
  }

  async function load() {
    days = await call('list_days');
    if (!selected || !days.some((d) => d.date === selected)) selected = days[0]?.date || null;
    if (!$('days')) {
      root.querySelector('.log')?.remove();
      root.innerHTML = `<div class="log" id="log"><section class="surface days"><div id="days" role="listbox" aria-label="Дни"></div></section><section class="surface detail" id="detail"></section></div>`;
    }
    renderDays();
    renderDetail();
  }

  root.addEventListener('click', async (e) => {
    const row = e.target.closest('[data-date]');
    if (row) {
      selected = row.dataset.date;
      $('log').classList.add('detail-open');
      renderDays();
      renderDetail();
      return;
    }
    if (e.target.closest('[data-back]')) { $('log').classList.remove('detail-open'); return; }
    const go = e.target.closest('[data-go]');
    if (go) ctx.navigate(go.dataset.go);
  });

  ctx.onAction = async (e) => {
    const b = e.target.closest('[data-exp]');
    if (!b) return;
    const path = await run(() => call('export_log', { format: b.dataset.exp }), b);
    if (path) snack(`Сохранено: ${path}`, 6000);
  };

  let lastRefresh = 0;
  return {
    update(s) {
      // Refresh today's numbers every 30 s while the journal is open.
      if (s.view.now - lastRefresh > 30000) { lastRefresh = s.view.now; if (days.length) load(); }
    },
    show() { lastRefresh = Date.now(); load(); },
  };
}

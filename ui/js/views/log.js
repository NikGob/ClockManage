import { call, esc, minutes } from '../api.js';
import { icon, CHECK } from '../icons.js';
import { doodle, play } from '../doodles.js';
import { run, snack, dateLabel, kindLabel } from '../ui.js';

const fmtMin = (m) => minutes(Math.round(m));
const hmOf = (iso) => (iso && iso.length >= 16 ? iso.slice(11, 16) : '—');

export function mountLog(root, ctx) {
  let days = [];
  let selected = null;
  let mode = 'days';
  let weekOf = null; // any date of the shown week; null = this week
  let weekTsv = '';
  const DAYS_HTML = `<div class="log" id="log">
      <section class="surface days" aria-label="Дни"><div id="days" role="listbox" aria-label="Дни"></div></section>
      <section class="surface detail" id="detail" aria-live="polite"></section>
    </div>`;

  root.innerHTML = `<div class="log-tabs"><div class="segmented" role="tablist" id="logtabs">
      <button class="interactive" data-mode="days" role="tab" aria-pressed="true">${CHECK}Дни</button>
      <button class="interactive" data-mode="week" role="tab" aria-pressed="false" data-tip="Часы для журнала по предметам за неделю">${CHECK}Неделя</button>
    </div></div><div id="logbody">${DAYS_HTML}</div>`;
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
        <td class="r tnum">${fmtMin(b.planned_min)}</td><td class="r tnum">${fmtMin(b.actual_min)}</td><td class="r tnum">${String(b.journal_hours ?? 0).replace('.', ',')}</td>
        <td class="r tnum">${b.pauses ? `${b.pauses} · ${fmtMin(b.pause_min)}` : '—'}</td></tr>`).join('');
    const pauses = st.pauses.map((p) => `<tr><td class="tnum">${hmOf(p.start)}–${p.end === 'идёт' ? 'идёт' : hmOf(p.end)}</td>
        <td>${esc(p.block || '—')} · ${{ work: 'работа', break: 'перерыв', lunch: 'обед' }[p.during] || p.during}</td>
        <td class="r tnum">${fmtMin(p.minutes)}</td><td class="r tnum">${p.access_min ? fmtMin(p.access_min) + (p.extensions ? ` · +${p.extensions}` : '') : '—'}</td></tr>`).join('');
    const brk = (st.breaks || []).map((b) => `<tr><td>${esc(b.type)}</td><td class="tnum">${hmOf(b.start)}–${b.end ? hmOf(b.end) : 'идёт'}</td>
        <td class="r tnum">${fmtMin(b.planned_min)}</td><td class="r tnum">${fmtMin(b.actual_min)}</td><td class="r tnum">${b.overrun_min >= 1 ? '+' + fmtMin(b.overrun_min) : '—'}</td></tr>`).join('');
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
      ${st.blocks.length ? `<table class="t"><caption class="sr-only">Блоки</caption><thead><tr><th>Блок</th><th class="r">План</th><th class="r">Факт</th><th class="r" title="Факт, округлённый вниз до 0,25 ч">В журнал</th><th class="r">Паузы</th></tr></thead><tbody>${blocks}</tbody>
        <tfoot><tr><td>Итого в журнал</td><td></td><td></td><td class="r tnum"><b>${String(st.journal_total ?? 0).replace('.', ',')}</b></td><td></td></tr></tfoot></table>` : ''}
      ${st.pauses.length ? `<div><h3 class="title-m" style="margin-bottom:8px">Паузы</h3><table class="t"><thead><tr><th>Время</th><th>Где</th><th class="r">Длилась</th><th class="r">Доступ</th></tr></thead><tbody>${pauses}</tbody></table></div>` : ''}
      ${st.emergencies.length ? `<div><h3 class="title-m" style="margin-bottom:8px">Аварийные доступы</h3><table class="t"><thead><tr><th>Время</th><th class="r">Длительность</th></tr></thead><tbody>${em}</tbody></table></div>` : ''}
      ${brk ? `<div><h3 class="title-m" style="margin-bottom:8px">Отрезки</h3><table class="t"><thead><tr><th>Что</th><th>Время</th><th class="r">План</th><th class="r">Факт</th><th class="r">Сверх</th></tr></thead><tbody>${brk}</tbody></table></div>` : ''}
      ${st.single_timer_min ? `<p class="body-m"><b>Одиночный таймер:</b> ${fmtMin(st.single_timer_min)}</p>` : ''}
      <p class="body-s muted">Начат: ${hmOf(st.started_at)} · Закрыт: ${hmOf(st.completed_at)}</p>`;
  }

  async function load() {
    days = await call('list_days');
    if (!selected || !days.some((d) => d.date === selected)) selected = days[0]?.date || null;
    if (!$('days')) $('logbody').innerHTML = DAYS_HTML;
    renderDays();
    renderDetail();
  }

  const hours = (h) => (h ? String(h).replace('.', ',') : '');
  const shortDate = (iso) => new Date(`${iso}T12:00:00`).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' }).replace('.', '');
  const shift = (iso, d) => {
    const t = new Date(`${iso}T12:00:00`);
    t.setDate(t.getDate() + d);
    return `${t.getFullYear()}-${String(t.getMonth() + 1).padStart(2, '0')}-${String(t.getDate()).padStart(2, '0')}`;
  };

  async function loadWeek() {
    let r;
    try { r = await call('week_stats', { date: weekOf }); } catch (e) { $('logbody').innerHTML = `<p class="body-l">${esc(e)}</p>`; return; }
    const w = r.week;
    weekTsv = r.tsv;
    const today = new Date().toISOString().slice(0, 10);
    const head = w.days.map((d, i) => `<th class="r${d === today ? ' is-today' : ''}">${['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'][i]}<br><span class="muted">${d.slice(8, 10)}</span></th>`).join('');
    const rows = w.subjects.map((s) => `<tr><td>${esc(s.name)}</td>${s.hours.map((h) => `<td class="r tnum">${hours(h)}</td>`).join('')}<td class="r tnum"><b>${hours(s.total)}</b></td></tr>`).join('');
    $('logbody').innerHTML = `<section class="surface week-card">
      <div class="wk-nav">
        <button class="icon-btn interactive" data-week="-7" aria-label="Прошлая неделя">${icon('up')}</button>
        <h2 class="title-l">${esc(shortDate(w.days[0]))} – ${esc(shortDate(w.days[6]))}</h2>
        <button class="icon-btn interactive" data-week="7" aria-label="Следующая неделя">${icon('down')}</button>
        <span class="grow"></span>
        <button class="btn tonal interactive" data-copy-week ${w.subjects.length ? '' : 'disabled'}>${icon('copy')}Скопировать таблицей</button>
      </div>
      ${w.subjects.length ? `<div class="scroll-x"><table class="t wk-t"><thead><tr><th>Предмет</th>${head}<th class="r">Итого</th></tr></thead>
        <tbody>${rows}</tbody>
        <tfoot><tr><td><b>Итого</b></td>${w.day_totals.map((h) => `<td class="r tnum"><b>${hours(h)}</b></td>`).join('')}<td class="r tnum"><b>${hours(w.total)}</b></td></tr></tfoot></table></div>
        <p class="body-s muted">Часы для журнала: каждый блок округлён вниз до 0,25 ч, одинаковые предметы за день сложены. Факт за неделю — ${fmtMin(w.actual_min)}. Обед, сон и другие отрезки не входят.</p>`
        : '<p class="body-l muted">За эту неделю учебных часов нет.</p>'}
    </section>`;
    $('logbody').querySelectorAll('[data-week]').forEach((b) => b.querySelector('svg')?.style.setProperty('transform', 'rotate(-90deg)'));
  }

  function setMode(m) {
    mode = m;
    root.querySelectorAll('[data-mode]').forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.mode === m)));
    if (m === 'week') loadWeek();
    else { $('logbody').innerHTML = DAYS_HTML; load(); }
  }

  root.addEventListener('click', async (e) => {
    const tab = e.target.closest('[data-mode]');
    if (tab) { if (tab.dataset.mode !== mode) setMode(tab.dataset.mode); return; }
    const wk = e.target.closest('[data-week]');
    if (wk) {
      weekOf = shift(weekOf || new Date().toISOString().slice(0, 10), Number(wk.dataset.week));
      loadWeek();
      return;
    }
    if (e.target.closest('[data-copy-week]')) {
      try { await navigator.clipboard.writeText(weekTsv); snack('Скопировано — вставляй в таблицу или журнал'); } catch { snack('Не удалось скопировать'); }
      return;
    }
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
      if (s.view.now - lastRefresh > 30000) {
        lastRefresh = s.view.now;
        if (mode === 'week') loadWeek();
        else if (days.length) load();
      }
    },
    show() { lastRefresh = Date.now(); load(); },
  };
}

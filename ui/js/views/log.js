import { call, esc, minutes } from '../api.js';
import { icon, CHECK } from '../icons.js';
import { doodle, play } from '../doodles.js';
import { run, snack, dateLabel, kindLabel, countUp, WEEKDAYS } from '../ui.js';
import { tr, LOCALE, dec } from '../i18n.js';

const fmtMin = (m) => minutes(Math.round(m));
const hmOf = (iso) => (iso && iso.length >= 16 ? iso.slice(11, 16) : '—');

export function mountLog(root, ctx) {
  let days = [];
  let selected = null;
  let mode = 'days';
  let weekOf = null; // any date of the shown week; null = this week
  let weekTsv = '';
  let weekTotalShown = null;
  const DAYS_HTML = `<div class="log" id="log">
      <section class="surface days" aria-label="${tr('log_days')}"><div id="days" role="listbox" aria-label="${tr('log_days')}"></div></section>
      <section class="surface detail" id="detail" aria-live="polite"></section>
    </div>`;

  root.innerHTML = `<div class="log-tabs"><div class="segmented" role="tablist" id="logtabs">
      <button class="interactive" data-logview="days" role="tab" aria-pressed="true">${CHECK}${tr('log_days')}</button>
      <button class="interactive" data-logview="week" role="tab" aria-pressed="false" data-tip="${tr('log_journal_hours_by_subject_for_the')}">${CHECK}${tr('log_week')}</button>
    </div></div><div id="logbody">${DAYS_HTML}</div>`;
  const $ = (id) => root.querySelector('#' + id);

  ctx.setActions(`
    <button class="btn text interactive" data-exp="json">${icon('download')}JSON</button>
    <button class="btn text interactive" data-exp="csv">${icon('download')}CSV</button>`);

  function renderDays() {
    if (!days.some((d) => d.started || d.actual_min > 0)) {
      $('log').innerHTML = `<div class="empty" style="grid-column:1/-1">${doodle('empty', 180)}<h2 class="headline-s">${tr('log_the_log_is_empty_so_far')}</h2>
        <p class="body-l muted">${tr('log_the_actual_time_of_every_day')}</p>
        <button class="btn filled interactive" data-go="today">${icon('play')}${tr('log_to_the_timer')}</button></div>`;
      play($('log'));
      return;
    }
    $('days').innerHTML = days.map((d) => {
      const frac = d.planned_min ? Math.min(1, d.actual_min / d.planned_min) : 0;
      const flags = [
        d.kind && d.kind !== 'full' ? kindLabel(d.kind).toLowerCase() : '',
        d.emergencies ? `<span class="flag">${tr('log_emergency')}${d.emergencies}</span>` : '',
        d.pauses ? `${tr('log_pauses')} ${d.pauses}` : '',
      ].filter(Boolean).join(' · ');
      return `<button class="day-row interactive" role="option" aria-selected="${d.date === selected}" data-date="${d.date}">
        <span class="d">${esc(dateLabel(d.date))}</span>
        <span class="n tnum">${fmtMin(d.actual_min)} / ${fmtMin(d.planned_min)}</span>
        <span class="linear" style="--v:${frac}"></span>
        <span class="body-s muted">${d.blocks_done}/${d.blocks} ${tr('log_blocks')}${flags ? ' · ' + flags : ''}</span>
      </button>`;
    }).join('');
  }

  async function renderDetail() {
    const el = $('detail');
    if (!el) return;
    if (!selected) { el.innerHTML = `<p class="body-l muted">${tr('log_pick_a_day_on_the_left')}</p>`; return; }
    let st;
    try { st = await call('day_stats', { date: selected }); } catch (e) { el.innerHTML = `<p class="body-l">${esc(e)}</p>`; return; }
    const blocks = st.blocks.map((b) => `<tr><td>${esc(b.name)}${b.done ? ' ' + icon('check', 's18') : ''}${b.note ? `<div class="body-s muted">${esc(b.note)}</div>` : ''}</td>
        <td class="r tnum">${fmtMin(b.planned_min)}</td><td class="r tnum">${fmtMin(b.actual_min)}</td><td class="r tnum">${dec(b.journal_hours ?? 0)}</td>
        <td class="r tnum">${b.pauses ? `${b.pauses} · ${fmtMin(b.pause_min)}` : '—'}</td></tr>`).join('');
    // Access on pause is gone since 0.5.0: its column and fact show up for older days only.
    const pauseAccess = st.pauses.some((p) => p.access_min);
    const pauses = st.pauses.map((p) => `<tr><td class="tnum">${hmOf(p.start)}–${p.end === 'идёт' || p.end === 'running' ? tr('stats_running') : hmOf(p.end)}</td>
        <td>${esc(p.block || '—')} · ${{ work: tr('log_work'), break: tr('log_break'), lunch: tr('log_lunch') }[p.during] || p.during}</td>
        <td class="r tnum">${fmtMin(p.minutes)}</td>${pauseAccess ? `<td class="r tnum">${p.access_min ? fmtMin(p.access_min) + (p.extensions ? ` · +${p.extensions}` : '') : '—'}</td>` : ''}</tr>`).join('');
    const brk = (st.breaks || []).map((b) => `<tr><td>${esc(b.type)}</td><td class="tnum">${hmOf(b.start)}–${b.end ? hmOf(b.end) : tr('stats_running')}</td>
        <td class="r tnum">${fmtMin(b.planned_min)}</td><td class="r tnum">${fmtMin(b.actual_min)}</td><td class="r tnum">${b.overrun_min >= 1 ? '+' + fmtMin(b.overrun_min) : '—'}</td></tr>`).join('');
    const em = st.emergencies.map((e) => `<tr><td class="tnum">${hmOf(e.at)}–${hmOf(e.until)}</td><td class="r tnum">${fmtMin(e.minutes)}${e.ended_early ? tr('log_closed_early') : ''}</td></tr>`).join('');
    el.innerHTML = `
      <button class="btn text interactive back" data-back>${icon('up')}${tr('log_all_days')}</button>
      <h2>${esc(dateLabel(st.date))}</h2>
      ${st.kind ? `<p class="body-m muted" style="margin-top:-12px">${esc(kindLabel(st.kind))} ${tr('log_day')}${st.study_day ? '' : tr('log_no_lock')}</p>` : ''}
      <div class="facts">
        <div class="fact"><div class="v tnum">${fmtMin(st.actual_min)}</div><div class="k">${tr('log_actual_of')} ${fmtMin(st.planned_min).toUpperCase()}</div></div>
        <div class="fact"><div class="v tnum">${st.pauses_count}</div><div class="k">${tr('log_fact_pauses')} ${fmtMin(st.pauses_min).toUpperCase()}</div></div>
        ${st.pause_access_min ? `<div class="fact"><div class="v tnum">${fmtMin(st.pause_access_min)}</div><div class="k">${tr('log_access_on_pause')}${st.pause_access_extensions ? ` · +${st.pause_access_extensions}` : ''}</div></div>` : ''}
        <div class="fact ${st.emergency_count ? 'alert' : ''}"><div class="v tnum">${st.emergency_count}</div><div class="k">${tr('log_emergency_accesses')}</div></div>
      </div>
      ${st.blocks.length ? `<table class="t"><caption class="sr-only">${tr('log_blocks_caption')}</caption><thead><tr><th>${tr('day_block')}</th><th class="r">${tr('main_plan')}</th><th class="r">${tr('log_actual')}</th><th class="r" title="${tr('log_journal_rounding_hint')}">${tr('log_journal')}</th><th class="r">${tr('log_pauses_title')}</th></tr></thead><tbody>${blocks}</tbody>
        <tfoot><tr><td>${tr('log_journal_total')}</td><td></td><td></td><td class="r tnum"><b>${dec(st.journal_total ?? 0)}</b></td><td></td></tr></tfoot></table>` : ''}
      ${st.pauses.length ? `<div><h3 class="title-m" style="margin-bottom:8px">${tr('log_pauses_title')}</h3><table class="t"><thead><tr><th>${tr('log_time')}</th><th>${tr('log_where')}</th><th class="r">${tr('log_lasted')}</th>${pauseAccess ? `<th class="r">${tr('log_access')}</th>` : ''}</tr></thead><tbody>${pauses}</tbody></table></div>` : ''}
      ${st.emergencies.length ? `<div><h3 class="title-m" style="margin-bottom:8px">${tr('log_emergency_accesses_title')}</h3><table class="t"><thead><tr><th>${tr('log_time')}</th><th class="r">${tr('log_duration')}</th></tr></thead><tbody>${em}</tbody></table></div>` : ''}
      ${brk ? `<div><h3 class="title-m" style="margin-bottom:8px">${tr('log_segments')}</h3><table class="t"><thead><tr><th>${tr('log_what')}</th><th>${tr('log_time')}</th><th class="r">${tr('main_plan')}</th><th class="r">${tr('log_actual')}</th><th class="r">${tr('log_over')}</th></tr></thead><tbody>${brk}</tbody></table></div>` : ''}
      ${st.single_timer_min ? `<p class="body-m"><b>${tr('log_single_timer')}</b> ${fmtMin(st.single_timer_min)}</p>` : ''}
      <p class="body-s muted">${tr('log_started')} ${hmOf(st.started_at)} ${tr('log_closed')} ${hmOf(st.completed_at)}</p>`;
  }

  async function load() {
    days = await call('list_days');
    if (!selected || !days.some((d) => d.date === selected)) selected = days[0]?.date || null;
    if (!$('days')) $('logbody').innerHTML = DAYS_HTML;
    renderDays();
    renderDetail();
  }

  const hours = (h) => (h ? dec(h) : '');
  const shortDate = (iso) => new Date(`${iso}T12:00:00`).toLocaleDateString(LOCALE, { day: 'numeric', month: 'short' }).replace('.', '');
  const localIso = (t = new Date()) => `${t.getFullYear()}-${String(t.getMonth() + 1).padStart(2, '0')}-${String(t.getDate()).padStart(2, '0')}`;
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
    const today = localIso();
    const head = w.days.map((d, i) => `<th class="r${d === today ? ' is-today' : ''}">${WEEKDAYS[i]}<br><span class="muted">${d.slice(8, 10)}</span></th>`).join('');
    const rows = w.subjects.map((s) => `<tr><td>${esc(s.name)}</td>${s.hours.map((h) => `<td class="r tnum">${hours(h)}</td>`).join('')}<td class="r tnum"><b>${hours(s.total)}</b></td></tr>`).join('');
    $('logbody').innerHTML = `<section class="surface week-card">
      <div class="wk-nav">
        <button class="icon-btn interactive" data-week="-7" aria-label="${tr('log_previous_week')}">${icon('up')}</button>
        <h2 class="title-l">${esc(shortDate(w.days[0]))} – ${esc(shortDate(w.days[6]))}</h2>
        <button class="icon-btn interactive" data-week="7" aria-label="${tr('log_next_week')}">${icon('down')}</button>
        <span class="grow"></span>
        <button class="btn tonal interactive" data-copy-week ${w.subjects.length ? '' : 'disabled'}>${icon('copy')}${tr('log_copy_as_a_table')}</button>
      </div>
      ${w.subjects.length ? `<div class="scroll-x"><table class="t wk-t"><thead><tr><th>${tr('stats_subject')}</th>${head}<th class="r">${tr('stats_tsv_total_row')}</th></tr></thead>
        <tbody>${rows}</tbody>
        <tfoot><tr><td><b>${tr('stats_tsv_total_row')}</b></td>${w.day_totals.map((h) => `<td class="r tnum"><b>${hours(h)}</b></td>`).join('')}<td class="r tnum"><b id="wk-total">${hours(w.total)}</b></td></tr></tfoot></table></div>
        <p class="body-s muted">${tr('log_journal_hours_every_block_is_rounded')} ${fmtMin(w.actual_min)}${tr('log_lunch_naps_and_other_segments_are')}</p>`
        : `<p class="body-l muted">${tr('log_no_study_hours_this_week')}</p>`}
    </section>`;
    $('logbody').querySelectorAll('[data-week]').forEach((b) => b.querySelector('svg')?.style.setProperty('transform', 'rotate(-90deg)'));
    if (weekTotalShown !== w.total) { countUp($('wk-total'), w.total, hours); weekTotalShown = w.total; }
  }

  function setMode(m) {
    mode = m;
    root.querySelectorAll('[data-logview]').forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.logview === m)));
    if (m === 'week') loadWeek();
    else { $('logbody').innerHTML = DAYS_HTML; load(); }
  }

  root.addEventListener('click', async (e) => {
    const tab = e.target.closest('[data-logview]');
    if (tab) { if (tab.dataset.logview !== mode) setMode(tab.dataset.logview); return; }
    const wk = e.target.closest('[data-week]');
    if (wk) {
      weekOf = shift(weekOf || localIso(), Number(wk.dataset.week));
      loadWeek();
      return;
    }
    if (e.target.closest('[data-copy-week]')) {
      try { await navigator.clipboard.writeText(weekTsv); snack(tr('log_copied_paste_it_into_a_spreadsheet')); } catch { snack(tr('log_couldnt_copy')); }
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
    if (path) snack(`${tr('log_saved')}: ${path}`, 6000);
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

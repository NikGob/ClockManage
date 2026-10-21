import { call, esc } from '../api.js';
import { icon, CHECK } from '../icons.js';
import { run, snack, hm, tsHm } from '../ui.js';
import { tr, LOCALE, dec } from '../i18n.js';

/** Ready diets: [id, name, text], built at render time so the texts are in the current language. */
function presets() {
  return [
    ['brain', tr('meals_max_for_the_brain'), tr('meals_goal_a_clear_steady_mind')],
    ['protein', tr('meals_brain_protein'), tr('meals_goal_plenty_of_protein_for')],
    ['light', tr('meals_brain_light'), tr('meals_goal_lose_weight_slowly_while')],
  ];
}

function kindName(kind) {
  return { breakfast: tr('meals_breakfast'), lunch: tr('meals_lunch'), dinner: tr('meals_dinner') }[kind] || tr('meals_food_picks');
}

/** A price in minor units of its currency (ISO 4217: cents, kopecks; yen have none) for any
 *  currency a delivery service uses: 1390 EUR → 13,90 €, 54900 RUB → 549 ₽, 980 JPY → ¥980. */
function money(minor, currency) {
  const plain = () => dec((minor / 100).toFixed(2)) + (currency ? ` ${currency}` : '');
  if (!currency) return plain();
  for (const currencyDisplay of ['narrowSymbol', 'symbol']) {
    try {
      const digits = new Intl.NumberFormat('en', { style: 'currency', currency }).resolvedOptions().maximumFractionDigits;
      const v = minor / 10 ** digits;
      const shown = Number.isInteger(v) ? 0 : digits;
      return new Intl.NumberFormat(LOCALE, { style: 'currency', currency, currencyDisplay, minimumFractionDigits: shown, maximumFractionDigits: shown }).format(v);
    } catch { /* an old engine without narrowSymbol, or an unknown code */ }
  }
  return plain();
}

const STEP_KM = { min: 0.5, max: 15, step: 0.5 };
const STEP_LEAD = { min: 5, max: 180, step: 5 };

export function mountMeals(root, ctx) {
  let cfg = null;
  let st = null;
  let tz = 0;
  let draft = null;
  let tab = null;
  try { tab = localStorage.getItem('meals-tab'); } catch { /* storage may be unavailable */ }

  root.innerHTML = `<div class="readable" id="meals"></div>`;
  const box = root.querySelector('#meals');
  ctx.setActions(`<button class="btn tonal interactive" data-top="pick">${icon('restaurant')}${tr('meals_pick_now')}</button>`);
  ctx.onAction = async (e) => {
    const b = e.target.closest('[data-top="pick"]');
    if (b) await run(() => call('meals_pick_now'), b);
  };

  const ready = () => cfg.meals.diet.trim() && cfg.meals.lat != null && cfg.meals.lon != null;

  function picksTab() {
    const p = st.picks;
    const status = st.running
      ? `<div class="setting"><span class="badge">${icon('restaurant')}</span><div class="grow"><div class="d">${tr('meals_the_agent_is_picking_food')}</div></div></div>`
      : st.error ? `<div class="setting"><span class="badge">${icon('warning')}</span><div class="grow"><div class="d">${esc(st.error)}</div></div></div>` : '';
    if (!p || !p.venues?.length) {
      const hint = ready() ? tr('meals_no_picks_yet_before_each_meal') : tr('meals_set_the_address_first');
      return `<section class="section">
        <div class="surface">${status}<div class="setting"><div class="grow"><div class="d">${hint}</div></div>
          ${ready() ? '' : `<button class="btn tonal interactive" data-tab="settings">${icon('settings')}${tr('main_settings')}</button>`}</div></div></section>`;
    }
    const service = p.service || st.service;
    const title = p.meal ? tr('meals_kind_at', { kind: kindName(p.kind), time: p.meal }) : kindName(p.kind);
    const card = (v) => `<div class="surface meal-venue">
        <div class="setting">${v.image ? `<img class="venue-pic" src="${esc(v.image)}" alt="" loading="lazy">` : `<span class="badge">${icon(v.store ? 'cart' : 'restaurant')}</span>`}
          <div class="grow"><div class="t">${esc(v.name)}</div><div class="d">${tr('meals_km_min', { km: dec(v.km), eta: esc(v.eta) })}</div></div>
          <button class="btn tonal interactive" data-open="${esc(v.url)}">${icon('open')}${esc(tr('meals_open_in_service', { service }))}</button></div>
        ${v.dishes.map((d) => `<button class="setting dish interactive" data-open="${esc(d.url)}" data-tip="${esc(tr('meals_opens_the_dish_on_service', { service }))}">
          ${d.image ? `<img class="dish-pic" src="${esc(d.image)}" alt="" loading="lazy">` : ''}<div class="grow"><div class="t">${esc(d.name)}</div>${d.why ? `<div class="d">${esc(d.why)}</div>` : ''}</div><span class="tnum">${esc(money(d.price, d.currency))}</span></button>`).join('')}
        ${v.dishes.length > 1 ? `<div class="setting meal-total"><div class="grow d">${tr('meals_total')}</div><span class="tnum">${esc(money(v.dishes.reduce((n, d) => n + d.price, 0), v.dishes[0].currency))}</span></div>` : ''}
      </div>`;
    const food = p.venues.filter((v) => !v.store).map(card).join('');
    const shops = p.venues.filter((v) => v.store).map(card).join('');
    return `<section class="section"><h2>${esc(title)}</h2>
      <p class="section-d">${esc(tr('meals_picked_at_tap_a_dish', { time: tsHm(p.at, tz), service }))}</p>
      ${status ? `<div class="surface">${status}</div>` : ''}
      ${food ? `<h3 class="meal-sub">${icon('restaurant')}${tr('meals_restaurants')}</h3>${food}` : ''}
      ${shops ? `<h3 class="meal-sub">${icon('cart')}${tr('meals_from_stores')}</h3>${shops}` : ''}</section>`;
  }

  /** − value + like the steppers in Settings. */
  function stepper(id, value, { min, max, step }, unit, label) {
    return `<div class="num-step" role="group" aria-label="${esc(label)}">
      <button class="icon-btn interactive" data-step="${id}" data-d="-1" aria-label="${tr('settings_less')}" ${value - step < min ? 'disabled' : ''}>${icon('remove')}</button>
      <span class="v tnum">${dec(value)}<small> ${unit}</small></span>
      <button class="icon-btn interactive" data-step="${id}" data-d="1" aria-label="${tr('settings_more')}" ${value + step > max ? 'disabled' : ''}>${icon('add')}</button>
    </div>`;
  }

  function phoneRows() {
    const pr = st.pairing;
    if (pr) {
      return `<div class="setting col pair-box">
        <div class="t">${tr('meals_scan_with_the_iphone_camera')}</div>
        <div class="qr">${pr.qr}</div>
        <div class="d">${tr('meals_then_share_add_to_home_screen')}</div>
        <div class="hstack"><span class="d">${tr('meals_or_type_the_code_in_the')}</span><span class="pin tnum">${esc(pr.code)}</span></div>
        <div class="d">${tr('meals_the_code_works_until', { time: new Date(pr.until).toLocaleTimeString(LOCALE, { hour: '2-digit', minute: '2-digit' }) })}</div>
        <div class="hstack"><code class="code">${esc(pr.url)}</code><button class="icon-btn interactive" data-copy="${esc(pr.url)}" aria-label="${tr('settings_copied')}">${icon('copy')}</button></div>
      </div>`;
    }
    return `<div class="setting"><span class="badge">${icon('phone')}</span>
        <div class="grow"><div class="t">${st.paired ? tr('meals_iphone_connected') : tr('meals_no_iphone_yet')}</div><div class="d">${tr('meals_picks_come_to_the_iphone_as')}</div></div>
        ${cfg.meals.link ? `<button class="btn text interactive" id="unpair">${tr('settings_unpair')}</button>` : ''}
        <button class="btn tonal interactive" id="pair">${icon('add')}${st.paired ? tr('meals_pair_again') : tr('meals_pair_an_iphone')}</button></div>`;
  }

  function settingsTab() {
    const m = cfg.meals;
    const diet = draft ?? m.diet;
    const where = m.lat != null && m.lon != null ? tr('meals_location_lat_lon', { lat: dec(m.lat.toFixed(4)), lon: dec(m.lon.toFixed(4)) }) : tr('meals_no_address_yet');
    const times = m.times.map((t) => `<span class="time-chip tnum">${hm(t)}<button class="icon-btn interactive" data-untime="${t}" aria-label="${esc(tr('meals_remove_time', { time: hm(t) }))}">${icon('close')}</button></span>`).join('');
    return `
      <section class="section">
        <h2>${tr('meals_diet')}</h2>
        <p class="section-d">${tr('meals_in_your_own_words_rules_banned')}</p>
        <div class="surface">
          <div class="setting col"><div class="hstack wrap"><span class="d">${tr('meals_ready_diets')}</span>${presets().map(([id, name, text]) => `<button class="chip interactive" data-preset="${id}" aria-pressed="${diet === text}">${esc(name)}</button>`).join('')}</div>
            <div class="field"><textarea id="diet" rows="8" spellcheck="false" aria-label="${tr('meals_diet')}">${esc(diet)}</textarea></div>
            ${draft !== null && draft !== m.diet ? `<div class="hstack"><button class="btn tonal interactive" id="diet-save">${icon('check')}${tr('meals_save_the_diet')}</button><button class="btn text interactive" id="diet-undo">${tr('ui_cancel')}</button></div>` : ''}</div>
        </div>
      </section>
      <section class="section">
        <h2>${tr('meals_delivery')}</h2>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">${tr('meals_delivery_service')}</div></div>
            <div class="segmented" role="group">${st.services.map(([id, name]) => `<button class="interactive" data-service="${id}" aria-pressed="${m.service === id}">${CHECK}${esc(name)}</button>`).join('')}</div></div>
          <div class="setting col"><div class="t">${tr('meals_address')}</div>
            <div class="hstack"><div class="field grow"><input id="addr" value="${esc(m.address)}" placeholder="${tr('meals_street_house_city')}" aria-label="${tr('meals_address')}"></div>
              <button class="btn tonal interactive" id="addr-find">${icon('search')}${tr('meals_find')}</button></div>
            <div class="d hstack">${icon('pin')}<span>${esc(where)}</span></div></div>
          <div class="setting"><div class="grow"><div class="t">${tr('meals_not_farther_than')}</div><div class="d">${tr('meals_restaurants_farther_away_are_never_offered')}</div></div>
            ${stepper('max_km', m.max_km, STEP_KM, tr('meals_km'), tr('meals_not_farther_than'))}</div>
        </div>
      </section>
      <section class="section">
        <h2>${tr('meals_meal_times')}</h2>
        <div class="surface">
          <div class="setting col"><div class="hstack wrap">${times || `<span class="d">${tr('meals_no_meal_times_the_agent_picks')}</span>`}</div>
            <div class="hstack"><div class="field"><input id="time-new" type="time" aria-label="${tr('meals_add_a_time')}"></div>
              <button class="btn text interactive" id="time-add">${icon('add')}${tr('meals_add_a_time')}</button></div></div>
          <div class="setting"><div class="grow"><div class="t">${tr('meals_pick_ahead')}</div><div class="d">${tr('meals_how_long_before_a_meal_the')}</div></div>
            ${stepper('lead_min', m.lead_min, STEP_LEAD, tr('overlay_min'), tr('meals_pick_ahead'))}</div>
        </div>
      </section>
      <section class="section">
        <h2>${tr('meals_iphone')}</h2>
        <div class="surface">
          ${phoneRows()}
          <details class="setting col more"><summary class="t interactive">${tr('meals_web_app_address')}</summary>
            <div class="field"><input id="pwa" value="${esc(m.pwa_url)}" spellcheck="false" aria-label="${tr('meals_web_app_address')}"></div></details>
        </div>
      </section>
      <section class="section">
        <h2>${tr('meals_agent')}</h2>
        <p class="section-d">${tr('meals_before_each_meal_clockmanage_asks_this')}</p>
        <div class="surface">
          <div class="setting"><div class="grow"><div class="t">${tr('meals_who_picks')}</div></div>
            <div class="segmented" role="group">${[['claude', tr('meals_claude_code')], ['custom', tr('meals_custom_command')]].map(([v, l]) =>
              `<button class="interactive" data-agent="${v}" aria-pressed="${m.agent === v}">${CHECK}${l}</button>`).join('')}</div></div>
          ${m.agent === 'custom'
            ? `<div class="setting col"><div class="field"><input id="agent" class="mono" value="${esc(m.agent_cmd)}" spellcheck="false" placeholder="codex exec --skip-git-repo-check -" aria-label="${tr('meals_custom_command')}"></div>
                <div class="d">${tr('meals_the_task_comes_on_stdin', { url: '<code>{mcp_url}</code>', config: '<code>{mcp_config}</code>' })}</div></div>`
            : `<div class="setting"><span class="badge">${icon(st.claude ? 'check' : 'warning')}</span><div class="grow"><div class="d">${st.claude
              ? tr('meals_found_it_runs_on_its_own', { path: `<code>${esc(st.claude)}</code>` })
              : tr('meals_claude_code_isnt_found_install_it')}</div></div></div>`}
        </div>
      </section>`;
  }

  function render() {
    if (!cfg || !st) return;
    // Until the address and the diet are set there is nothing to pick: open on Settings.
    const shown = tab || (ready() ? 'picks' : 'settings');
    const open = box.querySelector('details.more')?.open;
    box.innerHTML = `
      <div class="plan-tabs"><div class="segmented" role="tablist">
        ${[['picks', tr('meals_picks')], ['settings', tr('main_settings')]].map(([id, l]) =>
          `<button class="interactive" data-tab="${id}" role="tab" aria-selected="${shown === id}" aria-pressed="${shown === id}">${CHECK}${l}</button>`).join('')}
      </div></div>
      ${shown === 'picks' ? picksTab() : settingsTab()}`;
    if (open) box.querySelector('details.more')?.setAttribute('open', '');
  }

  let queue = Promise.resolve();
  function save(mut, okText) {
    const job = queue.then(async () => {
      const next = structuredClone(cfg);
      mut(next.meals);
      const saved = await run(() => call('save_config', { cfg: next }));
      if (saved) { cfg = saved; if (okText) snack(okText); }
      render();
      return !!saved;
    });
    queue = job.catch(() => {});
    return job;
  }

  box.addEventListener('input', (e) => {
    if (e.target.id !== 'diet') return;
    const was = draft !== null && draft !== cfg.meals.diet;
    draft = e.target.value;
    // The Save button comes and goes with a change; the text field keeps its focus and caret.
    if (was !== (draft !== cfg.meals.diet)) {
      const { selectionStart: a, selectionEnd: b } = e.target;
      render();
      const t = box.querySelector('#diet');
      t.focus();
      t.setSelectionRange(a, b);
    }
  });
  box.addEventListener('change', (e) => {
    const t = e.target;
    if (t.id === 'pwa') save((m) => { m.pwa_url = t.value.trim(); });
    else if (t.id === 'agent') save((m) => { m.agent_cmd = t.value.trim(); });
  });
  box.addEventListener('keydown', (e) => {
    if (e.key !== 'Enter') return;
    if (e.target.id === 'addr') box.querySelector('#addr-find')?.click();
    else if (e.target.id === 'time-new') box.querySelector('#time-add')?.click();
  });
  box.addEventListener('click', async (e) => {
    const b = e.target.closest('button');
    if (!b) return;
    if (b.dataset.tab) {
      tab = b.dataset.tab;
      try { localStorage.setItem('meals-tab', tab); } catch { /* storage may be unavailable */ }
      render();
    } else if (b.dataset.open) await run(() => call('open_url', { url: b.dataset.open }));
    else if (b.dataset.preset) {
      draft = presets().find(([id]) => id === b.dataset.preset)[2];
      render();
    } else if (b.dataset.copy) {
      try { await navigator.clipboard.writeText(b.dataset.copy); snack(tr('settings_copied')); } catch { snack(tr('log_couldnt_copy')); }
    } else if (b.id === 'diet-save') {
      const text = box.querySelector('#diet').value.trim();
      if (await save((m) => { m.diet = text; }, tr('meals_the_diet_was_updated'))) { draft = null; render(); }
    } else if (b.id === 'diet-undo') {
      draft = null;
      render();
    } else if (b.id === 'addr-find') {
      const address = box.querySelector('#addr').value.trim();
      if (!address) return;
      const r = await run(() => call('meals_geocode', { address }), b);
      if (r) save((m) => { m.address = address; m.lat = r.lat; m.lon = r.lon; }, r.label);
    } else if (b.dataset.step) {
      const [key, lim] = b.dataset.step === 'max_km' ? ['max_km', STEP_KM] : ['lead_min', STEP_LEAD];
      const v = Math.min(lim.max, Math.max(lim.min, cfg.meals[key] + Number(b.dataset.d) * lim.step));
      save((m) => { m[key] = v; });
    } else if (b.id === 'time-add') {
      const [h, mm] = (box.querySelector('#time-new').value || '').split(':').map(Number);
      if (!Number.isFinite(h)) { snack(tr('meals_times_look_like_13_00_19_00')); return; }
      const t = h * 60 + (mm || 0);
      save((m) => { m.times = [...new Set([...m.times, t])].sort((x, y) => x - y); });
    } else if (b.dataset.untime) {
      const t = Number(b.dataset.untime);
      save((m) => { m.times = m.times.filter((x) => x !== t); });
    } else if (b.dataset.service) save((m) => { m.service = b.dataset.service; });
    else if (b.dataset.agent) save((m) => { m.agent = b.dataset.agent; });
    else if (b.id === 'pair') { const p = await run(() => call('meals_pair'), b); if (p) await refresh(); }
    else if (b.id === 'unpair') { await run(() => call('meals_unpair'), b); await reload(); }
  });

  async function refresh() {
    st = await call('meals_state');
    render();
  }
  async function reload() {
    cfg = await call('get_config');
    await refresh();
  }
  reload();

  return {
    update(s) { tz = s.meta.tz_offset_min; },
    config: reload,
    meals: refresh,
  };
}

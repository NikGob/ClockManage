import { call, esc } from '../api.js';
import { icon } from '../icons.js';
import { run, snack, hm, tsHm } from '../ui.js';
import { tr, LOCALE, dec } from '../i18n.js';

/** "13:00, 19:00" → [780, 1140]; null when something doesn't parse. */
function parseTimes(text) {
  const out = [];
  for (const part of text.split(/[,;\s]+/).filter(Boolean)) {
    const m = /^(\d{1,2})[:.](\d{2})$/.exec(part);
    if (!m || Number(m[1]) > 23 || Number(m[2]) > 59) return null;
    out.push(Number(m[1]) * 60 + Number(m[2]));
  }
  return out;
}

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

export function mountMeals(root, ctx) {
  let cfg = null;
  let st = null;
  let tz = 0;
  let draft = null;

  root.innerHTML = `<div class="readable" id="meals"></div>`;
  const box = root.querySelector('#meals');
  ctx.setActions(`<button class="btn tonal interactive" data-top="pick">${icon('restaurant')}${tr('meals_pick_now')}</button>`);
  ctx.onAction = async (e) => {
    const b = e.target.closest('[data-top="pick"]');
    if (b) await run(() => call('meals_pick_now'), b);
  };

  const price = (d) => {
    try { return new Intl.NumberFormat(LOCALE, { style: 'currency', currency: d.currency || 'EUR' }).format(d.price / 100); } catch { return dec((d.price / 100).toFixed(2)); }
  };

  function picksSection() {
    const p = st.picks;
    const status = st.running
      ? `<div class="setting"><div class="grow"><div class="d">${tr('meals_the_agent_is_picking_food')}</div></div></div>`
      : st.error ? `<div class="setting"><span class="badge">${icon('warning')}</span><div class="grow"><div class="d">${esc(st.error)}</div></div></div>` : '';
    if (!p || !p.venues?.length) {
      return `<section class="section"><h2>${tr('meals_picks')}</h2>
        <div class="surface">${status}<div class="setting"><div class="grow"><div class="d">${tr('meals_no_picks_yet_before_each_meal')}</div></div></div></div></section>`;
    }
    const title = p.meal ? tr('meals_kind_at', { kind: kindName(p.kind), time: p.meal }) : kindName(p.kind);
    const venues = p.venues.map((v) => `<div class="surface meal-venue">
        <div class="setting"><span class="badge">${icon('restaurant')}</span>
          <div class="grow"><div class="t">${esc(v.name)}</div><div class="d">${tr('meals_km_min', { km: dec(v.km), eta: esc(v.eta) })}</div></div>
          <button class="btn tonal interactive" data-open="${esc(v.url)}">${icon('open')}${tr('meals_open_in_wolt')}</button></div>
        ${v.dishes.map((d) => `<button class="setting dish interactive" data-open="${esc(d.url)}" data-tip="${esc(tr('meals_opens_the_dish_on_wolt'))}">
          <div class="grow"><div class="t">${esc(d.name)}</div>${d.why ? `<div class="d">${esc(d.why)}</div>` : ''}</div><span class="tnum">${esc(price(d))}</span></button>`).join('')}
      </div>`).join('');
    return `<section class="section"><h2>${esc(title)}</h2>
      <p class="section-d">${tr('meals_picked_at_tap_a_dish', { time: tsHm(p.at, tz) })}</p>
      ${status ? `<div class="surface">${status}</div>` : ''}${venues}</section>`;
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

  function render() {
    if (!cfg || !st) return;
    const m = cfg.meals;
    const where = m.lat != null && m.lon != null ? tr('meals_location_lat_lon', { lat: dec(m.lat.toFixed(4)), lon: dec(m.lon.toFixed(4)) }) : tr('meals_no_address_yet');
    box.innerHTML = `
      ${picksSection()}
      <section class="section">
        <h2>${tr('meals_diet')}</h2>
        <p class="section-d">${tr('meals_in_your_own_words_rules_banned')}</p>
        <div class="surface">
          <div class="setting col"><div class="hstack wrap"><span class="d">${tr('meals_ready_diets')}</span>${presets().map(([id, name, text]) => `<button class="chip interactive" data-preset="${id}" aria-pressed="${(draft ?? m.diet) === text}">${esc(name)}</button>`).join('')}</div>
            <div class="field"><textarea id="diet" rows="8" spellcheck="false" aria-label="${tr('meals_diet')}">${esc(draft ?? m.diet)}</textarea></div>
            <div><button class="btn tonal interactive" id="diet-save">${icon('check')}${tr('meals_save_the_diet')}</button></div></div>
        </div>
      </section>
      <section class="section">
        <h2>${tr('meals_delivery')}</h2>
        <div class="surface">
          <div class="setting col"><div class="t">${tr('meals_address')}</div>
            <div class="hstack"><div class="field grow"><input id="addr" value="${esc(m.address)}" placeholder="${tr('meals_street_house_city')}" aria-label="${tr('meals_address')}"></div>
              <button class="btn tonal interactive" id="addr-find">${tr('meals_find')}</button></div>
            <div class="d">${esc(where)}</div></div>
          <div class="setting"><div class="grow"><div class="t">${tr('meals_not_farther_than')}</div><div class="d">${tr('meals_restaurants_farther_away_are_never_offered')}</div></div>
            <div class="field"><input id="maxkm" type="number" min="0.5" max="15" step="0.5" value="${m.max_km}" aria-label="${tr('meals_not_farther_than')}" style="width:96px"></div><span class="d">${tr('meals_km')}</span></div>
          <div class="setting"><div class="grow"><div class="t">${tr('meals_meal_times')}</div><div class="d">${tr('meals_for_example_13_00_19_00')}</div></div>
            <div class="field"><input id="times" value="${esc(m.times.map(hm).join(', '))}" aria-label="${tr('meals_meal_times')}" style="width:160px"></div></div>
          <div class="setting"><div class="grow"><div class="t">${tr('meals_pick_ahead')}</div><div class="d">${tr('meals_how_long_before_a_meal_the')}</div></div>
            <div class="field"><input id="lead" type="number" min="5" max="180" step="5" value="${m.lead_min}" aria-label="${tr('meals_pick_ahead')}" style="width:96px"></div><span class="d">${tr('overlay_min')}</span></div>
        </div>
      </section>
      <section class="section">
        <h2>${tr('meals_iphone')}</h2>
        <div class="surface">
          ${phoneRows()}
          <div class="setting col"><div class="t">${tr('meals_web_app_address')}</div>
            <div class="field"><input id="pwa" value="${esc(m.pwa_url)}" spellcheck="false" aria-label="${tr('meals_web_app_address')}"></div></div>
        </div>
      </section>
      <section class="section">
        <h2>${tr('meals_agent')}</h2>
        <p class="section-d">${tr('meals_before_each_meal_clockmanage_runs_this', { file: '<code>{mcp_config}</code>' })}</p>
        <div class="surface">
          <div class="setting col"><div class="field"><input id="agent" class="mono" value="${esc(m.agent_cmd)}" spellcheck="false" aria-label="${tr('meals_agent')}"></div>
            <div><button class="btn text interactive" id="agent-reset">${icon('refresh')}${tr('meals_back_to_claude_code')}</button></div></div>
        </div>
      </section>`;
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

  box.addEventListener('input', (e) => { if (e.target.id === 'diet') draft = e.target.value; });
  box.addEventListener('change', (e) => {
    const t = e.target;
    if (t.id === 'maxkm') save((m) => { m.max_km = Number(t.value) || m.max_km; });
    else if (t.id === 'lead') save((m) => { m.lead_min = Math.round(Number(t.value)) || m.lead_min; });
    else if (t.id === 'pwa') save((m) => { m.pwa_url = t.value.trim(); });
    else if (t.id === 'agent') save((m) => { m.agent_cmd = t.value.trim(); });
    else if (t.id === 'times') {
      const times = parseTimes(t.value);
      if (times === null) { snack(tr('meals_times_look_like_13_00_19_00')); render(); } else save((m) => { m.times = times; });
    }
  });
  box.addEventListener('click', async (e) => {
    const b = e.target.closest('button');
    if (!b) return;
    if (b.dataset.open) await run(() => call('open_url', { url: b.dataset.open }));
    else if (b.dataset.preset) {
      draft = presets().find(([id]) => id === b.dataset.preset)[2];
      render();
      snack(tr('meals_press_save_the_diet_to_keep'));
    }
    else if (b.dataset.copy) {
      try { await navigator.clipboard.writeText(b.dataset.copy); snack(tr('settings_copied')); } catch { snack(tr('log_couldnt_copy')); }
    } else if (b.id === 'diet-save') {
      const text = box.querySelector('#diet').value.trim();
      if (await save((m) => { m.diet = text; }, tr('meals_the_diet_was_updated'))) draft = null;
    } else if (b.id === 'addr-find') {
      const address = box.querySelector('#addr').value.trim();
      if (!address) return;
      const r = await run(() => call('meals_geocode', { address }), b);
      if (r) save((m) => { m.address = address; m.lat = r.lat; m.lon = r.lon; }, r.label);
    } else if (b.id === 'agent-reset') save((m) => { m.agent_cmd = ''; });
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

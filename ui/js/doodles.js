// Hand-drawn illustrations: ink strokes drawn on with stroke-dashoffset and flat fills.
// While a doodle moves it "boils" like hand-drawn animation: jittered vector frames at 12 fps
// plus a light pixel-crunchy displacement filter. When the motion is over it settles on the
// clean, exact vector frame and stays still (no perpetual jitter). Clicking it wiggles again.

let uid = 0;

function frame(inner, { size = 240, label = '' } = {}) {
  const id = `crunch${++uid}`;
  return `<svg class="doodle" viewBox="0 0 240 240" width="${size}" height="${size}" role="img" aria-label="${label}" data-filter="${id}">
  <defs><filter id="${id}" x="-8%" y="-8%" width="116%" height="116%" color-interpolation-filters="sRGB">
    <feTurbulence type="fractalNoise" baseFrequency="0.045" numOctaves="1" seed="1"/>
    <feDisplacementMap in="SourceGraphic" scale="3.2" xChannelSelector="R" yChannelSelector="G"/>
  </filter></defs>
  <g class="layer">${inner}</g></svg>`;
}

// stroke helper: draw-on path with delay (ms) and duration
const s = (d, delay = 0, dur = 600, cls = '') =>
  `<path class="ink draw ${cls}" pathLength="1" d="${d}" style="--delay:${delay}ms;--d:${dur}ms"/>`;
// "fade" fills only fade in (they stay pinned under their outline); the rest pop.
const fill = (d, cls, delay = 0) =>
  `<path class="${cls}${/\bfade\b/.test(cls) ? '' : ' pop'}" d="${d}" style="--delay:${delay}ms"/>`;

export function alarmClock(size) {
  return frame(`
  <g class="shake">
    ${fill('M60 64 C46 78 46 98 62 108 L98 72 C88 58 72 54 60 64 Z', 'fill-p', 80)}
    ${fill('M180 64 C194 78 194 98 178 108 L142 72 C152 58 168 54 180 64 Z', 'fill-p', 120)}
    ${s('M58 62 C44 76 44 98 60 108 L96 72 C86 58 70 54 58 62 Z', 0, 500)}
    ${s('M182 62 C196 76 196 98 180 108 L144 72 C154 58 170 54 182 62 Z', 60, 500)}
    <g class="hammer">${s('M120 46 L120 30', 380, 200)}${s('M113 24 C113 18 127 18 127 24 C127 31 113 31 113 24 Z', 420, 260)}</g>
    ${fill('M122 62 C164 60 196 94 194 134 C196 176 162 206 120 204 C78 206 46 174 48 132 C46 92 80 60 122 62 Z', 'fill-bg', 200)}
    ${s('M120 60 C162 58 194 92 192 132 C194 174 160 204 118 202 C76 204 44 172 46 130 C44 90 78 58 124 60', 120, 900)}
    ${s('M120 78 L120 88', 700, 150, 'thin')}${s('M176 132 L166 132', 740, 150, 'thin')}
    ${s('M120 186 L120 176', 780, 150, 'thin')}${s('M64 132 L74 132', 820, 150, 'thin')}
    ${s('M120 134 L120 98', 900, 260)}${s('M120 134 L144 148', 960, 240)}
    ${s('M76 190 L60 212', 600, 220)}${s('M164 190 L180 212', 640, 220)}
  </g>
  <g class="waves">
    ${s('M30 116 Q20 132 30 148', 1100, 260, 'thin')}${s('M14 104 Q0 132 14 160', 1180, 300, 'thin')}
    ${s('M210 116 Q220 132 210 148', 1140, 260, 'thin')}${s('M226 104 Q240 132 226 160', 1220, 300, 'thin')}
  </g>`, { size, label: 'Будильник звенит' });
}

export function teaCup(size) {
  return frame(`
    ${fill('M74 110 L80 160 Q82 170 98 170 L142 170 Q158 170 160 160 L166 110 Z', 'fill-p', 200)}
    ${s('M52 180 Q120 200 188 180', 0, 420)}
    ${s('M70 104 L78 160 Q80 172 96 172 L144 172 Q160 172 162 160 L170 104', 120, 700)}
    ${s('M70 104 Q120 116 170 104 Q120 94 70 104 Z', 300, 500, 'thin')}
    ${s('M168 118 Q198 114 194 138 Q190 158 160 152', 520, 420)}
    <g class="steam">
      ${s('M100 88 Q92 74 102 62 Q112 50 104 36', 800, 500, 'thin')}
      ${s('M122 86 Q114 70 124 58 Q134 46 126 32', 900, 500, 'thin')}
      ${s('M144 88 Q136 74 146 62 Q156 50 148 38', 1000, 500, 'thin')}
    </g>`, { size, label: 'Чашка чая' });
}

function burst(delay) {
  let out = '';
  const n = 10;
  for (let i = 0; i < n; i++) {
    const a = (i / n) * Math.PI * 2 + 0.2;
    const r1 = 96 + (i % 2) * 6;
    const r2 = r1 + 16 + (i % 3) * 4;
    const x1 = 120 + Math.cos(a) * r1, y1 = 120 + Math.sin(a) * r1;
    const x2 = 120 + Math.cos(a) * r2, y2 = 120 + Math.sin(a) * r2;
    out += s(`M${x1.toFixed(1)} ${y1.toFixed(1)} L${x2.toFixed(1)} ${y2.toFixed(1)}`, delay + i * 40, 220, 'thin');
  }
  const sparkle = (x, y, r, cls, d) =>
    fill(`M${x} ${y - r} Q${x + r * 0.18} ${y - r * 0.18} ${x + r} ${y} Q${x + r * 0.18} ${y + r * 0.18} ${x} ${y + r} Q${x - r * 0.18} ${y + r * 0.18} ${x - r} ${y} Q${x - r * 0.18} ${y - r * 0.18} ${x} ${y - r} Z`, cls, d);
  out += sparkle(28, 46, 12, 'fill-t', delay + 120) + sparkle(212, 62, 9, 'fill-p', delay + 200)
    + sparkle(204, 206, 13, 'fill-t', delay + 280) + sparkle(32, 196, 8, 'fill-p', delay + 360);
  return out;
}

export function checkStamp(size) {
  return frame(`
    ${fill('M124 44 C168 42 200 76 198 122 C200 166 166 198 122 196 C78 198 44 164 46 120 C44 78 78 44 124 44 Z', 'fill-p', 60)}
    ${s('M120 40 C166 38 200 74 200 120 C202 166 164 200 120 200 C74 202 40 166 40 120 C38 76 76 40 126 42', 0, 800)}
    <path class="ink draw" style="stroke-width:11;--delay:650ms;--d:420ms" pathLength="1" d="M80 122 L108 150 L162 88"/>
    ${burst(900)}`, { size, label: 'Готово' });
}

export function finishFlag(size) {
  return frame(`
    ${fill('M82 48 Q112 34 136 50 Q160 66 186 52 V114 Q160 128 136 112 Q112 96 82 110 Z', 'fill-p', 300)}
    ${s('M78 210 L78 36', 0, 500)}
    ${s('M78 44 Q110 30 134 46 Q158 62 186 48 L186 112 Q158 126 134 110 Q110 94 78 108', 200, 800)}
    ${s('M56 212 Q80 204 104 212', 500, 300, 'thin')}
    ${burst(1000)}`, { size, label: 'День закрыт' });
}

export function padlock(size) {
  return frame(`
    ${fill('M74 116 H166 Q174 116 174 124 V188 Q174 196 166 196 H74 Q66 196 66 188 V124 Q66 116 74 116 Z', 'fill-p', 200)}
    <g class="shackle">${s('M92 112 V84 Q92 54 120 54 Q148 54 148 84 V112', 0, 600)}</g>
    ${s('M72 112 H168 Q176 112 176 120 V188 Q176 196 168 196 H72 Q64 196 64 188 V120 Q64 112 72 112 Z', 200, 800)}
    ${s('M114 146 C114 138 126 138 126 146 C126 154 114 154 114 146 Z', 800, 300, 'thin')}
    ${s('M120 154 L120 170', 950, 200)}`, { size, label: 'Замок' });
}

export function emptyBook(size) {
  return frame(`
    ${fill('M44 72 Q82 58 118 78 V186 Q82 166 44 180 Z', 'fill-s', 200)}
    ${s('M40 70 Q80 54 120 76 Q160 54 200 70 L200 182 Q160 166 120 188 Q80 166 40 182 Z', 0, 1000)}
    ${s('M120 76 L120 188', 600, 400)}
    ${s('M140 96 Q160 88 180 94', 900, 250, 'thin')}${s('M140 116 Q160 108 180 114', 1000, 250, 'thin')}
    ${s('M140 136 Q156 130 170 134', 1100, 250, 'thin')}`, { size, label: 'Пустой журнал' });
}

export function notebook(size) {
  return frame(`
    ${fill('M48 64 Q84 52 118 70 V182 Q84 166 48 178 Z', 'fill-s', 200)}
    ${s('M44 62 Q82 48 120 68 Q158 48 196 62 L196 178 Q158 164 120 184 Q82 164 44 178 Z', 0, 900)}
    ${s('M120 68 L120 184', 500, 300)}
    ${s('M136 92 Q150 84 164 92 Q176 98 184 90', 800, 400, 'thin')}
    ${s('M136 114 Q152 106 168 112', 1000, 300, 'thin')}
    ${s('M60 94 Q80 86 102 92', 700, 300, 'thin')}${s('M60 114 Q76 108 96 112', 850, 300, 'thin')}
    <g class="pencil">
      ${fill('M150 150 L200 100 L214 114 L164 164 Z', 'fill-p', 1100)}
      ${s('M146 170 L150 150 L200 100 L214 114 L164 164 Z', 1000, 600)}
      ${s('M146 170 L156 158', 1300, 200, 'thin')}${s('M192 108 L206 122', 1400, 200, 'thin')}
    </g>`, { size, label: 'Тетрадь и карандаш' });
}

export function bowl(size) {
  return frame(`
    ${fill('M56 124 Q60 188 120 190 Q180 188 184 124 Z', 'fill-p', 200)}
    ${s('M50 122 L190 122 Q186 190 120 192 Q54 190 50 122 Z', 0, 800)}
    ${s('M92 196 L148 196', 500, 250)}
    ${s('M146 116 L196 40', 700, 350)}${s('M160 118 L206 50', 780, 350)}
    <g class="steam">
      ${s('M88 108 Q80 94 90 82 Q100 70 92 56', 900, 500, 'thin')}
      ${s('M112 106 Q104 90 114 78 Q124 66 116 52', 1000, 500, 'thin')}
    </g>`, { size, label: 'Обед' });
}

// The hand is drawn around x=128 and shifted left so the swing is centred in the frame.
const HAND_DX = -14;
const shiftX = (d) => d.replace(/(-?\d+(?:\.\d+)?) (-?\d+(?:\.\d+)?)/g, (m, x, y) => `${+x + HAND_DX} ${y}`);
// Wrist pivot of the wagging hand (keep in sync with .wagging .wag in m3.css).
const WRIST = [128 + HAND_DX, 222];

/** Arc around the wrist at radius r between angles a0..a1 (degrees from vertical, + = right). */
function wristArc(r, a0, a1) {
  const pt = (rr, a) => {
    const t = (a * Math.PI) / 180;
    return [(WRIST[0] + rr * Math.sin(t)).toFixed(1), (WRIST[1] - rr * Math.cos(t)).toFixed(1)];
  };
  const [x0, y0] = pt(r, a0);
  const [x1, y1] = pt(r, a1);
  const [cx, cy] = pt(r / Math.cos(((a1 - a0) / 2) * Math.PI / 180), (a0 + a1) / 2);
  return `M${x0} ${y0} Q${cx} ${cy} ${x1} ${y1}`;
}

export function fingerWag(size) {
  // One closed silhouette (index finger + fist) so fill and outline can never drift apart.
  // Motion arcs sit on circles around the wrist, farther out than any point of the hand, so
  // however the hand swings it never crosses them.
  const hand = shiftX('M98 134 L98 58 Q98 42 110 42 Q122 42 122 58 L122 120 Q126 112 136 113 Q146 114 147 124 '
    + 'Q151 116 160 118 Q169 121 169 131 Q177 128 180 137 Q182 144 180 152 L178 178 '
    + 'Q174 206 140 208 Q106 209 96 188 L90 160 Q88 146 98 134 Z');
  const cuff = shiftX('M110 204 L108 228 L154 228 L152 202');
  const still = (d, delay, cls = '') => `<path class="ink ${cls}" d="${d}" style="--delay:${delay}ms"/>`;
  return frame(`
    <g class="wag">
      ${fill(cuff + ' Z', 'fill-s fade', 260)}
      ${s(cuff, 380, 320)}
      ${fill(hand, 'fill-p fade', 160)}
      ${s(hand, 0, 620)}
      ${s(shiftX('M104 56 Q110 51 116 56'), 520, 160, 'thin')}
      ${s(shiftX('M147 124 L148 144'), 560, 160, 'thin')}${s(shiftX('M169 131 L169 150'), 600, 160, 'thin')}
      ${s(shiftX('M92 164 Q106 176 128 168 Q140 162 136 150'), 620, 260, 'thin')}
    </g>
    <g class="wag-arcs left">${still(wristArc(194, -35, -24), 0, 'thin')}${still(wristArc(208, -32, -26), 0, 'thin')}</g>
    <g class="wag-arcs right">${still(wristArc(194, 12, 23), 0, 'thin')}${still(wristArc(208, 15, 21), 0, 'thin')}</g>`,
  { size, label: 'Не-не-не, грозящий палец' });
}

// Circles are drawn with quadratic curves (no arcs): jittering arc flags would break paths.
const blob = (cx, cy, r) =>
  `M${cx - r} ${cy} Q${cx - r} ${cy - r} ${cx} ${cy - r} Q${cx + r} ${cy - r} ${cx + r} ${cy} Q${cx + r} ${cy + r} ${cx} ${cy + r} Q${cx - r} ${cy + r} ${cx - r} ${cy} Z`;

export function stretch(size) {
  return frame(`
    ${fill('M106 108 Q120 100 134 108 L131 164 L109 164 Z', 'fill-p', 300)}
    ${s(blob(120, 76, 20), 0, 500)}
    ${s('M120 96 L120 164', 300, 300)}
    <g class="arms">
      ${s('M110 112 Q92 84 80 52', 500, 350)}${s('M130 112 Q148 84 160 52', 560, 350)}
      ${s('M80 52 L72 44 M80 52 L84 42', 820, 200, 'thin')}${s('M160 52 L168 44 M160 52 L156 42', 860, 200, 'thin')}
    </g>
    ${s('M120 164 L102 214', 650, 300)}${s('M120 164 L138 214', 700, 300)}
    ${s('M76 218 Q120 226 164 218', 900, 300, 'thin')}
    <g class="reach-lines">${s('M58 70 Q48 56 58 42', 1100, 250, 'thin')}${s('M182 70 Q192 56 182 42', 1150, 250, 'thin')}</g>
  `, { size, label: 'Потянись' });
}

export function waterGlass(size) {
  return frame(`
    ${fill('M92 112 Q106 104 120 112 Q134 120 148 112 L141 198 Q140 204 134 204 L106 204 Q100 204 99 198 Z', 'fill-p', 300)}
    ${s('M84 58 L96 200 Q98 210 108 210 L132 210 Q142 210 144 200 L156 58', 0, 800)}
    ${s('M84 58 Q120 66 156 58', 200, 300, 'thin')}
    ${s('M92 112 Q106 104 120 112 Q134 120 148 112', 600, 400, 'thin')}
    ${s('M138 36 L126 160', 800, 300)}${s('M138 36 L166 26', 950, 200)}
    <g class="bubbles">${s(blob(112, 170, 5), 1100, 200, 'thin')}${s(blob(128, 148, 4), 1200, 200, 'thin')}${s(blob(116, 132, 3), 1300, 200, 'thin')}</g>
  `, { size, label: 'Стакан воды' });
}

export function trophy(size) {
  return frame(`
    ${fill('M88 56 L152 56 L148 104 Q144 132 120 136 Q96 132 92 104 Z', 'fill-p', 300)}
    ${fill('M92 176 L148 176 L148 198 L92 198 Z', 'fill-s', 500)}
    ${s('M86 54 L154 54 L150 104 Q146 134 120 138 Q94 134 90 104 Z', 0, 800)}
    ${s('M88 66 Q62 66 66 88 Q70 104 92 104', 400, 400)}${s('M152 66 Q178 66 174 88 Q170 104 148 104', 450, 400)}
    ${s('M112 138 L110 164 L130 164 L128 138', 700, 300)}
    ${s('M104 164 L136 164 L140 176 L100 176 Z', 800, 300)}
    ${s('M92 176 L148 176 L148 198 L92 198 Z', 900, 400)}
    ${fill('M120 76 Q122 88 132 90 Q122 92 120 104 Q118 92 108 90 Q118 88 120 76 Z', 'fill-t', 1200)}
    ${burst(1300)}`, { size, label: 'Кубок' });
}

const DOODLES = {
  nope: [fingerWag], await: [alarmClock], break: [teaCup, stretch, waterGlass], block: [checkStamp, trophy],
  day: [finishFlag, trophy], access: [padlock], lock: [padlock], empty: [emptyBook], idle: [notebook], lunch: [bowl],
};

/** Hand-drawn scene for a moment; moments with several scenes pick one at random. */
export function doodle(kind, size = 240) {
  const set = DOODLES[kind] || DOODLES.await;
  return set[Math.floor(Math.random() * set.length)](size);
}

const reduced = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

// Deterministic hash noise in [-1, 1].
function noise(a, b, c) {
  let h = (a * 374761393 + b * 668265263 + c * 2147483647) | 0;
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  h ^= h >>> 16;
  return ((h >>> 0) / 4294967295) * 2 - 1;
}

const JITTER = 1.5;
const VARIANTS = 6;
const FPS_MS = 83; // ~12 fps: enough frames to feel alive, still reads as hand-drawn
let pathSeed = 0;

function variants(d, seed) {
  const out = [];
  for (let v = 0; v < VARIANTS; v++) {
    let i = 0;
    out.push(d.replace(/-?\d+(?:\.\d+)?/g, (n) => (Number(n) + noise(seed, i++, v + 1) * JITTER).toFixed(1)));
  }
  return out;
}

function prepare(svg) {
  if (svg._paths) return;
  svg._paths = [...svg.querySelectorAll('.layer path')].map((el) => {
    const d = el.getAttribute('d');
    return { el, d, v: variants(d, ++pathSeed) };
  });
  svg._turb = svg.querySelector('feTurbulence');
  svg._layer = svg.querySelector('.layer');
  // Motion length = the latest draw-on / pop end.
  let end = 0;
  for (const el of svg.querySelectorAll('[style]')) {
    const st = el.getAttribute('style');
    const delay = Number(/--delay:(\d+)/.exec(st)?.[1] || 0);
    const dur = Number(/--d:(\d+)/.exec(st)?.[1] || 520);
    end = Math.max(end, delay + dur);
  }
  svg._drawMs = end;
  svg.addEventListener('pointerdown', () => wiggle(svg));
}

/** Boil for `ms`, then settle on the exact clean frame. */
function boil(svg, ms) {
  clearInterval(svg._boilT);
  clearTimeout(svg._settleT);
  if (reduced()) return;
  let f = 0;
  svg._layer.setAttribute('filter', `url(#${svg.dataset.filter})`);
  const tick = () => {
    f = (f + 1) % VARIANTS;
    for (const p of svg._paths) p.el.setAttribute('d', p.v[f]);
    svg._turb?.setAttribute('seed', String(f + 1));
  };
  tick();
  svg._boilT = setInterval(tick, FPS_MS);
  svg._settleT = setTimeout(() => settle(svg), ms);
}

function settle(svg) {
  clearInterval(svg._boilT);
  for (const p of svg._paths) p.el.setAttribute('d', p.d);
  svg._layer.removeAttribute('filter');
}

/** Short squash + boil when the user pokes a doodle. */
export function wiggle(svg) {
  if (!svg._paths) return;
  svg.classList.remove('poke');
  void svg.getBoundingClientRect();
  svg.classList.add('poke');
  // restart one-shot state animations (steam, pencil, wag…) inside it
  svg.querySelectorAll('.pencil, .steam, .wag, .wag-arcs, .shake, .waves').forEach((g) => {
    g.style.animation = 'none';
    void g.getBoundingClientRect();
    g.style.animation = '';
  });
  if (!svg._still) boil(svg, 700);
}

/**
 * Draw every doodle inside `root` on, boil while it draws (plus `extraMs`), then settle.
 * Returns a function that settles immediately.
 */
export function play(root, { extraMs = 900, jitter = true } = {}) {
  const svgs = [...root.querySelectorAll('svg.doodle')];
  for (const svg of svgs) {
    prepare(svg);
    svg.classList.remove('play');
    void svg.getBoundingClientRect();
    svg.classList.add('play');
    svg._still = !jitter;
    if (jitter) boil(svg, svg._drawMs + extraMs);
  }
  return () => svgs.forEach((svg) => svg._paths && settle(svg));
}

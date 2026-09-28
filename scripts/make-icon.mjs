// Generates the ClockManage app icon: a padlock whose body is a timer dial.
// The dial carries the app's wavy M3 progress ring; the keyhole sits at the dial centre —
// "time unlocks it". Usage: node scripts/make-icon.mjs > src-tauri/icons/icon.svg
const BG = '#0E5A1E';
const FG = '#CFF7C4';
const cx = 512;
const cy = 612;
const R = 188;

const a0 = -Math.PI / 2;
const sweep = Math.PI * 2 * 0.72;
const waves = 12;
const amp = 11;
const steps = 400;

let wave = '';
for (let i = 0; i <= steps; i++) {
  const t = i / steps;
  const a = a0 + sweep * t;
  const taper = Math.min(1, t / 0.06, (1 - t) / 0.06);
  const r = R + amp * taper * Math.sin(t * sweep * waves);
  wave += `${i ? 'L' : 'M'}${(cx + r * Math.cos(a)).toFixed(1)} ${(cy + r * Math.sin(a)).toFixed(1)}`;
}

const gap = 0.16;
const b0 = a0 + sweep + gap;
const b1 = a0 + Math.PI * 2 - gap;
const pt = (a) => `${(cx + R * Math.cos(a)).toFixed(1)} ${(cy + R * Math.sin(a)).toFixed(1)}`;
const track = `M${pt(b0)} A${R} ${R} 0 0 1 ${pt(b1)}`;

process.stdout.write(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <rect x="48" y="48" width="928" height="928" rx="232" fill="${BG}"/>
  <path d="M362 440 V318 C362 232 430 168 512 168 C594 168 662 232 662 318 V440" fill="none" stroke="${FG}" stroke-width="80" stroke-linecap="round"/>
  <circle cx="${cx}" cy="${cy}" r="272" fill="${FG}"/>
  <path d="${track}" fill="none" stroke="${BG}" stroke-opacity=".28" stroke-width="30" stroke-linecap="round"/>
  <path d="${wave}" fill="none" stroke="${BG}" stroke-width="34" stroke-linecap="round" stroke-linejoin="round"/>
  <circle cx="${cx}" cy="${cy - 22}" r="48" fill="${BG}"/>
  <path d="M488 ${cy} L472 ${cy + 92} Q470 ${cy + 106} 484 ${cy + 106} L540 ${cy + 106} Q554 ${cy + 106} 552 ${cy + 92} L536 ${cy} Z" fill="${BG}"/>
</svg>
`);

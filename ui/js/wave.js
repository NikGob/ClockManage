// M3 Expressive wavy circular progress: the active arc is a travelling wave while the timer
// runs and morphs to a flat arc when paused. Track stays flat.

const R = 88;
const WAVES = 22;

export class WaveRing {
  constructor(svg) {
    this.svg = svg;
    svg.setAttribute('viewBox', '0 0 200 200');
    svg.innerHTML = '<path class="track" d=""/><path class="wave" d=""/>';
    this.path = svg.querySelector('.wave');
    this.track = svg.querySelector('.track');
    this.frac = 0;
    this.shown = 0;
    this.amp = 0;
    this.targetAmp = 0;
    this.phase = 0;
    this.raf = 0;
    this.last = 0;
    this.reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  }

  set(frac, running) {
    this.frac = Math.max(0, Math.min(1, frac));
    this.targetAmp = running ? 1.9 : 0;
    this.kick();
  }

  kick() {
    if (!this.raf) this.raf = requestAnimationFrame((t) => this.frame(t));
  }

  frame(t) {
    this.raf = 0;
    const dt = this.last ? Math.min(64, t - this.last) : 16;
    this.last = t;
    // Ease displayed progress and amplitude toward targets.
    const k = this.reduced ? 1 : 1 - Math.pow(0.001, dt / 600);
    this.shown += (this.frac - this.shown) * (Math.abs(this.frac - this.shown) > 0.5 ? 1 : k);
    this.amp += (this.targetAmp - this.amp) * k;
    if (!this.reduced) this.phase += dt * 0.0022;
    this.draw();
    const moving = Math.abs(this.frac - this.shown) > 0.0005 || Math.abs(this.targetAmp - this.amp) > 0.01 || this.targetAmp > 0;
    if (moving && !document.hidden) this.kick();
    else this.last = 0;
  }

  draw() {
    const p = this.shown;
    // Track: flat arc after the indicator with a small gap on both sides (M3 Expressive).
    const gap = p > 0.001 && p < 0.999 ? 0.09 : 0;
    const a0 = p * Math.PI * 2 + gap;
    const a1 = Math.PI * 2 - gap;
    if (a1 - a0 > 0.01) {
      const pt = (a) => `${(100 + R * Math.sin(a)).toFixed(2)} ${(100 - R * Math.cos(a)).toFixed(2)}`;
      const large = a1 - a0 > Math.PI ? 1 : 0;
      this.track.setAttribute('d', a1 - a0 > 6.2 ? `M100 ${100 - R}A${R} ${R} 0 1 1 99.99 ${100 - R}Z` : `M${pt(a0)}A${R} ${R} 0 ${large} 1 ${pt(a1)}`);
    } else this.track.setAttribute('d', '');
    if (p <= 0.001) { this.path.setAttribute('d', ''); return; }
    const end = p * Math.PI * 2;
    const steps = Math.max(8, Math.ceil(end / (Math.PI / 90)));
    const amp = this.reduced ? 0 : this.amp;
    let d = '';
    for (let i = 0; i <= steps; i++) {
      const a = (i / steps) * end;
      // Taper the wave at both ends so the round caps stay on the track.
      const taper = Math.min(1, a / 0.35, (end - a) / 0.35);
      const r = R + amp * taper * Math.sin(a * WAVES - this.phase * 6);
      const x = 100 + r * Math.sin(a);
      const y = 100 - r * Math.cos(a);
      d += `${i ? 'L' : 'M'}${x.toFixed(2)} ${y.toFixed(2)}`;
    }
    this.path.setAttribute('d', d);
  }
}

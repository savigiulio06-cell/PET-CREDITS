// Sprite-based character: PNG poses drawn as <img>, plus an SVG layer for the crater and the
// procedural particles. Entrance "crash-landing", fly-while-dragged, low-credits pose and
// speech bubbles. Stick skins never touch this file (see rig.ts).
import { createBubble } from './bubble';
import { GROUND, HAND_REST, SIGN, STAGE_H, STAGE_W } from './layout';
import type { MotionTracker } from './motion';
import type { SpritePose, SpriteQuote, SpriteSpec, UsageSnapshot } from './types';

const NS = 'http://www.w3.org/2000/svg';
const DEG = 180 / Math.PI;

const FALL_S = 0.45; // fall from above the window
const HOLD_S = 1.4; // crouched landing pose stays this long
const FADE_S = 0.25; // land -> standing crossfade
const REGEN_S = 3; // the crater closes back up
const SHAKE_S = 0.35;
const POP_S = 0.2;
const GREET_WAIT_MS = 8000;

const DIRT = ['#7a5634', '#8a6440', '#6b4a2e', '#4a3220', '#b8956a', '#d8c29a'];
const DUST = '#eadcc0';
/** Where the fist is in the fly PNG, as a fraction of its height (x is the leading edge). */
const FIST_Y = 0.28;

type Mode = 'off' | 'entrance' | 'stand' | 'fly';

const clamp = (v: number, a: number, b: number): number => Math.min(b, Math.max(a, v));
const lerp = (a: number, b: number, w: number): number => a + (b - a) * w;
const approach = (cur: number, target: number, rate: number, dt: number): number =>
  cur + (target - cur) * (1 - Math.exp(-rate * dt));
const rnd = (a: number, b: number): number => a + Math.random() * (b - a);
const ease = (u: number): number => u * u * (3 - 2 * u);
const f1 = (n: number): string => n.toFixed(1);

function svgEl<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string> = {}): SVGElementTagNameMap[K] {
  const el = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  return el;
}

function box(className: string): HTMLDivElement {
  const d = document.createElement('div');
  d.className = className;
  return d;
}

function layerSvg(className: string): SVGSVGElement {
  const s = svgEl('svg', { viewBox: `0 0 ${STAGE_W} ${STAGE_H}`, 'aria-hidden': 'true' });
  s.setAttribute('class', className);
  return s;
}

interface Particle {
  el: SVGElement;
  x: number;
  y: number;
  vx: number;
  vy: number;
  rot: number;
  vr: number;
  age: number;
  life: number;
}

interface Puff {
  el: SVGCircleElement;
  x: number;
  y: number;
  drift: number;
  r0: number;
  r1: number;
  age: number;
  life: number;
  alpha: number;
}

// crater ellipses, outermost first: rim, slope, hole, deep
const CRATER = [
  { fill: '#d8c29a', rx: 80, ry: 16, dy: 0 },
  { fill: '#b8956a', rx: 64, ry: 12.5, dy: 1 },
  { fill: '#6b4a2e', rx: 46, ry: 9, dy: 2 },
  { fill: '#4a3220', rx: 26, ry: 5, dy: 3 },
] as const;
const CRACKS = 7;
const SPECKS = 8;

/**
 * Low = session >= threshold or weekly >= threshold; with hysteresis it only clears once both
 * are below threshold - 2. Based on the same data the sign shows: no data / stale / login issue = not low.
 */
function isLow(snapshot: UsageSnapshot | null, th: SpriteSpec['lowThresholds'], wasLow: boolean): boolean {
  if (!snapshot) return false;
  const hasData = snapshot.windows.length > 0 || snapshot.session !== null;
  const loginIssue = snapshot.issue === 'auth_expired' || snapshot.issue === 'login_missing';
  if (!hasData || snapshot.stale || loginIssue) return false;
  const pct = (kind: string): number | null => snapshot.windows.find((w) => w.kind === kind)?.used_pct ?? null;
  const session = pct('session');
  const weekly = pct('weekly');
  const gap = wasLow ? 2 : 0;
  return (session !== null && session >= th.session - gap) || (weekly !== null && weekly >= th.weekly - gap);
}

export class SpriteCharacter {
  private readonly signEl: HTMLElement;
  private readonly motion: MotionTracker;
  private readonly layer = box('sprite-layer');
  private readonly world = box('sprite-world');
  private readonly craterSvg = layerSvg('sprite-fx');
  private readonly fxSvg = layerSvg('sprite-fx');
  private readonly imgs: [HTMLImageElement, HTMLImageElement];
  private front = 0;
  private readonly bubble = createBubble();

  private readonly craterRings: SVGEllipseElement[] = [];
  private readonly craterCracks: SVGPathElement[] = [];
  private readonly craterSpecks: SVGCircleElement[] = [];
  private readonly craterCrackG = svgEl('g', { fill: 'none', stroke: '#8a6440', 'stroke-width': '1.6', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' });
  private readonly craterG = svgEl('g');
  private crater = { active: false, age: 0, regen: -1, cx: 0, cy: 0, specks: [] as number[] };

  private spec: SpriteSpec | null = null;
  private mode: Mode = 'off';
  private reduced = false;
  private raf = 0;
  private last = 0;
  private clock = 0;

  // entrance
  private en = { t: 0, landed: false, cx: 0, cy: 0, big: 0, rot: 90 };
  private settled = true;
  private streaks: SVGLineElement[] = [];
  private popT = -1;
  private shakeT = -1;
  private landT = -1;
  private sx = 1;
  private sy = 1;
  private particles: Particle[] = [];
  private puffs: Puff[] = [];

  // flight
  private vx = 0;
  private vy = 0;
  private moving = false;
  private faceDir: 1 | -1 = 1;
  private fx = 1;
  private tilt = 0;

  // data / bubbles
  private snapshot: UsageSnapshot | null = null;
  private known = false;
  private low = false;
  private lowPending = false;
  private ready = false;
  private greeted = false;
  private greetTimer = 0;

  constructor(stage: HTMLElement, sign: HTMLElement, motion: MotionTracker) {
    this.signEl = sign;
    this.motion = motion;
    this.layer.setAttribute('aria-hidden', 'true');
    this.layer.style.display = 'none';

    for (const c of CRATER) {
      const e = svgEl('ellipse', { fill: c.fill });
      this.craterRings.push(e);
      this.craterG.append(e);
    }
    for (let i = 0; i < CRACKS; i++) {
      const p = svgEl('path', { pathLength: '1', 'stroke-dasharray': '1 1', 'stroke-dashoffset': '1' });
      this.craterCracks.push(p);
      this.craterCrackG.append(p);
    }
    this.craterG.append(this.craterCrackG);
    for (let i = 0; i < SPECKS; i++) {
      const c = svgEl('circle', { r: '2.2', fill: DIRT[i % 4] ?? '#6b4a2e', opacity: '0' });
      this.craterSpecks.push(c);
      this.craterG.append(c);
    }
    this.craterSvg.append(this.craterG);

    const mk = (): HTMLImageElement => {
      const img = document.createElement('img');
      img.alt = '';
      img.draggable = false;
      img.className = 'sprite-img';
      img.style.opacity = '0';
      return img;
    };
    this.imgs = [mk(), mk()];
    this.world.append(this.craterSvg, this.imgs[0], this.imgs[1], this.fxSvg);
    this.layer.append(this.world);
    stage.prepend(this.layer);
    stage.append(this.bubble.root);

    document.addEventListener('visibilitychange', () => {
      if (document.hidden) {
        this.stopLoop();
      } else if (this.mode !== 'off') {
        this.ensureLoop();
      }
    });
  }

  /* ------------------------------------------------------------ API */

  /** Starts this skin (entrance included, unless reduced motion). Safe to call again to restart. */
  activate(spec: SpriteSpec): void {
    this.teardown();
    this.spec = spec;
    // every activation (start or skin switch) counts as "opening" him: greeting / low quote play again
    this.greeted = false;
    this.low = false;
    this.lowPending = false;
    this.layer.style.display = '';
    this.signEl.classList.add('compact', 'sprite-hidden');
    for (const pose of Object.values(spec.poses)) {
      const pre = new Image();
      pre.src = pose.url; // warm the cache so poses swap without a blank frame
    }
    this.computeLow();
    this.layoutSign();
    this.ready = false;
    if (this.reduced || spec.entrance !== 'crash-landing') this.startStand();
    else this.startEntrance();
  }

  /** Stops everything and hands the sign back to the stick figure. */
  deactivate(): void {
    this.teardown();
    this.spec = null;
    this.layer.style.display = 'none';
    const s = this.signEl;
    s.classList.remove('compact', 'sprite-hidden');
    for (const p of ['left', 'top', 'width', 'height']) s.style.removeProperty(p);
    s.style.removeProperty('--sign-compact-bg');
    s.style.removeProperty('--sign-fade');
    s.style.transform = '';
    s.style.transformOrigin = `${HAND_REST.x - SIGN.x}px 0px`;
  }

  setReduced(reduced: boolean): void {
    this.reduced = reduced;
    if (this.mode === 'off' || !this.spec) return;
    if (reduced) {
      this.stopLoop();
      this.clearFx();
      this.signEl.style.transform = '';
      this.moving = false;
      this.vx = this.vy = 0;
      this.startStand();
    }
  }

  /** Latest usage (null = no snapshot yet). Drives the low pose and the quotes. */
  setSnapshot(snapshot: UsageSnapshot | null): void {
    this.snapshot = snapshot;
    if (this.mode === 'off' || !this.spec) return;
    const before = this.low;
    this.computeLow();
    if (this.low !== before) this.refreshPose();
    this.flush();
  }

  /** A window-move event arrived: make sure the loop is looking at the drag. */
  wake(): void {
    if (this.mode === 'stand' && !this.reduced) this.ensureLoop();
  }

  /* --------------------------------------------------------- state */

  private computeLow(): void {
    const th = this.spec?.lowThresholds ?? { session: 90, weekly: 70 };
    const low = isLow(this.snapshot, th, this.low);
    if (low && !this.low) this.lowPending = true;
    if (!low) this.lowPending = false;
    this.low = low;
    this.known = this.snapshot !== null;
  }

  private standPose(): SpritePose {
    const poses = (this.spec as SpriteSpec).poses;
    return this.low && poses.low ? poses.low : poses.idle;
  }

  private teardown(): void {
    this.stopLoop();
    this.clearFx();
    if (this.greetTimer) window.clearTimeout(this.greetTimer);
    this.greetTimer = 0;
    this.bubble.hide();
    for (const img of this.imgs) {
      img.style.transition = 'none';
      img.style.opacity = '0';
      img.style.transform = '';
    }
    this.mode = 'off';
    this.vx = this.vy = 0;
    this.moving = false;
    this.tilt = 0;
    this.sx = this.sy = 1;
  }

  /** Removes particles, streaks, crater and any pending one-shot effect. */
  private clearFx(): void {
    for (const p of this.particles) p.el.remove();
    for (const p of this.puffs) p.el.remove();
    for (const s of this.streaks) s.remove();
    this.particles = [];
    this.puffs = [];
    this.streaks = [];
    this.crater.active = false;
    this.craterSvg.style.display = 'none';
    this.world.style.transform = '';
    this.popT = this.shakeT = this.landT = -1;
    this.sx = this.sy = 1;
  }

  private startStand(): void {
    this.mode = 'stand';
    this.settled = true;
    this.show(this.standPose(), this.spec!.anchorX, GROUND, 0);
    this.layoutSign();
    this.setSignVisible(true, 0);
    this.finishEntrance();
  }

  private refreshPose(): void {
    if (this.mode !== 'stand' || !this.settled || !this.spec) return;
    this.show(this.standPose(), this.spec.anchorX, GROUND, this.reduced ? 0 : FADE_S);
    this.layoutSign();
  }

  /* -------------------------------------------------------- images */

  /** Anchor x shifted just enough to keep the whole pose inside the stage (any pose). */
  private clampX(p: SpritePose, ax: number): number {
    const margin = this.spec && p === this.spec.poses.fly ? 10 : 4;
    const w = p.w * p.scale;
    if (w > STAGE_W - 2 * margin) return ax;
    const left = ax - p.anchor[0] * p.scale;
    if (left < margin) return ax + (margin - left);
    const over = left + w - (STAGE_W - margin);
    return over > 0 ? ax - over : ax;
  }

  private place(img: HTMLImageElement, p: SpritePose, x: number, ay: number, clamp = true): void {
    const ax = clamp ? this.clampX(p, x) : x;
    const s = p.scale;
    img.style.width = `${f1(p.w * s)}px`;
    img.style.height = `${f1(p.h * s)}px`;
    img.style.left = `${f1(ax - p.anchor[0] * s)}px`;
    img.style.top = `${f1(ay - p.anchor[1] * s)}px`;
    img.style.transformOrigin = `${f1(p.anchor[0] * s)}px ${f1(p.anchor[1] * s)}px`;
    if (img.getAttribute('src') !== p.url) img.setAttribute('src', p.url);
  }

  /** Puts `p` (anchored at ax, ay) on the hidden image and crossfades to it. */
  private show(p: SpritePose, ax: number, ay: number, fade: number, clamp = true): void {
    const cur = this.imgs[this.front];
    const next = this.imgs[1 - this.front];
    if (!cur || !next) return;
    this.place(next, p, ax, ay, clamp);
    const tr = fade > 0 ? `opacity ${fade}s linear` : 'none';
    cur.style.transition = tr;
    next.style.transition = tr;
    next.style.transform = '';
    next.style.opacity = '1';
    cur.style.opacity = '0';
    this.front = 1 - this.front;
  }

  /* ----------------------------------------------------------- sign */

  private layoutSign(): void {
    if (!this.spec) return;
    const p = this.standPose();
    if (!p.sign) return;
    const s = p.scale;
    const axp = this.clampX(p, this.spec.anchorX);
    const left = axp - p.anchor[0] * s;
    const top = GROUND - p.anchor[1] * s;
    const st = this.signEl.style;
    // squash/bounce scales the sign about the same ground point as the sprite
    st.transformOrigin = `${f1(axp - (left + p.sign.x * s))}px ${f1(GROUND - (top + p.sign.y * s))}px`;
    st.left = `${f1(left + p.sign.x * s)}px`;
    st.top = `${f1(top + p.sign.y * s)}px`;
    st.width = `${f1(p.sign.w * s)}px`;
    st.height = `${f1(p.sign.h * s)}px`;
    st.setProperty('--sign-compact-bg', p.sign.bg);
  }

  /** `fade` (s) matches the pose crossfade so the live sign and the figure appear together. */
  private setSignVisible(v: boolean, fade = FADE_S): void {
    this.signEl.style.setProperty('--sign-fade', `${fade}s`);
    this.signEl.classList.toggle('sprite-hidden', !v);
  }

  /* ----------------------------------------------------- entrance */

  private impactPoint(): [number, number] {
    const spec = this.spec as SpriteSpec;
    const land = spec.poses.land;
    if (!land) return [spec.anchorX, GROUND];
    const s = land.scale;
    const ox = this.clampX(land, spec.anchorX) - land.anchor[0] * s;
    const oy = GROUND - land.anchor[1] * s;
    const imp = land.impact ?? land.anchor;
    return [ox + imp[0] * s, oy + imp[1] * s];
  }

  private startEntrance(): void {
    const spec = this.spec as SpriteSpec;
    const fly = spec.poses.fly;
    this.mode = 'entrance';
    this.settled = false;
    this.setSignVisible(false);
    const [ix, iy] = this.impactPoint();
    // Rotate the fly sprite so the leading fist points down; find the body centre that puts the fist on the impact point.
    const right = fly.faces === 'right';
    const rot = right ? 90 : -90;
    const s = fly.scale;
    const tx = ((right ? fly.w : 0) - fly.anchor[0]) * s;
    const ty = (FIST_Y * fly.h - fly.anchor[1]) * s;
    const rx = right ? -ty : ty;
    const ry = right ? tx : -tx;
    const cx = ix - rx;
    const cy = iy - ry;
    this.en = { t: 0, landed: false, cx, cy, big: iy + 6, rot };
    this.show(fly, cx, cy, 0, false);
    this.makeStreaks();
    this.applyTransform();
    this.ensureLoop();
  }

  private makeStreaks(): void {
    for (const off of [-24, 3, 27]) {
      const halo = svgEl('line', { stroke: 'rgb(0 0 0 / 0.2)', 'stroke-width': '5', 'stroke-linecap': 'round' });
      const core = svgEl('line', { stroke: '#fff', 'stroke-width': '2.4', 'stroke-linecap': 'round', opacity: '0.9' });
      halo.dataset.off = String(off);
      core.dataset.off = String(off);
      this.fxSvg.append(halo, core);
      this.streaks.push(halo, core);
    }
  }

  private updateStreaks(cx: number, cy: number): void {
    const fly = (this.spec as SpriteSpec).poses.fly;
    const tail = cy - (fly.w * fly.scale) / 2;
    this.streaks.forEach((l, i) => {
      const off = Number(l.dataset.off ?? 0);
      const len = 55 + ((i >> 1) * 23) % 45;
      const y1 = tail - 4 - ((i >> 1) % 2) * 14;
      l.setAttribute('x1', f1(cx + off));
      l.setAttribute('x2', f1(cx + off));
      l.setAttribute('y1', f1(y1));
      l.setAttribute('y2', f1(y1 - len));
    });
  }

  private updateEntrance(dt: number): void {
    const en = this.en;
    en.t += dt;
    if (!en.landed) {
      const u = clamp(en.t / FALL_S, 0, 1);
      this.updateStreaks(en.cx, en.cy - en.big * (1 - u * u));
      if (en.t >= FALL_S) this.impact();
    } else if (!this.settled && en.t - FALL_S >= HOLD_S) {
      this.settle();
    }
  }

  private impact(): void {
    const spec = this.spec as SpriteSpec;
    this.en.landed = true;
    for (const s of this.streaks) s.remove();
    this.streaks = [];
    const [ix, iy] = this.impactPoint();
    this.show(spec.poses.land ?? this.standPose(), spec.anchorX, GROUND, 0);
    this.popT = 0;
    this.shakeT = 0;
    this.spawnDirt(ix, iy);
    this.spawnPuffs(ix, iy, 5, 1, 0.9);
    this.startCrater(ix, iy);
  }

  private settle(): void {
    const spec = this.spec as SpriteSpec;
    this.settled = true;
    this.mode = 'stand';
    this.show(this.standPose(), spec.anchorX, GROUND, FADE_S);
    this.layoutSign();
    this.setSignVisible(true, FADE_S);
    this.crater.regen = 0;
    this.finishEntrance();
  }

  /** Entrance over (or skipped): quotes may now appear. */
  private finishEntrance(): void {
    this.ready = true;
    this.flush();
    if (!this.greeted && !this.known && this.greetTimer === 0) {
      // first snapshot not here yet: wait for it a while, then give up on the greeting
      this.greetTimer = window.setTimeout(() => {
        this.greetTimer = 0;
        this.greeted = true;
      }, GREET_WAIT_MS);
    }
  }

  private say(q: SpriteQuote | undefined): void {
    if (q) this.bubble.show(q.text, q.lang);
  }

  private flush(): void {
    if (!this.ready || !this.spec || this.mode === 'fly') return;
    if (this.lowPending) {
      this.lowPending = false;
      this.greeted = true; // the low quote replaces the greeting
      this.clearGreetTimer();
      this.say(this.spec.quotes.low);
    } else if (!this.greeted && this.known && !this.low) {
      this.greeted = true;
      this.clearGreetTimer();
      this.say(this.spec.quotes.greeting);
    }
  }

  private clearGreetTimer(): void {
    if (this.greetTimer) window.clearTimeout(this.greetTimer);
    this.greetTimer = 0;
  }

  /* ------------------------------------------------------- effects */

  private spawnDirt(ix: number, iy: number): void {
    const n = 25 + Math.floor(Math.random() * 11);
    for (let i = 0; i < n; i++) {
      const a = -Math.PI * rnd(0.08, 0.92);
      const speed = rnd(140, 430);
      const r = rnd(2, 6.5);
      const fill = DIRT[Math.floor(Math.random() * DIRT.length)] ?? '#7a5634';
      let el: SVGElement;
      if (Math.random() < 0.3) {
        el = svgEl('circle', { r: f1(r * 0.8), fill });
      } else {
        const k = 4 + Math.floor(Math.random() * 3);
        const pts: string[] = [];
        for (let j = 0; j < k; j++) {
          const ang = (j / k) * Math.PI * 2 + rnd(-0.3, 0.3);
          const rr = r * rnd(0.6, 1.25);
          pts.push(`${f1(Math.cos(ang) * rr)},${f1(Math.sin(ang) * rr)}`);
        }
        el = svgEl('polygon', { points: pts.join(' '), fill });
      }
      this.fxSvg.append(el);
      const p: Particle = {
        el,
        x: ix + rnd(-10, 10),
        y: iy - 4,
        vx: Math.cos(a) * speed,
        vy: Math.sin(a) * speed * 1.15,
        rot: rnd(0, 360),
        vr: rnd(-700, 700),
        age: 0,
        life: rnd(0.7, 1.2),
      };
      this.particles.push(p);
      this.drawParticle(p);
    }
  }

  private drawParticle(p: Particle): void {
    p.el.setAttribute('transform', `translate(${f1(p.x)} ${f1(p.y)}) rotate(${f1(p.rot)})`);
    const fade = p.age < p.life * 0.6 ? 1 : 1 - (p.age - p.life * 0.6) / (p.life * 0.4);
    p.el.setAttribute('opacity', f1(clamp(fade, 0, 1)));
  }

  private spawnPuffs(x: number, y: number, count: number, size: number, life: number): void {
    for (let i = 0; i < count; i++) {
      const el = svgEl('circle', { fill: DUST, opacity: '0' });
      this.fxSvg.append(el);
      const side = count > 1 ? (i / (count - 1)) * 2 - 1 : 0;
      this.puffs.push({
        el,
        x: x + side * 34 * size + rnd(-6, 6),
        y: y - 4,
        drift: side * 22 * size,
        r0: 6 * size,
        r1: rnd(24, 36) * size,
        age: 0,
        life: life * rnd(0.85, 1.15),
        alpha: 0.75,
      });
    }
  }

  private startCrater(cx: number, cy: number): void {
    const c = this.crater;
    c.active = true;
    c.age = 0;
    c.regen = -1;
    c.cx = cx;
    c.cy = cy;
    c.specks = Array.from({ length: SPECKS }, () => rnd(-0.6, 0.6));
    this.craterCracks.forEach((p, i) => {
      const a = (i / CRACKS) * Math.PI * 2 + rnd(-0.25, 0.25);
      const dx = Math.cos(a);
      const dy = Math.sin(a) * 0.3; // perspective: flattened like the crater
      const r0 = rnd(0.55, 0.75);
      const l1 = rnd(10, 20);
      const l2 = rnd(8, 18);
      const bend = rnd(-0.5, 0.5);
      const x0 = cx + dx * CRATER[0].rx * r0;
      const y0 = cy + dy * CRATER[0].rx * r0;
      const x1 = x0 + dx * l1;
      const y1 = y0 + dy * l1;
      const bx = Math.cos(a + bend);
      const by = Math.sin(a + bend) * 0.3;
      p.setAttribute('d', `M${f1(x0)} ${f1(y0)} L${f1(x1)} ${f1(y1)} L${f1(x1 + bx * l2)} ${f1(y1 + by * l2)}`);
    });
    this.drawCrater();
  }

  private drawCrater(): void {
    const c = this.crater;
    if (!c.active) {
      this.craterSvg.style.display = 'none';
      return;
    }
    this.craterSvg.style.display = '';
    let s: number;
    let flat = 1;
    let alpha = 1;
    let crack: number;
    if (c.regen < 0) {
      s = ease(clamp(c.age / 0.14, 0, 1));
      crack = clamp((c.age - 0.05) / 0.3, 0, 1);
    } else {
      const k = clamp(c.regen / REGEN_S, 0, 1);
      const k2 = ease(k);
      s = 1 - 0.8 * k2;
      flat = 1 - 0.5 * k2;
      alpha = 1 - k2;
      crack = 1 - clamp(k / 0.35, 0, 1);
    }
    this.craterG.setAttribute('opacity', f1(alpha));
    CRATER.forEach((d, i) => {
      const e = this.craterRings[i];
      if (!e) return;
      e.setAttribute('cx', f1(c.cx));
      e.setAttribute('cy', f1(c.cy + d.dy * s));
      e.setAttribute('rx', f1(Math.max(0.1, d.rx * s)));
      e.setAttribute('ry', f1(Math.max(0.1, d.ry * s * flat)));
    });
    for (const p of this.craterCracks) p.setAttribute('stroke-dashoffset', (1 - crack).toFixed(3));
    this.craterSpecks.forEach((sp, i) => {
      if (c.regen < 0) {
        sp.setAttribute('opacity', '0');
        return;
      }
      const u = (c.regen - (0.2 + i * 0.28)) / 0.5;
      if (u < 0 || u > 1) {
        sp.setAttribute('opacity', '0');
        return;
      }
      sp.setAttribute('cx', f1(c.cx + (c.specks[i] ?? 0) * CRATER[0].rx * s));
      sp.setAttribute('cy', f1(c.cy - 34 * (1 - u * u)));
      sp.setAttribute('opacity', f1(u < 0.85 ? 1 : (1 - u) / 0.15));
    });
  }

  private updateFx(dt: number): void {
    // stage shake (the whole drawing layer, decaying)
    if (this.shakeT >= 0) {
      this.shakeT += dt;
      if (this.shakeT >= SHAKE_S) {
        this.shakeT = -1;
        this.world.style.transform = '';
      } else {
        const amp = 4 * (1 - this.shakeT / SHAKE_S);
        this.world.style.transform = `translate(${f1(rnd(-amp, amp))}px, ${f1(rnd(-amp, amp) * 0.7)}px)`;
      }
    }

    // scale pop (impact) and squash (landing after a flight)
    this.sx = this.sy = 1;
    if (this.popT >= 0) {
      this.popT += dt;
      const u = this.popT / POP_S;
      if (u >= 1) this.popT = -1;
      else this.sx = this.sy = 1 + 0.08 * Math.sin(Math.PI * u);
    }
    if (this.landT >= 0) {
      this.landT += dt;
      if (this.landT > 0.7) {
        this.landT = -1;
      } else {
        const sq = Math.exp(-7 * this.landT) * Math.cos(20 * this.landT);
        this.sy = 1 - 0.1 * sq;
        this.sx = 1 + 0.05 * sq;
      }
    }

    // dirt
    for (let i = this.particles.length - 1; i >= 0; i--) {
      const p = this.particles[i];
      if (!p) continue;
      p.age += dt;
      if (p.age >= p.life) {
        p.el.remove();
        this.particles.splice(i, 1);
        continue;
      }
      p.vy += 1100 * dt;
      p.x += p.vx * dt;
      p.y += p.vy * dt;
      p.rot += p.vr * dt;
      const floor = this.crater.cy + 3;
      if (p.y > floor && p.vy > 0) {
        p.y = floor;
        p.vy = 0;
        p.vx *= 0.8;
        p.vr = 0;
      }
      this.drawParticle(p);
    }

    // dust
    for (let i = this.puffs.length - 1; i >= 0; i--) {
      const p = this.puffs[i];
      if (!p) continue;
      p.age += dt;
      const u = p.age / p.life;
      if (u >= 1) {
        p.el.remove();
        this.puffs.splice(i, 1);
        continue;
      }
      const e = 1 - (1 - u) * (1 - u);
      p.el.setAttribute('cx', f1(p.x + p.drift * e));
      p.el.setAttribute('cy', f1(p.y - 16 * e));
      p.el.setAttribute('r', f1(lerp(p.r0, p.r1, e)));
      p.el.setAttribute('opacity', f1(p.alpha * Math.pow(1 - u, 1.2)));
    }

    // crater
    const c = this.crater;
    if (c.active) {
      c.age += dt;
      if (c.regen >= 0) {
        c.regen += dt;
        if (c.regen >= REGEN_S) c.active = false;
      }
      this.drawCrater();
    }
  }

  /* -------------------------------------------------------- flight */

  private flyCenterY(): number {
    const idle = (this.spec as SpriteSpec).poses.idle;
    return GROUND - (idle.h * idle.scale) / 2;
  }

  private startFly(): void {
    const spec = this.spec as SpriteSpec;
    this.mode = 'fly';
    this.fx = this.faceDir;
    this.tilt = 0;
    this.landT = -1;
    this.bubble.hide();
    this.setSignVisible(false, 0.1);
    this.show(spec.poses.fly, spec.anchorX, this.flyCenterY(), 0.1);
  }

  private endFly(): void {
    const spec = this.spec as SpriteSpec;
    this.mode = 'stand';
    this.tilt = 0;
    this.landT = 0;
    this.spawnPuffs(this.clampX(this.standPose(), spec.anchorX), GROUND, 3, 0.45, 0.5);
    this.show(this.standPose(), spec.anchorX, GROUND, 0.12);
    this.layoutSign();
    this.setSignVisible(true, 0.12);
    this.flush(); // a quote held back during the flight
  }

  private updateMotion(dt: number): void {
    const spec = this.spec as SpriteSpec;
    const raw = this.motion.sample();
    this.vx = approach(this.vx, raw.vx, 16, dt);
    this.vy = approach(this.vy, raw.vy, 16, dt);
    const speed = Math.hypot(this.vx, this.vy);
    if (Math.abs(this.vx) > 90) this.faceDir = this.vx > 0 ? 1 : -1;
    if (spec.locomotion === 'fly' && !this.reduced) {
      if (!this.moving && speed > 260) {
        this.moving = true;
        this.startFly();
      } else if (this.moving && speed < 110) {
        this.moving = false;
        this.endFly();
      }
    }
    if (this.mode === 'fly') {
      this.fx += clamp(this.faceDir - this.fx, -dt * 8, dt * 8);
      const lean = Math.atan2(this.vy, Math.max(Math.abs(this.vx), 1)) * DEG * this.faceDir;
      this.tilt = approach(this.tilt, clamp(lean, -25, 25), 10, dt);
    }
  }

  /* ---------------------------------------------------------- loop */

  private applyTransform(): void {
    const img = this.imgs[this.front];
    if (!img || !this.spec) return;
    if (this.mode === 'entrance' && !this.en.landed) {
      const u = clamp(this.en.t / FALL_S, 0, 1);
      img.style.transform = `translateY(${f1(-this.en.big * (1 - u * u))}px) rotate(${this.en.rot}deg)`;
    } else if (this.mode === 'fly') {
      const nat = this.spec.poses.fly.faces === 'left' ? -1 : 1;
      const bob = 3 * Math.sin(this.clock * 9);
      img.style.transform = `translateY(${f1(bob)}px) rotate(${f1(this.tilt)}deg) scaleX(${(this.fx * nat).toFixed(3)})`;
    } else if (this.sx !== 1 || this.sy !== 1) {
      img.style.transform = `scale(${this.sx.toFixed(4)}, ${this.sy.toFixed(4)})`;
    } else {
      img.style.transform = '';
    }
  }

  private update(dt: number): void {
    this.clock += dt;
    if (this.mode === 'entrance') this.updateEntrance(dt);
    else if (this.mode === 'stand' || this.mode === 'fly') this.updateMotion(dt);
    this.updateFx(dt);
    this.applyTransform();
    this.signEl.style.transform =
      this.mode === 'stand' && this.settled && (this.sx !== 1 || this.sy !== 1)
        ? `scale(${this.sx.toFixed(4)}, ${this.sy.toFixed(4)})`
        : '';
  }

  private busy(): boolean {
    return (
      this.mode === 'entrance' ||
      this.mode === 'fly' ||
      this.moving ||
      Math.hypot(this.vx, this.vy) > 3 ||
      this.landT >= 0 ||
      this.popT >= 0 ||
      this.shakeT >= 0 ||
      this.particles.length > 0 ||
      this.puffs.length > 0 ||
      this.crater.active
    );
  }

  private readonly tick = (ts: number): void => {
    this.raf = 0;
    if (this.mode === 'off' || this.reduced || document.hidden) return;
    const dt = this.last ? Math.min((ts - this.last) / 1000, 0.05) : 1 / 60;
    this.last = ts;
    this.update(dt);
    if (this.busy()) {
      this.raf = requestAnimationFrame(this.tick);
    } else {
      this.last = 0;
    }
  };

  private ensureLoop(): void {
    if (this.raf !== 0 || this.mode === 'off' || this.reduced || document.hidden) return;
    this.last = 0;
    this.raf = requestAnimationFrame(this.tick);
  }

  private stopLoop(): void {
    if (this.raf !== 0) cancelAnimationFrame(this.raf);
    this.raf = 0;
    this.last = 0;
  }
}

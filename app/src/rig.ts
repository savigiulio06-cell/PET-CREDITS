// Procedural stick-figure rig: joints are computed in TS every frame and drawn as SVG paths.
// Local coordinates face +x; the final x is mirrored by the facing factor `fx` (+1 right, -1 left).
import { ARM, GROUND, HAND_REST, HIP_X, LEG, SIGN, STAGE_H, STAGE_W, TORSO } from './layout';
import type { MotionTracker } from './motion';

const NS = 'http://www.w3.org/2000/svg';
const RAD = Math.PI / 180;

type V = [number, number];

export interface RigFlags {
  sleeping: boolean;
  warn: boolean;
  critical: boolean;
  working: boolean;
}

export interface RigStyle {
  color: string;
  stroke: number;
  head: number;
}

const clamp = (v: number, a: number, b: number): number => Math.min(b, Math.max(a, v));
const lerp = (a: number, b: number, w: number): number => a + (b - a) * w;
/** Frame-rate independent exponential follow. */
const approach = (cur: number, target: number, rate: number, dt: number): number =>
  cur + (target - cur) * (1 - Math.exp(-rate * dt));
const f1 = (n: number): string => n.toFixed(1);

function shade(hex: string, amount: number): string {
  let h = hex.replace('#', '');
  if (h.length === 3) h = h.split('').map((c) => c + c).join('');
  const n = parseInt(h, 16);
  const ch = (shift: number): number => {
    const c = (n >> shift) & 255;
    return Math.round(clamp(amount < 0 ? c * (1 + amount) : c + (255 - c) * amount, 0, 255));
  };
  return `rgb(${ch(16)}, ${ch(8)}, ${ch(0)})`;
}

/** Two-bone IK: returns the middle joint and the (possibly clamped) end point. */
function ik(a: V, target: V, l1: number, l2: number, bend: 1 | -1): { joint: V; end: V } {
  const dx = target[0] - a[0];
  const dy = target[1] - a[1];
  const dist = Math.hypot(dx, dy) || 0.001;
  const d = clamp(dist, Math.abs(l1 - l2) + 0.5, l1 + l2 - 0.2);
  const ux = dx / dist;
  const uy = dy / dist;
  const x = (d * d + l1 * l1 - l2 * l2) / (2 * d);
  const h = Math.sqrt(Math.max(l1 * l1 - x * x, 0));
  const joint: V = [a[0] + ux * x - uy * h * bend, a[1] + uy * x + ux * h * bend];
  return { joint, end: [a[0] + ux * d, a[1] + uy * d] };
}

function limb(...pts: V[]): string {
  return pts.map((p, i) => `${i === 0 ? 'M' : 'L'}${f1(p[0])} ${f1(p[1])}`).join(' ');
}

function svgEl<K extends keyof SVGElementTagNameMap>(
  tag: K,
  attrs: Record<string, string> = {},
): SVGElementTagNameMap[K] {
  const el = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  return el;
}

interface Spring {
  x: number;
  v: number;
}

function stepSpring(s: Spring, k: number, c: number, drive: number, dt: number): void {
  const n = Math.max(1, Math.ceil(dt / (1 / 120)));
  const h = dt / n;
  for (let i = 0; i < n; i++) {
    s.v += (-k * s.x - c * s.v + drive) * h;
    s.x += s.v * h;
  }
}

export class Rig {
  private readonly sign: HTMLElement;
  private readonly motion: MotionTracker;
  private style: RigStyle = { color: '#f28c28', stroke: 9, head: 17 };
  private flags: RigFlags = { sleeping: false, warn: false, critical: false, working: false };
  private reduced = false;

  // drawn parts
  private readonly shadowFig = svgEl('ellipse', { ry: '5', fill: '#000', opacity: '0.2' });
  private readonly shadowSign = svgEl('ellipse', { ry: '5', fill: '#000', opacity: '0.13' });
  private readonly legBack = svgEl('path');
  private readonly legFront = svgEl('path');
  private readonly torso = svgEl('path');
  private readonly head = svgEl('circle');
  private readonly armFree = svgEl('path');
  private readonly rod = svgEl('line');
  private readonly armHold = svgEl('path');
  private readonly grip = svgEl('circle');
  private readonly eyeA = svgEl('ellipse', { rx: '2.2', fill: '#20202a' });
  private readonly eyeB = svgEl('ellipse', { rx: '2.2', fill: '#20202a' });
  private readonly mouth = svgEl('path', { fill: 'none', stroke: '#20202a', 'stroke-width': '2', 'stroke-linecap': 'round' });
  private readonly sweat = svgEl('path', { d: 'M0 -7 C4 -1 6 2 0 6 C-6 2 -4 -1 0 -7 Z', fill: '#9bdcff', stroke: '#4fa9e3', 'stroke-width': '1.6' });
  private readonly zs: SVGPathElement[] = [0, 1, 2].map(() =>
    svgEl('path', { d: 'M-4 -4 H4 L-4 4 H4', fill: 'none', stroke: '#6f7aa6', 'stroke-width': '2.4', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }),
  );
  private readonly alert = svgEl('g');

  // dynamic state
  private t = 0;
  private vx = 0;
  private vy = 0;
  private ax = 0;
  private ay = 0;
  private run = 0;
  private air = 0;
  private sit = 0;
  private fx = 1;
  private fTarget = 1;
  private lastMoveT = -10;
  private phase = 0;
  private lean = 3;
  private tilt = 0;
  private armU = -22;
  private armV = -8;
  private moving = false;
  private peak = 0;
  private bounce: Spring = { x: 0, v: 0 };
  private pend: Spring = { x: 0, v: 0 }; // sign angle (deg)
  private offX: Spring = { x: 0, v: 0 }; // hand/sign lag (px)
  private offY: Spring = { x: 0, v: 0 };
  private nextBlink = 2;
  private blinkEnd = 0;
  private nextEvent = 4;
  private event: 'look' | 'tap' | null = null;
  private eventEnd = 0;
  private lookW = 0;
  private tapW = 0;

  private raf = 0;
  private last = 0;
  private running = false;

  constructor(svg: SVGSVGElement, sign: HTMLElement, motion: MotionTracker) {
    this.sign = sign;
    this.motion = motion;
    svg.setAttribute('viewBox', `0 0 ${STAGE_W} ${STAGE_H}`);
    for (const p of [this.legBack, this.legFront, this.torso, this.armFree, this.armHold, this.mouth]) {
      p.setAttribute('fill', 'none');
      p.setAttribute('stroke-linecap', 'round');
      p.setAttribute('stroke-linejoin', 'round');
    }
    this.rod.setAttribute('stroke', '#5b4b3b');
    this.rod.setAttribute('stroke-width', '5');
    this.rod.setAttribute('stroke-linecap', 'round');
    this.alert.append(
      svgEl('circle', { r: '11', fill: '#e5484d' }),
      svgEl('rect', { x: '-2', y: '-7', width: '4', height: '9', rx: '2', fill: '#fff' }),
      svgEl('circle', { cy: '5.5', r: '2.2', fill: '#fff' }),
    );
    svg.append(
      this.shadowSign, this.shadowFig, this.legBack, this.armFree, this.legFront, this.torso, this.head,
      this.eyeA, this.eyeB, this.mouth, this.rod, this.armHold, this.grip,
      this.sweat, ...this.zs, this.alert,
    );
    sign.style.transformOrigin = `${HAND_REST.x - SIGN.x}px 0px`;
    this.setStyle(this.style);
    document.addEventListener('visibilitychange', () => this.updateLoop());
  }

  setStyle(style: RigStyle): void {
    this.style = style;
    const dark = shade(style.color, -0.22);
    for (const p of [this.torso, this.legFront, this.armHold]) p.setAttribute('stroke', style.color);
    for (const p of [this.legBack, this.armFree]) p.setAttribute('stroke', dark);
    for (const p of [this.torso, this.legBack, this.legFront, this.armFree, this.armHold]) {
      p.setAttribute('stroke-width', String(style.stroke));
    }
    this.head.setAttribute('fill', style.color);
    this.head.setAttribute('r', String(style.head));
    this.grip.setAttribute('fill', style.color);
    this.grip.setAttribute('r', String(style.stroke / 2 + 1));
    this.renderIfStatic();
  }

  setFlags(flags: RigFlags): void {
    this.flags = flags;
    this.renderIfStatic();
  }

  setReduced(reduced: boolean): void {
    this.reduced = reduced;
    this.updateLoop();
    this.renderIfStatic();
  }

  /* ---------------------------------------------------------- loop */
  private updateLoop(): void {
    const should = !this.reduced && !document.hidden;
    if (should && !this.running) {
      this.running = true;
      this.last = 0;
      this.raf = requestAnimationFrame((ts) => this.tick(ts));
    } else if (!should && this.running) {
      this.running = false;
      cancelAnimationFrame(this.raf);
    }
  }

  private tick(ts: number): void {
    if (!this.running) return;
    this.raf = requestAnimationFrame((n) => this.tick(n));
    const active = this.moving || this.air > 0.02 || this.run > 0.02 || Math.abs(this.bounce.x) > 0.2 || Math.abs(this.pend.x) > 0.2;
    const minInterval = active ? 15 : 33; // ~60fps while moving, ~30fps at rest
    if (this.last && ts - this.last < minInterval) return;
    const dt = this.last ? Math.min((ts - this.last) / 1000, 0.05) : 1 / 60;
    this.last = ts;
    this.t += dt;
    this.frame(dt);
  }

  private renderIfStatic(): void {
    if (this.reduced) {
      this.resetDynamics();
      this.frame(0);
    }
  }

  private resetDynamics(): void {
    this.vx = this.vy = this.ax = this.ay = this.run = this.air = 0;
    this.fx = this.fTarget = 1;
    this.bounce = { x: 0, v: 0 };
    this.pend = { x: 0, v: 0 };
    this.offX = { x: 0, v: 0 };
    this.offY = { x: 0, v: 0 };
    this.moving = false;
    this.sit = this.flags.sleeping ? 1 : 0;
    this.t = 0;
  }

  /* --------------------------------------------------------- pose */
  private frame(dt: number): void {
    const anim = this.reduced ? 0 : 1;
    const t = this.t;
    const { stroke, head: headR } = this.style;
    const f = this.flags;
    const mx = (lx: number): number => HIP_X + this.fx * lx;
    const footY = GROUND - stroke / 2;
    const hipBase = footY - (LEG * 2 - 3);

    /* ---- motion input ---- */
    if (anim && dt > 0) {
      const raw = this.motion.sample();
      const pvx = this.vx;
      const pvy = this.vy;
      this.vx = approach(this.vx, raw.vx, 16, dt);
      this.vy = approach(this.vy, raw.vy, 16, dt);
      this.ax = approach(this.ax, clamp((this.vx - pvx) / dt, -14000, 14000), 25, dt);
      this.ay = approach(this.ay, clamp((this.vy - pvy) / dt, -14000, 14000), 25, dt);

      const speed = Math.hypot(this.vx, this.vy);
      this.peak = Math.max(this.peak, speed);
      if (!this.moving && speed > 260) this.moving = true;
      else if (this.moving && speed < 110) {
        this.moving = false;
        // landing: little squash and rebound
        this.bounce.v += 110 + 220 * clamp(this.peak / 1100, 0, 1) + 160 * this.air;
        this.peak = 0;
      }

      const airT = clamp((Math.abs(this.vy) - 140) / 300, 0, 1) * (f.sleeping ? 0.4 : 1);
      this.air = approach(this.air, airT, airT > this.air ? 9 : 9, dt);
      const runT = clamp(Math.abs(this.vx) / 600, 0, 1) * (1 - this.air * 0.7) * (f.sleeping ? 0.3 : 1);
      this.run = approach(this.run, runT, 10, dt);

      if (Math.abs(this.vx) > 90) {
        this.fTarget = this.vx > 0 ? 1 : -1;
        this.lastMoveT = t;
      } else if (t - this.lastMoveT > 1.5 || f.sleeping) {
        this.fTarget = 1;
      }
      this.fx += clamp(this.fTarget - this.fx, -dt * 8, dt * 8);

      if (this.run > 0.04) {
        const freq = 1.3 + 3.2 * this.run;
        this.phase += 2 * Math.PI * freq * dt;
      }

      // springs: sign pendulum and hand lag, driven by acceleration
      const thetaTarget = clamp(this.vx * 0.004, -3.5, 3.5);
      stepSpring(this.pend, 100, 5.5, 100 * thetaTarget + 0.1 * this.ax, dt);
      stepSpring(this.offX, 190, 11, -0.2 * this.ax, dt);
      stepSpring(this.offY, 190, 11, -0.2 * this.ay, dt);
      this.offX.x = clamp(this.offX.x, -5, 5);
      this.offY.x = clamp(this.offY.x, -5, 5);
      stepSpring(this.bounce, 300, 15, 0, dt);
      this.sit = approach(this.sit, f.sleeping ? 1 : 0, 3, dt);
    } else if (anim === 0) {
      this.sit = f.sleeping ? 1 : 0;
    }
    const sit = this.sit;
    const run = this.run;
    const air = this.air;
    const hold = 1 - sit;

    /* ---- idle events ---- */
    const calm = !f.sleeping && !f.warn && !f.critical && !f.working && !this.moving && anim === 1;
    if (calm && !this.event && t > this.nextEvent) {
      this.event = Math.random() < 0.55 ? 'look' : 'tap';
      this.eventEnd = t + (this.event === 'look' ? 1.8 : 2);
    }
    if (this.event && (t > this.eventEnd || !calm)) {
      this.event = null;
      this.nextEvent = t + 5 + Math.random() * 5;
    }
    this.lookW = approach(this.lookW, this.event === 'look' ? 1 : 0, 6, Math.max(dt, 0.001));
    this.tapW = approach(this.tapW, this.event === 'tap' ? 1 : 0, 10, Math.max(dt, 0.001));

    /* ---- hip ---- */
    const breath = Math.sin(t * (f.working ? 3.6 : 2.2)) * 0.7 * anim * (1 - sit);
    const trem = anim * (f.critical ? 1.5 * Math.sin(t * 46) : f.warn ? 0.7 * Math.sin(t * 40) : 0);
    const hipLx = lerp(0, -14, sit);
    const sitHipY = GROUND - stroke / 2 - 3;
    let hipY = lerp(hipBase, sitHipY, sit);
    hipY += this.bounce.x * (1 - sit) + breath - air * 14 - run * 2.4 * Math.abs(Math.sin(this.phase));
    const hip: V = [hipLx, hipY];

    /* ---- torso, head ---- */
    const leanT = 3 + 1.4 * Math.sin(t * 1.6) * anim + run * 15 + sit * 32 + (f.working ? 3 : 0) - air * 4;
    this.lean = anim ? approach(this.lean, leanT, 10, dt) : leanT;
    const lam = this.lean * RAD;
    const neck: V = [hip[0] + TORSO * Math.sin(lam), hip[1] - TORSO * Math.cos(lam)];
    const shoulder: V = [hip[0] + 0.9 * TORSO * Math.sin(lam), hip[1] - 0.9 * TORSO * Math.cos(lam)];
    const tiltT =
      6 * this.lookW + 2 * Math.sin(t * 0.9) * anim + sit * 30 + (f.warn ? 4 * Math.sin(t * 7) * anim : 0) - run * 6 + air * 8;
    this.tilt = anim ? approach(this.tilt, tiltT, 9, dt) : tiltT;
    const headA = lam + this.tilt * RAD;
    const headC: V = [neck[0] + (headR - 2) * Math.sin(headA), neck[1] - (headR - 2) * Math.cos(headA)];

    /* ---- legs (IK, feet planted unless lifted) ---- */
    const stanceW = 1 - run;
    const s = lerp(8, 30, run);
    const lift = lerp(3, 17, run);
    const legs: { knee: V; foot: V }[] = [0, 1].map((i) => {
      const th = this.phase + i * Math.PI;
      const stance = (i === 1 ? 7 : -7) * stanceW;
      let fxl = stance + (run > 0.03 ? s * Math.sin(th) * Math.min(1, run * 4) : 0) + hipLx * 0;
      let fyl = footY - (run > 0.03 ? lift * Math.max(0, Math.cos(th)) * Math.min(1, run * 4) : 0);
      if (i === 1) fyl -= this.tapW * 5 * Math.max(0, Math.sin(t * 15));
      if (f.working && !run) fyl -= i === 1 ? 2 * Math.max(0, Math.sin(t * 8)) * anim : 0;
      // sitting: feet forward, knees up
      fxl = lerp(fxl, 26 + i * 11, sit);
      fyl = lerp(fyl, footY, sit);
      // dangling when lifted
      const dx = (i === 1 ? 7 : -7) - this.vx * 0.012 + 10 * Math.sin(t * 7 + i * 2.1) * anim;
      const dy = hipY + 58 + 4 * Math.sin(t * 9 + i * 1.3) * anim;
      fxl = lerp(fxl, hipLx + dx, air);
      fyl = lerp(fyl, dy, air);
      const r = ik(hip, [fxl, fyl], LEG, LEG, -1);
      return { knee: r.joint, foot: r.end };
    });
    const [lb, lf] = legs as [{ knee: V; foot: V }, { knee: V; foot: V }];

    /* ---- free arm ---- */
    let u = -22 + 2 * Math.sin(t * 1.3) * anim;
    let v = u + 14;
    if (f.working) {
      u = 15 + 25 * Math.sin(t * 4.2) * anim;
      v = u + 50 + 15 * Math.sin(t * 8.1) * anim;
    }
    if (f.warn) {
      u = 100;
      v = 200 + 14 * Math.sin(t * 7) * anim;
    }
    if (f.critical) {
      u = 165 + 15 * Math.sin(t * 11) * anim;
      v = u + 25 + 30 * Math.sin(t * 11 + 1) * anim;
    }
    // running: arm swings against the leading leg
    const sw = 50 * run;
    u = lerp(u, -sw * Math.sin(this.phase), run);
    v = lerp(v, u + 10 + 60 * run, run);
    // lifted: arm flails up
    u = lerp(u, 168 + 14 * Math.sin(t * 9 + 1) * anim, air);
    v = lerp(v, u + 30 * Math.sin(t * 12) * anim, air);
    if (sit > 0) {
      u = lerp(u, 45, sit);
      v = lerp(v, 100, sit);
    }
    this.armU = anim ? approach(this.armU, u, 14, dt) : u;
    this.armV = anim ? approach(this.armV, v, 14, dt) : v;
    const eFree: V = [shoulder[0] + ARM * Math.sin(this.armU * RAD), shoulder[1] + ARM * Math.cos(this.armU * RAD)];
    const hFree: V = [eFree[0] + ARM * Math.sin(this.armV * RAD), eFree[1] + ARM * Math.cos(this.armV * RAD)];

    /* ---- world mapping ---- */
    const W = (p: V): V => [mx(p[0]) + trem, p[1]];
    const hipW = W(hip);
    const neckW = W(neck);
    const shW = W(shoulder);
    const headW = W(headC);

    /* ---- holding arm (world space) ---- */
    const shNomX = HIP_X + 0.9 * TORSO * Math.sin(3 * RAD);
    const shNomY = hipBase - 0.9 * TORSO * Math.cos(3 * RAD);
    const handHold: V = [
      HAND_REST.x + this.offX.x + 0.6 * (shW[0] - shNomX),
      HAND_REST.y + this.offY.x + 0.6 * (shW[1] - shNomY),
    ];
    const lap: V = [W(lf.knee)[0] - 4, W(lf.knee)[1] - 2];
    const target: V = [lerp(lap[0], handHold[0], hold), lerp(lap[1], handHold[1], hold)];
    const holdIk = ik(shW, target, ARM, ARM, 1);

    /* ---- sign ---- */
    const sdx = (holdIk.end[0] - HAND_REST.x) * hold;
    const sdy = (holdIk.end[1] - HAND_REST.y) * hold;
    const theta = clamp(
      this.pend.x + (f.working ? 1.2 * Math.sin(t * 3.2) : 0.5 * Math.sin(t * 1.1)) * anim,
      -5.5,
      5.5,
    );
    this.sign.style.transform = anim ? `translate(${f1(sdx)}px, ${f1(sdy)}px) rotate(${theta.toFixed(2)}deg)` : '';
    const th = theta * RAD;
    const px = HAND_REST.x + sdx;
    const py = HAND_REST.y + sdy;
    const along = (d: number): V => [px + d * Math.cos(th), py + d * Math.sin(th)];
    const rodA = along(-8);
    const rodB = along(SIGN.x - HAND_REST.x + SIGN.w - 2);
    this.rod.setAttribute('x1', f1(rodA[0]));
    this.rod.setAttribute('y1', f1(rodA[1]));
    this.rod.setAttribute('x2', f1(rodB[0]));
    this.rod.setAttribute('y2', f1(rodB[1]));
    this.rod.setAttribute('opacity', '1');

    /* ---- draw ---- */
    this.legBack.setAttribute('d', limb(hipW, W(lb.knee), W(lb.foot)));
    this.legFront.setAttribute('d', limb(hipW, W(lf.knee), W(lf.foot)));
    this.torso.setAttribute('d', limb(hipW, neckW));
    this.head.setAttribute('cx', f1(headW[0]));
    this.head.setAttribute('cy', f1(headW[1]));
    this.armFree.setAttribute('d', limb(shW, W(eFree), W(hFree)));
    this.armHold.setAttribute('d', limb(shW, holdIk.joint, holdIk.end));
    this.grip.setAttribute('cx', f1(holdIk.end[0]));
    this.grip.setAttribute('cy', f1(holdIk.end[1]));

    // face: minimal, turns with the body (squeezes while the figure flips)
    if (anim && t > this.nextBlink) {
      this.blinkEnd = t + 0.12;
      this.nextBlink = t + 2.5 + Math.random() * 3;
    }
    const blink = t < this.blinkEnd || f.sleeping ? 0.12 : 1;
    const dart = f.working ? 1.6 * Math.sin(t * 3) * anim : 0;
    const look = 2.2 * this.lookW;
    const cosH = Math.cos(headA);
    const sinH = Math.sin(headA);
    const faceAt = (lx: number, ly: number): V => [
      headW[0] + (this.fx * lx) * cosH - ly * sinH,
      headW[1] + (this.fx * lx) * sinH + ly * cosH,
    ];
    const e1 = faceAt(headR * 0.3 + dart + look, -2);
    const e2 = faceAt(headR * 0.66 + dart + look, -2);
    this.eyeA.setAttribute('cx', f1(e1[0]));
    this.eyeA.setAttribute('cy', f1(e1[1]));
    this.eyeB.setAttribute('cx', f1(e2[0]));
    this.eyeB.setAttribute('cy', f1(e2[1]));
    this.eyeA.setAttribute('ry', f1(2.6 * blink));
    this.eyeB.setAttribute('ry', f1(2.6 * blink));
    const mp = faceAt(headR * 0.5, headR * 0.45);
    if (f.critical && !f.sleeping) {
      this.mouth.setAttribute('d', `M${f1(mp[0] - 2.5)} ${f1(mp[1])} a2.5 3 0 1 0 5 0 a2.5 3 0 1 0 -5 0`);
      this.mouth.setAttribute('opacity', '1');
    } else if (f.warn && !f.sleeping) {
      this.mouth.setAttribute('d', `M${f1(mp[0] - 5)} ${f1(mp[1])} q2.5 -3 5 0 t5 0`);
      this.mouth.setAttribute('opacity', '1');
    } else {
      this.mouth.setAttribute('opacity', '0');
    }
    this.mouth.setAttribute('stroke-width', '2');

    // extras
    const hot = (f.warn || f.critical) && !f.sleeping;
    const sp = anim ? (t * (f.critical ? 1.1 : 0.75)) % 1 : 0.35;
    const sOp = sp < 0.15 ? sp / 0.15 : sp > 0.8 ? (1 - sp) / 0.2 : 1;
    this.sweat.setAttribute('transform', `translate(${f1(headW[0] + this.fx * headR * 0.95)} ${f1(headW[1] - headR * 0.55 + sp * 22)})`);
    this.sweat.setAttribute('opacity', hot ? f1(sOp) : '0');

    const zOn = sit > 0.6 && f.sleeping;
    this.zs.forEach((z, k) => {
      const ph = anim ? (t * 0.35 + k / 3) % 1 : 0.2 + k * 0.28;
      const sc = 0.6 + ph * 0.7;
      z.setAttribute(
        'transform',
        `translate(${f1(headW[0] + 4 + ph * 12)} ${f1(headW[1] - 24 - ph * 34)}) scale(${sc.toFixed(2)})`,
      );
      z.setAttribute('opacity', zOn ? f1(Math.sin(Math.PI * ph)) : '0');
    });

    const alertOn = f.critical && !f.sleeping;
    const pulse = 1 + 0.15 * Math.sin(t * 9) * anim;
    this.alert.setAttribute('transform', `translate(${f1(headW[0] + 30)} ${f1(headW[1] - 34)}) scale(${pulse.toFixed(2)})`);
    this.alert.setAttribute('opacity', alertOn ? '1' : '0');

    // ground shadows
    this.shadowFig.setAttribute('cx', f1(HIP_X));
    this.shadowFig.setAttribute('cy', f1(GROUND + 3));
    this.shadowFig.setAttribute('rx', f1(lerp(30, 40, sit) * (1 - 0.45 * air)));
    this.shadowFig.setAttribute('opacity', f1(0.2 * (1 - 0.6 * air)));
    this.shadowSign.setAttribute('cx', f1(SIGN.x + SIGN.w / 2));
    this.shadowSign.setAttribute('cy', f1(GROUND + 4));
    this.shadowSign.setAttribute('rx', f1(SIGN.w / 2 - 6));
  }
}

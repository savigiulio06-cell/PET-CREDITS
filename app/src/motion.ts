// Turns the stream of `pet-moved` window positions into a velocity (CSS px / s).
export interface Velocity {
  vx: number;
  vy: number;
}

interface Sample {
  t: number;
  x: number;
  y: number;
}

const WINDOW_MS = 110;
const STALE_MS = 110;

export class MotionTracker {
  private samples: Sample[] = [];

  /** `x`,`y` are physical px: converted to CSS px with devicePixelRatio. */
  push(x: number, y: number, now = performance.now()): void {
    if (!Number.isFinite(x) || !Number.isFinite(y)) return;
    const dpr = window.devicePixelRatio || 1;
    this.samples.push({ t: now, x: x / dpr, y: y / dpr });
    this.prune(now);
  }

  private prune(now: number): void {
    while (this.samples.length > 1 && now - (this.samples[0]?.t ?? now) > WINDOW_MS) this.samples.shift();
  }

  sample(now = performance.now()): Velocity {
    this.prune(now);
    const first = this.samples[0];
    const last = this.samples[this.samples.length - 1];
    if (!first || !last || last === first || now - last.t > STALE_MS) {
      // no recent movement: forget old samples so the next drag starts clean
      if (last && now - last.t > 400) this.samples = [last];
      return { vx: 0, vy: 0 };
    }
    const dt = Math.max(last.t - first.t, 16) / 1000;
    return { vx: (last.x - first.x) / dt, vy: (last.y - first.y) / dt };
  }
}

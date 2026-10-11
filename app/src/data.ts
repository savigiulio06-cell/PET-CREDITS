// Data source: Tauri events/commands when running in the app, mock data in a plain browser.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { MOCK_KINDS, mockSnapshot, type MockKind } from './mock';
import type { Issue, MovedPayload, UsageSnapshot } from './types';

export interface DataHandlers {
  onSnapshot(snapshot: UsageSnapshot | null): void;
  onVariant(id: string): void;
  onActivity(active: boolean): void;
  /** Window position in physical px, one call per `pet-moved` event. */
  onMoved(x: number, y: number): void;
}

export const inTauri = (): boolean => '__TAURI_INTERNALS__' in window;

/** Defensive parse: the payload is data from the backend, never trusted blindly. */
export function normalizeSnapshot(raw: unknown): UsageSnapshot | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const r = raw as Partial<UsageSnapshot>;
  if (!Array.isArray(r.windows) || typeof r.fetched_at !== 'string') return null;
  const windows = r.windows.filter(
    (w) => typeof w === 'object' && w !== null && typeof w.used_pct === 'number' && Number.isFinite(w.used_pct),
  );
  return {
    provider: typeof r.provider === 'string' ? r.provider : 'claude',
    windows,
    session: normalizeSession(r.session),
    source: r.source ?? 'none',
    fetched_at: r.fetched_at,
    stale: r.stale === true,
    note: typeof r.note === 'string' ? r.note : null,
    issue: normalizeIssue(r.issue),
  };
}

const ISSUES: readonly string[] = ['auth_expired', 'login_missing', 'rate_limited', 'api_unavailable'];

/** Unknown or missing values become null (older backends, newer enum variants). */
function normalizeIssue(raw: unknown): Issue | null {
  return typeof raw === 'string' && ISSUES.includes(raw) ? (raw as Issue) : null;
}

function normalizeSession(raw: unknown): UsageSnapshot['session'] {
  if (typeof raw !== 'object' || raw === null) return null;
  const s = raw as Partial<NonNullable<UsageSnapshot['session']>>;
  if (typeof s.tokens !== 'object' || s.tokens === null) return null;
  return {
    session_id: typeof s.session_id === 'string' ? s.session_id : '',
    project: typeof s.project === 'string' ? s.project : null,
    last_activity: typeof s.last_activity === 'string' ? s.last_activity : null,
    tokens: s.tokens,
    cost_usd_est: typeof s.cost_usd_est === 'number' ? s.cost_usd_est : null,
    agents: Array.isArray(s.agents) ? s.agents.filter((a) => typeof a === 'object' && a !== null && !!a.tokens) : [],
  };
}

export async function startData(h: DataHandlers): Promise<void> {
  if (inTauri()) {
    await listen<unknown>('usage-update', (e) => h.onSnapshot(normalizeSnapshot(e.payload)));
    await listen<string>('set-variant', (e) => {
      if (typeof e.payload === 'string') h.onVariant(e.payload);
    });
    await listen<boolean>('activity', (e) => h.onActivity(e.payload === true));
    await listen<MovedPayload>('pet-moved', (e) => {
      const p = e.payload;
      if (typeof p === 'object' && p !== null && typeof p.x === 'number' && typeof p.y === 'number') h.onMoved(p.x, p.y);
    });
    try {
      // The saved skin: an event emitted at start-up could fire before this page listens.
      const variant = await invoke<unknown>('get_variant');
      if (typeof variant === 'string') h.onVariant(variant);
    } catch {
      // keep the locally remembered skin
    }
    try {
      h.onSnapshot(normalizeSnapshot(await invoke<unknown>('get_snapshot')));
    } catch {
      h.onSnapshot(null);
    }
    return;
  }

  const params = new URLSearchParams(window.location.search);
  const raw = params.get('mock') ?? 'ok';
  const kind = (MOCK_KINDS as readonly string[]).includes(raw) ? (raw as MockKind) : 'ok';
  h.onSnapshot(mockSnapshot(kind));
  if (kind === 'working') h.onActivity(true);
  if (params.get('drag') === '1') startMockDrag(h);
}

/**
 * Browser-only: fake `pet-moved` events (right, left, up and down, then stop) so the
 * running/dangling/settling animations can be checked without Tauri.
 * Timeline (s): right 0.4-1.6, left 1.7-2.9, up 3.0-3.6, down 3.6-4.2, stop.
 */
function startMockDrag(h: DataHandlers): void {
  const dpr = window.devicePixelRatio || 1;
  const segments: { from: number; to: number; ax: 'x' | 'y'; dir: 1 | -1; peak: number }[] = [
    { from: 0.4, to: 1.6, ax: 'x', dir: 1, peak: 1200 },
    { from: 1.7, to: 2.9, ax: 'x', dir: -1, peak: 1200 },
    { from: 3.0, to: 3.6, ax: 'y', dir: -1, peak: 1000 },
    { from: 3.6, to: 4.2, ax: 'y', dir: 1, peak: 1000 },
  ];
  let x = 800 * dpr;
  let y = 500 * dpr;
  const start = performance.now();
  let prev = 0;
  const timer = window.setInterval(() => {
    const now = (performance.now() - start) / 1000;
    const dt = now - prev;
    prev = now;
    for (const s of segments) {
      if (now < s.from || now > s.to) continue;
      const v = s.peak * Math.sin((Math.PI * (now - s.from)) / (s.to - s.from)) * dpr;
      if (s.ax === 'x') x += s.dir * v * dt;
      else y += s.dir * v * dt;
      h.onMoved(x, y);
    }
    if (now > 4.5) window.clearInterval(timer);
  }, 16);
}

/** Tells the backend which window size is wanted (320x280 compact, 320x460 expanded). */
export async function setExpanded(expanded: boolean): Promise<void> {
  if (!inTauri()) return;
  try {
    await invoke('set_expanded', { expanded });
  } catch {
    // the backend may not support resizing: the layout still works
  }
}

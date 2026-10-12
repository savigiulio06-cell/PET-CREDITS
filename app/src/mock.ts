// Invented browser-only fixtures (?mock=ok|low|warn|critical|stale|nodata|working|auth_expired).
import type { UsageSnapshot, AgentUsage, UsageWindow } from './types';

export type MockKind = 'ok' | 'low' | 'warn' | 'critical' | 'stale' | 'nodata' | 'working' | 'auth_expired';
export const MOCK_KINDS: readonly MockKind[] = ['ok', 'low', 'warn', 'critical', 'stale', 'nodata', 'working', 'auth_expired'];

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

function iso(ms: number): string {
  return new Date(ms).toISOString();
}

function agents(): AgentUsage[] {
  return [
    {
      agent: 'main',
      model: 'claude-sonnet-4-5-20250929',
      tokens: { input: 18_000, output: 96_000, cache_creation: 210_000, cache_read: 760_000 },
      cost_usd_est: 0.31,
      requests: 42,
    },
    {
      agent: 'explorer',
      model: 'claude-haiku-4-5-20251001',
      tokens: { input: 6_000, output: 12_000, cache_creation: 34_000, cache_read: 58_000 },
      cost_usd_est: 0.06,
      requests: 9,
    },
    {
      agent: 'reviewer',
      model: 'claude-opus-4-1-20250805',
      tokens: { input: 4_000, output: 9_000, cache_creation: 21_000, cache_read: 30_000 },
      cost_usd_est: 0.05,
      requests: 4,
    },
  ];
}

function windows(session: number, weekly: number, opus: number, now: number): UsageWindow[] {
  return [
    {
      kind: 'session',
      label: 'Session (5h)',
      used_pct: session,
      resets_at: iso(now + 1 * HOUR + 20 * MIN),
      window_minutes: 300,
    },
    {
      kind: 'weekly',
      label: 'Weekly',
      used_pct: weekly,
      resets_at: iso(now + 3 * DAY + 4 * HOUR),
      window_minutes: 10080,
    },
    {
      kind: 'weekly_scoped',
      label: 'Weekly · Opus',
      used_pct: opus,
      resets_at: iso(now + 3 * DAY + 4 * HOUR),
      window_minutes: 10080,
    },
  ];
}

export function mockSnapshot(kind: MockKind, now = Date.now()): UsageSnapshot {
  const base: UsageSnapshot = {
    provider: 'claude',
    windows: windows(42, 18, 31, now),
    session: {
      session_id: 'demo-session',
      project: 'demo-project',
      last_activity: iso(now - 12 * MIN),
      tokens: { input: 32_000, output: 117_000, cache_creation: 265_000, cache_read: 848_000 },
      cost_usd_est: 0.42,
      agents: agents(),
    },
    source: 'api',
    fetched_at: iso(now - 40_000),
    stale: false,
    note: null,
    issue: null,
  };

  switch (kind) {
    case 'low':
      // session 93% (>= 90) trips the sprite skins' "low credits" pose
      base.windows = windows(93, 41, 30, now);
      break;
    case 'warn':
      base.windows = windows(86, 72, 64, now);
      base.session!.tokens = { input: 90_000, output: 410_000, cache_creation: 900_000, cache_read: 4_100_000 };
      base.session!.cost_usd_est = 2.87;
      break;
    case 'critical':
      base.windows = windows(97, 91, 96, now);
      base.session!.tokens = { input: 140_000, output: 620_000, cache_creation: 1_300_000, cache_read: 9_400_000 };
      base.session!.cost_usd_est = 7.9;
      base.source = 'statusline';
      base.note = 'Limite quasi raggiunto';
      break;
    case 'stale':
      base.windows = windows(58, 31, 22, now);
      base.fetched_at = iso(now - 25 * MIN);
      base.stale = true;
      base.source = 'local_logs';
      base.note = 'API non raggiungibile, uso i log locali';
      break;
    case 'nodata': {
      const empty: UsageSnapshot = {
        provider: 'claude',
        windows: [],
        session: null,
        source: 'none',
        fetched_at: iso(now),
        stale: true,
        note: null,
        issue: null,
      };
      return empty;
    }
    case 'auth_expired':
      base.windows = windows(58, 31, 22, now);
      base.fetched_at = iso(now - 3 * HOUR);
      base.stale = true;
      base.issue = 'auth_expired';
      base.note = 'API login expired, waiting for Claude Code to renew it';
      break;
    case 'working':
      base.session!.last_activity = iso(now - 3_000);
      break;
    case 'ok':
      break;
  }
  return base;
}

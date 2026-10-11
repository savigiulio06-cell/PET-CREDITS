import { t } from './i18n';

export type Level = 'ok' | 'mid' | 'hi' | 'crit';

/** <60 green, 60-79 yellow, 80-94 orange, >=95 red. */
export function levelOf(pct: number): Level {
  if (pct >= 95) return 'crit';
  if (pct >= 80) return 'hi';
  if (pct >= 60) return 'mid';
  return 'ok';
}

export function clampPct(pct: number): number {
  if (!Number.isFinite(pct)) return 0;
  return Math.min(100, Math.max(0, pct));
}

/** 1234567 -> "1.2M", 340000 -> "340k", 1500 -> "1.5k", 42 -> "42". */
export function formatTokens(n: number): string {
  if (!Number.isFinite(n) || n < 0) return t.noValue;
  if (n >= 1_000_000) {
    const v = n / 1_000_000;
    return `${v >= 100 ? Math.round(v) : trim1(v)}M`;
  }
  if (n >= 1_000) {
    const v = n / 1_000;
    return `${v >= 100 ? Math.round(v) : trim1(v)}k`;
  }
  return String(Math.round(n));
}

function trim1(v: number): string {
  const s = v.toFixed(1);
  return s.endsWith('.0') ? s.slice(0, -2) : s;
}

export function formatCost(usd: number | null | undefined): string {
  if (usd === null || usd === undefined || !Number.isFinite(usd)) return t.noValue;
  return `~$${usd >= 100 ? Math.round(usd) : usd.toFixed(2)}`;
}

export function parseTime(iso: string | null | undefined): number | null {
  if (!iso) return null;
  const ms = Date.parse(iso);
  return Number.isNaN(ms) ? null : ms;
}

const clockFmt = () =>
  new Intl.DateTimeFormat(t.locale, { hour: '2-digit', minute: '2-digit', hour12: false });
const weekdayFmt = () => new Intl.DateTimeFormat(t.locale, { weekday: 'short' });

export function formatClock(iso: string | null | undefined): string {
  const ms = parseTime(iso);
  return ms === null ? t.noValue : clockFmt().format(new Date(ms));
}

/** "1h 20m" within 24h, otherwise "lun 09:00". Empty string when unknown. */
export function formatReset(iso: string | null, now: number): string {
  const ms = parseTime(iso);
  if (ms === null) return '';
  const diff = ms - now;
  if (diff <= 0) return t.resetNow;
  const minutes = Math.ceil(diff / 60_000);
  if (minutes > 24 * 60) {
    const d = new Date(ms);
    return `${weekdayFmt().format(d).replace('.', '')} ${clockFmt().format(d)}`;
  }
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return `${m}m`;
  return m === 0 ? `${h}h` : `${h}h ${m}m`;
}

/** Shortens "claude-sonnet-4-5-20250929" style ids for the small panel. */
export function shortModel(model: string | null): string {
  if (!model) return t.noValue;
  return model.replace(/^claude-/, '').replace(/-\d{8}$/, '');
}

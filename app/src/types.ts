// Mirrors crates/pet-core/src/model.rs (UsageSnapshot serialized as JSON).
// Dates are ISO-8601 UTC strings; Option<T> arrives as T | null.

export type WindowKind = 'session' | 'weekly' | 'weekly_scoped' | 'other';
/** Why the numbers cannot be refreshed (mirrors `Issue` in model.rs). */
export type Issue = 'auth_expired' | 'login_missing' | 'rate_limited' | 'api_unavailable';
export type Source = 'api' | 'statusline' | 'local_logs' | 'none';

export interface UsageWindow {
  kind: WindowKind;
  label: string;
  /** 0..100 (may exceed 100 on overage). */
  used_pct: number;
  resets_at: string | null;
  window_minutes: number | null;
}

export interface TokenTotals {
  input: number;
  output: number;
  cache_creation: number;
  cache_read: number;
}

export interface AgentUsage {
  agent: string;
  model: string | null;
  tokens: TokenTotals;
  cost_usd_est: number | null;
  requests: number;
}

export interface SessionUsage {
  session_id: string;
  project: string | null;
  last_activity: string | null;
  tokens: TokenTotals;
  cost_usd_est: number | null;
  agents: AgentUsage[];
}

export interface UsageSnapshot {
  provider: string;
  windows: UsageWindow[];
  session: SessionUsage | null;
  source: Source;
  fetched_at: string;
  stale: boolean;
  note: string | null;
  issue: Issue | null;
}

/** Skin = a manifest only: the stick figure is drawn procedurally from these parameters. */
export interface PetManifest {
  id: string;
  name: string;
  author: string;
  license: string;
  description: string;
  /** Optional: the backend/UI pick the skin flagged default first. */
  default?: boolean;
  /** Line colour (#rgb / #rrggbb). */
  color: string;
  /** Limb thickness in px (3..16). */
  stroke: number;
  /** Head radius in px (optional). */
  head?: number;
}

/** Window position in physical pixels, as emitted by the backend on `pet-moved`. */
export interface MovedPayload {
  x: number;
  y: number;
}

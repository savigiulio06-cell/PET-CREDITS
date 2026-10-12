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

/** Painted board of a standing pose, in that PNG's pixels; live numbers are drawn on top of it. */
export interface SpriteSign {
  x: number;
  y: number;
  w: number;
  h: number;
  /** Colour of the (blanked) board, used as the sign background. */
  bg: string;
}

/** One sprite pose. Coordinates are in the PNG's pixels; `scale` = CSS px per PNG px. */
export interface SpritePose {
  /** File name as written in the manifest (only a lookup key, never used as a URL). */
  src: string;
  /** Bundled asset URL, resolved from the build-time glob (never from the manifest). */
  url: string;
  w: number;
  h: number;
  scale: number;
  /** Point that sits on the ground (stage anchorX, GROUND); for `fly` the body centre. */
  anchor: [number, number];
  /** Standing poses only (idle, low). */
  sign?: SpriteSign;
  /** `land` only: where the fist hits the ground. */
  impact?: [number, number];
  /** Direction the sprite faces in the PNG. */
  faces: 'left' | 'right';
}

export interface SpriteQuote {
  text: string;
  /** BCP 47 language tag of `text` (optional). */
  lang?: string;
}

export interface SpriteSpec {
  /** Stage x of the anchor of standing poses. */
  anchorX: number;
  poses: { idle: SpritePose; fly: SpritePose; low?: SpritePose; land?: SpritePose };
  entrance: 'crash-landing' | 'none';
  locomotion: 'fly' | 'none';
  /** Used percentage at which the `low` pose (and quote) kicks in. */
  lowThresholds: { session: number; weekly: number };
  quotes: { greeting?: SpriteQuote; low?: SpriteQuote };
}

export type PetKind = 'stick' | 'sprite';

/** Skin = a manifest. Stick skins are drawn procedurally; sprite skins use PNG poses. */
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
  /** Renderer: "stick" (default) or "sprite". */
  kind: PetKind;
  /** Present only when kind is "sprite" and the manifest is valid. */
  sprite?: SpriteSpec;
}

/** Window position in physical pixels, as emitted by the backend on `pet-moved`. */
export interface MovedPayload {
  x: number;
  y: number;
}

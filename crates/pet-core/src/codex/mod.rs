//! Codex provider — not implemented yet.
//!
//! Plan: read the newest `~/.codex/sessions/YYYY/MM/DD/*.jsonl` rollout, take the
//! last `token_count` event and map `rate_limits.primary` (300 min window) to
//! [`crate::WindowKind::Session`] and `rate_limits.secondary` (10080 min) to
//! [`crate::WindowKind::Weekly`] (`used_percent`, `resets_at` epoch seconds).

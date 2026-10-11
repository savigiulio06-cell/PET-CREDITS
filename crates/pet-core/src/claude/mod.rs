//! Claude Code provider.
//!
//! Three sources, combined by [`ClaudeProvider`]:
//! - [`oauth`]: the usage endpoint behind Claude's settings page (live limits);
//! - [`bridge`]: limits pushed by the Claude Code status line (instant, fallback);
//! - [`transcripts`]: local session logs (per-session / per-agent tokens).

pub mod bridge;
pub mod install;
pub mod last_usage;
pub mod oauth;
pub mod pricing;
pub mod provider;
pub mod statusline;
pub mod transcripts;

pub use provider::ClaudeProvider;

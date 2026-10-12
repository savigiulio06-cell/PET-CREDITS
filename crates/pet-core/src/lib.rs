//! Core of PET-CREDITS: a provider-agnostic model of AI-assistant usage limits
//! plus the providers that fill it (Claude Code today, Codex & others later).
//!
//! No GUI code lives here so everything can be unit-tested headless.

pub mod claude;
pub mod codex;
pub mod error;
pub mod fsutil;
pub mod model;
pub mod paths;
pub mod presence;
pub mod provider;

pub use error::{Error, Result};
pub use model::*;
pub use provider::UsageProvider;

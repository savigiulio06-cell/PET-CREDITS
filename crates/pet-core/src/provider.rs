use crate::{Result, UsageSnapshot};

/// A source of usage data for one AI assistant.
///
/// Implementations must be cheap to call repeatedly (the app polls them) and
/// must never log or return credentials.
pub trait UsageProvider: Send {
    /// Stable id, e.g. "claude".
    fn id(&self) -> &'static str;
    /// Display name, e.g. "Claude Code".
    fn name(&self) -> &'static str;
    /// Collect a fresh snapshot, combining whatever sources are available.
    fn poll(&mut self) -> Result<UsageSnapshot>;
    /// True if the user is actively working (used to pick the poll interval).
    fn is_active(&self) -> bool {
        false
    }
}

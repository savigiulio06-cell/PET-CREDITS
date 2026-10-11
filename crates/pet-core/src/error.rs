use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("http: {0}")]
    Http(String),
    /// The server asked us to slow down; retry after the given seconds if known.
    #[error("rate limited by server")]
    RateLimited { retry_after_secs: Option<u64> },
    /// No credentials, expired token, etc. Never contains the token itself.
    #[error("auth: {0}")]
    Auth(String),
    #[error("unexpected format: {0}")]
    Format(String),
    #[error("not available: {0}")]
    Unavailable(String),
}

pub type Result<T> = std::result::Result<T, Error>;

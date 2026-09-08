use std::time::Duration;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("provider `{provider}` is not configured: {reason}")]
    NotConfigured {
        provider: &'static str,
        reason: String,
    },

    #[error("invalid query: {0}")]
    InvalidQuery(String),

    #[error("provider `{provider}` does not support this kind of query")]
    Unsupported { provider: &'static str },

    #[error("no results found")]
    NotFound,

    #[error("provider `{provider}` rejected the credentials")]
    Unauthorized { provider: &'static str },

    #[error("provider `{provider}` rate limited the request")]
    RateLimited {
        provider: &'static str,
        retry_after: Option<Duration>,
    },

    #[error("network error while contacting `{provider}`: {message}")]
    Transport {
        provider: &'static str,
        message: String,
    },

    #[error("provider `{provider}` returned an error: {message}")]
    Api {
        provider: &'static str,
        status: Option<u16>,
        message: String,
    },

    #[error("provider `{provider}` returned an unexpected response: {message}")]
    UnexpectedResponse {
        provider: &'static str,
        message: String,
    },
}

impl Error {
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Transport { .. } | Self::RateLimited { .. })
            || matches!(self, Self::Api { status: Some(s), .. } if *s >= 500)
    }
}

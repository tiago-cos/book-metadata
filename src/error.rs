use std::time::Duration;

/// Convenience alias for results returned by this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Every failure mode a [`MetadataProvider`](crate::MetadataProvider) can report.
///
/// The variants are deliberately provider-agnostic: callers can branch on
/// `NotConfigured` / `Unauthorized` / `RateLimited` without knowing which
/// backend produced the error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The provider is missing configuration it needs before it can be used,
    /// such as an API key.
    #[error("provider `{provider}` is not configured: {reason}")]
    NotConfigured {
        /// Provider that requires configuration.
        provider: &'static str,
        /// What exactly is missing.
        reason: String,
    },

    /// The query itself is malformed (empty title, bad ISBN, ...).
    #[error("invalid query: {0}")]
    InvalidQuery(String),

    /// The provider cannot answer this kind of query.
    #[error("provider `{provider}` does not support this kind of query")]
    Unsupported {
        /// Provider that rejected the query.
        provider: &'static str,
    },

    /// The query was valid but nothing matched.
    #[error("no results found")]
    NotFound,

    /// The credentials were rejected.
    #[error("provider `{provider}` rejected the credentials")]
    Unauthorized {
        /// Provider that rejected the credentials.
        provider: &'static str,
    },

    /// The provider asked us to slow down.
    #[error("provider `{provider}` rate limited the request")]
    RateLimited {
        /// Provider that rate limited us.
        provider: &'static str,
        /// How long to wait before retrying, when the provider says so.
        retry_after: Option<Duration>,
    },

    /// The request never completed (DNS, TLS, timeout, connection reset, ...).
    #[error("network error while contacting `{provider}`: {message}")]
    Transport {
        /// Provider we were talking to.
        provider: &'static str,
        /// Human readable cause.
        message: String,
    },

    /// The provider answered, but with an error status or an error payload.
    #[error("provider `{provider}` returned an error: {message}")]
    Api {
        /// Provider that returned the error.
        provider: &'static str,
        /// HTTP status code, when the failure was an HTTP-level one.
        status: Option<u16>,
        /// Message extracted from the response.
        message: String,
    },

    /// The provider answered with something we could not make sense of.
    /// Usually means the upstream schema changed.
    #[error("provider `{provider}` returned an unexpected response: {message}")]
    UnexpectedResponse {
        /// Provider that returned the response.
        provider: &'static str,
        /// What went wrong while decoding it.
        message: String,
    },
}

impl Error {
    /// Whether retrying the exact same request later could plausibly succeed.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Transport { .. } | Self::RateLimited { .. })
            || matches!(self, Self::Api { status: Some(s), .. } if *s >= 500)
    }
}

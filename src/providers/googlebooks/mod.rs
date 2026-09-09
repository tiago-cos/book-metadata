//! [Google Books](https://books.google.com) metadata provider.
//!
//! The broadest of the three for descriptions and covers, and the cheapest to
//! query: one request answers everything, since Google returns a whole volume
//! per hit rather than making you walk between records.
//!
//! What it does not have is series data. Google indexes a volume's position in
//! a series but never the series name, so [`BookMetadata::series`] is always
//! `None` here — use Hardcover for that.
//!
//! ```no_run
//! # use book_metadata::{providers::googlebooks::GoogleBooksProvider, MetadataProvider, MetadataQuery};
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let provider = GoogleBooksProvider::from_env()?;
//! let book = provider.fetch(&MetadataQuery::isbn("9780441013593")).await?;
//! println!("{}", book.description.unwrap_or_default());
//! # Ok(())
//! # }
//! ```
//!
//! # Configuration
//!
//! An API key is **required**. Google documents anonymous access, but the
//! unauthenticated quota is exhausted on arrival in practice: the first
//! request answers 403 `dailyLimitExceededUnreg` — "Daily Limit for
//! Unauthenticated Use Exceeded. Continued use requires signup". Rather than
//! let that surface as a runtime rate limit nobody can wait out, the key is
//! demanded up front, as Hardcover's is.
//!
//! Create one in the Google Cloud console with the Books API enabled, then:
//!
//! ```no_run
//! # use book_metadata::providers::googlebooks::GoogleBooksProvider;
//! # fn run() -> Result<(), book_metadata::Error> {
//! let provider = GoogleBooksProvider::builder()
//!     .api_key("...")            // or GoogleBooksProvider::from_env()
//!     .build()?;
//! # let _ = provider;
//! # Ok(())
//! # }
//! ```

mod convert;
mod model;

use std::time::Duration;

use async_trait::async_trait;

use crate::error::{Error, Result};
use crate::http;
use crate::metadata::BookMetadata;
use crate::provider::MetadataProvider;
use crate::query::{MetadataQuery, QueryKind};
use model::{ErrorResponse, VolumesResponse};

/// Identifier reported by [`MetadataProvider::name`].
pub const PROVIDER_NAME: &str = convert::SOURCE;

/// The volumes endpoint.
pub const DEFAULT_ENDPOINT: &str = "https://www.googleapis.com/books/v1/volumes";

/// Environment variable read by [`GoogleBooksProvider::from_env`].
pub const API_KEY_ENV: &str = "GOOGLE_BOOKS_API_KEY";

const MAX_RESULTS_PER_REQUEST: usize = 40;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// A configured Google Books client.
///
/// Cloning is cheap and one instance is meant to be reused, so connections are
/// pooled.
#[derive(Clone)]
pub struct GoogleBooksProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    country: Option<String>,
}

impl GoogleBooksProvider {
    /// Builds a provider from an API key.
    ///
    /// Google's unauthenticated quota is exhausted on arrival, so there is no
    /// useful keyless mode to fall back to.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotConfigured`] when the key is blank.
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Self::builder().api_key(api_key).build()
    }

    /// Builds a provider from the `GOOGLE_BOOKS_API_KEY` environment variable.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotConfigured`] when the variable is unset or blank.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var(API_KEY_ENV).map_err(|_| Error::NotConfigured {
            provider: PROVIDER_NAME,
            reason: format!("the `{API_KEY_ENV}` environment variable is not set"),
        })?;
        Self::new(api_key)
    }

    /// Starts a builder for an API key, a country, or the less common knobs.
    #[must_use]
    pub fn builder() -> GoogleBooksBuilder {
        GoogleBooksBuilder::default()
    }

    async fn volumes(&self, search: &str, limit: usize) -> Result<Vec<model::Volume>> {
        let mut params: Vec<(&str, String)> = vec![
            ("q", search.to_owned()),
            ("maxResults", limit.min(MAX_RESULTS_PER_REQUEST).to_string()),
            ("printType", "books".to_owned()),
        ];
        if let Some(country) = &self.country {
            params.push(("country", country.clone()));
        }
        params.push(("key", self.api_key.clone()));

        let response = http::send_raw(
            PROVIDER_NAME,
            self.client.get(&self.endpoint).query(&params),
        )
        .await?;

        let status = response.status;
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !status.is_success() {
            return Err(classify(status, &response.body, response.retry_after));
        }

        let parsed: VolumesResponse = http::decode(PROVIDER_NAME, &response.body)?;
        Ok(parsed.items.unwrap_or_default())
    }

    async fn search_by_isbn(&self, isbn: &str, limit: usize) -> Result<Vec<BookMetadata>> {
        let volumes = self.volumes(&format!("isbn:{isbn}"), limit).await?;

        let (exact, loose): (Vec<_>, Vec<_>) = volumes
            .into_iter()
            .partition(|volume| carries_isbn(volume, isbn));
        let volumes = if exact.is_empty() { loose } else { exact };

        Ok(volumes
            .into_iter()
            .filter_map(|volume| convert::from_volume(volume, Some(isbn)))
            .collect())
    }

    async fn search_by_title(
        &self,
        title: &str,
        author: Option<&str>,
        limit: usize,
    ) -> Result<Vec<BookMetadata>> {
        let search = author.map_or_else(
            || format!("intitle:{}", quote(title)),
            |author| format!("intitle:{} inauthor:{}", quote(title), quote(author)),
        );

        let volumes = self.volumes(&search, limit).await?;
        Ok(volumes
            .into_iter()
            .filter_map(|volume| convert::from_volume(volume, None))
            .collect())
    }
}

#[async_trait]
impl MetadataProvider for GoogleBooksProvider {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<BookMetadata>> {
        query.validate()?;

        let mut results = match query.kind() {
            QueryKind::Isbn(isbn) => self.search_by_isbn(isbn, query.max_results()).await?,
            QueryKind::TitleAuthor { title, author } => {
                self.search_by_title(title, author.as_deref(), query.max_results())
                    .await?
            }
        };

        results.truncate(query.max_results());
        Ok(results)
    }
}

impl std::fmt::Debug for GoogleBooksProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleBooksProvider")
            .field("endpoint", &self.endpoint)
            .field("api_key", &"<redacted>")
            .field("country", &self.country)
            .finish_non_exhaustive()
    }
}

fn carries_isbn(volume: &model::Volume, isbn: &str) -> bool {
    volume
        .volume_info
        .as_ref()
        .and_then(|info| info.industry_identifiers.as_deref())
        .unwrap_or_default()
        .iter()
        .filter_map(|identifier| identifier.identifier.as_deref())
        .any(|identifier| identifier.eq_ignore_ascii_case(isbn))
}

fn quote(term: &str) -> String {
    format!("\"{}\"", term.replace('"', " ").trim())
}

fn classify(status: reqwest::StatusCode, body: &str, retry_after: Option<Duration>) -> Error {
    let parsed: Option<ErrorResponse> = serde_json::from_str(body).ok();
    let error = parsed.and_then(|parsed| parsed.error);

    let reasons: Vec<String> = error
        .as_ref()
        .and_then(|error| error.errors.as_ref())
        .map(|errors| {
            errors
                .iter()
                .filter_map(|detail| detail.reason.clone())
                .collect()
        })
        .unwrap_or_default();

    let message = error
        .as_ref()
        .and_then(|error| error.message.clone())
        .unwrap_or_else(|| http::snippet(body));

    let matches = |needle: &str| {
        reasons
            .iter()
            .any(|reason| reason.to_lowercase().contains(needle))
    };

    if matches("unreg") {
        return Error::NotConfigured {
            provider: PROVIDER_NAME,
            reason: format!(
                "Google rejected the request as unauthenticated; set a valid API key \
                 (or `{API_KEY_ENV}`). Google said: {message}"
            ),
        };
    }

    let exhausted = matches("ratelimit") || matches("quota") || matches("limitexceeded");
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || exhausted {
        return Error::RateLimited {
            provider: PROVIDER_NAME,
            retry_after,
        };
    }
    if matches("key") || matches("credential") || matches("auth") {
        return Error::Unauthorized {
            provider: PROVIDER_NAME,
        };
    }

    Error::Api {
        provider: PROVIDER_NAME,
        status: Some(status.as_u16()),
        message,
    }
}

/// Builder for [`GoogleBooksProvider`].
#[derive(Debug, Clone)]
pub struct GoogleBooksBuilder {
    endpoint: String,
    api_key: Option<String>,
    country: Option<String>,
    timeout: Duration,
    user_agent: String,
    client: Option<reqwest::Client>,
}

impl Default for GoogleBooksBuilder {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_owned(),
            api_key: None,
            country: None,
            timeout: DEFAULT_TIMEOUT,
            user_agent: concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")).to_owned(),
            client: None,
        }
    }
}

impl GoogleBooksBuilder {
    /// Sets the API key. Required: Google refuses unauthenticated requests
    /// outright, so [`build`](Self::build) fails without one.
    #[must_use]
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        let api_key = api_key.into().trim().to_owned();
        self.api_key = (!api_key.is_empty()).then_some(api_key);
        self
    }

    /// Sets the country Google should answer for, as an ISO 3166-1 alpha-2
    /// code such as `"US"` or `"PT"`.
    ///
    /// Google geolocates callers and answers 403 from regions it has not been
    /// configured for. Setting this explicitly is the fix.
    #[must_use]
    pub fn country(mut self, country: impl Into<String>) -> Self {
        self.country = Some(country.into());
        self
    }

    /// Overrides the endpoint. Mostly useful for tests.
    #[must_use]
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// Per-request timeout. Ignored when a custom client is supplied.
    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// `User-Agent` sent with each request. Ignored when a custom client is
    /// supplied.
    #[must_use]
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Uses an existing HTTP client, so the caller keeps control over proxies,
    /// connection pooling and middleware.
    #[must_use]
    pub fn client(mut self, client: reqwest::Client) -> Self {
        self.client = Some(client);
        self
    }

    /// Builds the provider.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotConfigured`] when no API key was set, or
    /// [`Error::Transport`] when the HTTP client cannot be constructed.
    pub fn build(self) -> Result<GoogleBooksProvider> {
        let api_key = self.api_key.ok_or_else(|| Error::NotConfigured {
            provider: PROVIDER_NAME,
            reason: format!(
                "an API key is required; Google refuses unauthenticated requests. \
                 Create one in the Google Cloud console with the Books API enabled \
                 and pass it to `GoogleBooksProvider::new`, or set `{API_KEY_ENV}`"
            ),
        })?;

        let client = match self.client {
            Some(client) => client,
            None => http::build_client(PROVIDER_NAME, &self.user_agent, self.timeout)?,
        };

        Ok(GoogleBooksProvider {
            client,
            endpoint: self.endpoint,
            api_key,
            country: self.country,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_an_api_key() {
        let error = GoogleBooksProvider::builder()
            .build()
            .expect_err("Google refuses unauthenticated requests");
        assert!(matches!(error, Error::NotConfigured { .. }));
        assert!(error.to_string().contains("googlebooks"));

        assert!(matches!(
            GoogleBooksProvider::new("   ").expect_err("a blank key is no key"),
            Error::NotConfigured { .. }
        ));
    }

    #[test]
    fn never_prints_the_api_key() {
        let provider = GoogleBooksProvider::new("super-secret").expect("should build");

        let rendered = format!("{provider:?}");
        assert!(!rendered.contains("super-secret"));
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn quotes_search_terms_as_phrases() {
        assert_eq!(quote(" Dune "), "\"Dune\"");
        assert_eq!(quote(r#"The "Good" Book"#), "\"The  Good  Book\"");
    }

    #[test]
    fn classifies_googles_overloaded_403() {
        for reason in ["dailyLimitExceeded", "rateLimitExceeded", "quotaExceeded"] {
            let body = format!(
                r#"{{"error":{{"code":403,"message":"Limit Exceeded","errors":[{{"reason":"{reason}"}}]}}}}"#
            );
            assert!(
                matches!(
                    classify(reqwest::StatusCode::FORBIDDEN, &body, None),
                    Error::RateLimited { .. }
                ),
                "`{reason}` should read as an exhausted quota"
            );
        }

        let unregistered = r#"{"error":{"code":403,
            "message":"Daily Limit for Unauthenticated Use Exceeded. Continued use requires signup.",
            "errors":[{"reason":"dailyLimitExceededUnreg"}]}}"#;
        match classify(reqwest::StatusCode::FORBIDDEN, unregistered, None) {
            Error::NotConfigured { reason, .. } => {
                assert!(reason.contains("API key"), "got {reason}");
            }
            other => panic!("expected Error::NotConfigured, got {other:?}"),
        }

        let bad_key = r#"{"error":{"code":403,"message":"Bad Request",
            "errors":[{"reason":"keyInvalid"}]}}"#;
        assert!(matches!(
            classify(reqwest::StatusCode::FORBIDDEN, bad_key, None),
            Error::Unauthorized { .. }
        ));

        let region = r#"{"error":{"code":403,
            "message":"It looks like you're making a request from a region that is not supported",
            "errors":[{"reason":"forbidden"}]}}"#;
        match classify(reqwest::StatusCode::FORBIDDEN, region, None) {
            Error::Api {
                message, status, ..
            } => {
                assert_eq!(status, Some(403));
                assert!(message.contains("region"), "got {message}");
            }
            other => panic!("expected Error::Api, got {other:?}"),
        }
    }

    #[test]
    fn falls_back_to_the_raw_body_when_the_error_is_not_json() {
        match classify(reqwest::StatusCode::BAD_GATEWAY, "<html>nope</html>", None) {
            Error::Api { message, .. } => assert!(message.contains("nope")),
            other => panic!("expected Error::Api, got {other:?}"),
        }
    }
}

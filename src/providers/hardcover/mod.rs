//! [Hardcover](https://hardcover.app) metadata provider.
//!
//! Hardcover exposes a public GraphQL API. It is the best of the free sources
//! for book metadata.
//!
//! # Configuration
//!
//! The API requires a personal API key. Create an account, then copy the token
//! from <https://hardcover.app/account/api>. It can be handed over directly or
//! read from the `HARDCOVER_API_KEY` environment variable:
//!
//! ```no_run
//! # use book_metadata::{providers::hardcover::HardcoverProvider, MetadataProvider, MetadataQuery};
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let provider = HardcoverProvider::from_env()?;
//! let book = provider.fetch(&MetadataQuery::isbn("9780441013593")).await?;
//! println!("{} ({:?})", book.title, book.series);
//! # Ok(())
//! # }
//! ```
//!
//! The token the site shows you already starts with `Bearer `; that prefix is
//! stripped automatically, so either form works.
//!
//! Hardcover asks API clients to stay under roughly 60 requests per minute.
//! This crate does no rate limiting of its own — a request that trips the
//! limit surfaces as [`Error::RateLimited`].
//!
//! # Choosing between a work's editions
//!
//! Hardcover files every translation and printing of a book under one work, so
//! a title search has to pick one. It takes the edition with the highest
//! `users_count` — the one most readers actually have — falling back to a
//! completeness heuristic only to separate editions of equal popularity.
//!
//! ISBN lookups skip all of this: they resolve one exact printing, which is
//! the one you asked for.

mod convert;
mod model;
mod queries;

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::http;
use crate::metadata::BookMetadata;
use crate::provider::MetadataProvider;
use crate::query::{MetadataQuery, QueryKind};
use model::{BooksData, EditionsData, GraphQlResponse};

/// Identifier reported by [`MetadataProvider::name`].
pub const PROVIDER_NAME: &str = convert::SOURCE;

/// Hardcover's public GraphQL endpoint.
pub const DEFAULT_ENDPOINT: &str = "https://api.hardcover.app/v1/graphql";

/// Environment variable read by [`HardcoverProvider::from_env`].
pub const API_KEY_ENV: &str = "HARDCOVER_API_KEY";

const EDITIONS_PER_BOOK: u32 = 10;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// A configured Hardcover client.
///
/// Cloning is cheap (the inner HTTP client is shared) and one instance is
/// meant to be reused for the lifetime of your process, so connections are
/// pooled.
#[derive(Clone)]
pub struct HardcoverProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
}

impl HardcoverProvider {
    /// Builds a provider from an API key.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotConfigured`] when the key is blank.
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Self::builder().api_key(api_key).build()
    }

    /// Builds a provider from the `HARDCOVER_API_KEY` environment variable.
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

    /// Starts a builder for the less common knobs: a custom endpoint, a
    /// different timeout, or a pre-configured [`reqwest::Client`].
    #[must_use]
    pub fn builder() -> HardcoverBuilder {
        HardcoverBuilder::default()
    }

    /// The endpoint this provider talks to.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    async fn query<T: serde::de::DeserializeOwned>(
        &self,
        document: &str,
        variables: Value,
    ) -> Result<T> {
        let request = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&json!({ "query": document, "variables": variables }));

        let body = http::send(PROVIDER_NAME, request)
            .await?
            .ok_or_else(|| Error::Api {
                provider: PROVIDER_NAME,
                status: Some(404),
                message: format!("no GraphQL endpoint at {}", self.endpoint),
            })?;

        let parsed: GraphQlResponse<T> = http::decode(PROVIDER_NAME, &body)?;

        if let Some(message) = parsed.error_message() {
            return Err(classify_graphql_error(message));
        }

        parsed.data.ok_or_else(|| Error::UnexpectedResponse {
            provider: PROVIDER_NAME,
            message: "response contained neither `data` nor `errors`".to_owned(),
        })
    }

    async fn search_by_isbn(&self, isbn: &str, limit: usize) -> Result<Vec<BookMetadata>> {
        let data: EditionsData = self
            .query(
                &queries::book_by_isbn(),
                json!({ "isbn": isbn, "limit": u32::try_from(limit).unwrap_or(u32::MAX) }),
            )
            .await?;

        Ok(data
            .editions
            .unwrap_or_default()
            .into_iter()
            .filter_map(convert::from_edition)
            .collect())
    }

    async fn search_by_title(
        &self,
        title: &str,
        author: Option<&str>,
        limit: usize,
    ) -> Result<Vec<BookMetadata>> {
        let title_pattern = ilike_pattern(title).ok_or_else(|| {
            Error::InvalidQuery(format!("`{title}` contains no searchable characters"))
        })?;
        let author_pattern = author.and_then(ilike_pattern);

        let results = self
            .run_books_search(&title_pattern, author_pattern.as_deref(), limit)
            .await?;
        if !results.is_empty() {
            return Ok(results);
        }

        match author.and_then(relaxed_author_pattern) {
            Some(relaxed) if Some(relaxed.as_str()) != author_pattern.as_deref() => {
                self.run_books_search(&title_pattern, Some(&relaxed), limit)
                    .await
            }
            _ => Ok(results),
        }
    }

    async fn run_books_search(
        &self,
        title_pattern: &str,
        author_pattern: Option<&str>,
        limit: usize,
    ) -> Result<Vec<BookMetadata>> {
        let mut filter = json!({ "title": { "_ilike": title_pattern } });
        if let Some(author) = author_pattern {
            filter["contributions"] = json!({ "author": { "name": { "_ilike": author } } });
        }

        let data: BooksData = self
            .query(
                &queries::books_search(),
                json!({
                    "where": filter,
                    "limit": u32::try_from(limit).unwrap_or(u32::MAX),
                    "editionLimit": EDITIONS_PER_BOOK,
                }),
            )
            .await?;

        Ok(data
            .books
            .unwrap_or_default()
            .into_iter()
            .filter_map(convert::from_book)
            .collect())
    }
}

#[async_trait]
impl MetadataProvider for HardcoverProvider {
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

impl std::fmt::Debug for HardcoverProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HardcoverProvider")
            .field("endpoint", &self.endpoint)
            .field("api_key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// Builder for [`HardcoverProvider`].
#[derive(Debug, Clone)]
pub struct HardcoverBuilder {
    api_key: Option<String>,
    endpoint: String,
    timeout: Duration,
    user_agent: String,
    client: Option<reqwest::Client>,
}

impl Default for HardcoverBuilder {
    fn default() -> Self {
        Self {
            api_key: None,
            endpoint: DEFAULT_ENDPOINT.to_owned(),
            timeout: DEFAULT_TIMEOUT,
            user_agent: concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")).to_owned(),
            client: None,
        }
    }
}

impl HardcoverBuilder {
    /// Sets the API key. A leading `Bearer ` is stripped for you.
    #[must_use]
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Overrides the GraphQL endpoint. Mostly useful for tests.
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
    pub fn build(self) -> Result<HardcoverProvider> {
        let api_key = self
            .api_key
            .as_deref()
            .map(normalize_api_key)
            .filter(|key| !key.is_empty())
            .ok_or_else(|| Error::NotConfigured {
                provider: PROVIDER_NAME,
                reason: format!(
                    "an API key is required; get one at https://hardcover.app/account/api \
                     and pass it to `HardcoverProvider::new`, or set `{API_KEY_ENV}`"
                ),
            })?;

        let client = match self.client {
            Some(client) => client,
            None => http::build_client(PROVIDER_NAME, &self.user_agent, self.timeout)?,
        };

        Ok(HardcoverProvider {
            client,
            endpoint: self.endpoint,
            api_key,
        })
    }
}

fn normalize_api_key(raw: &str) -> String {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix("Bearer ")
        .or_else(|| trimmed.strip_prefix("bearer "))
        .unwrap_or(trimmed)
        .trim()
        .to_owned()
}

fn ilike_pattern(term: &str) -> Option<String> {
    pattern_from_tokens(&tokens(term))
}

fn relaxed_author_pattern(term: &str) -> Option<String> {
    let kept: Vec<&str> = tokens(term)
        .into_iter()
        .filter(|token| token.chars().count() > 1)
        .collect();
    pattern_from_tokens(&kept)
}

fn tokens(term: &str) -> Vec<&str> {
    term.split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect()
}

fn pattern_from_tokens(tokens: &[&str]) -> Option<String> {
    if tokens.is_empty() {
        return None;
    }
    Some(format!("%{}%", tokens.join("%")))
}

fn classify_graphql_error(message: String) -> Error {
    let lowered = message.to_ascii_lowercase();
    let looks_like_auth = [
        "access-denied",
        "invalid-jwt",
        "invalid-headers",
        "unauthorized",
    ]
    .iter()
    .any(|marker| lowered.contains(marker));

    if looks_like_auth {
        Error::Unauthorized {
            provider: PROVIDER_NAME,
        }
    } else {
        Error::Api {
            provider: PROVIDER_NAME,
            status: None,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_an_api_key() {
        let error = HardcoverProvider::new("   ").expect_err("a blank key is no key");
        assert!(matches!(error, Error::NotConfigured { .. }));
        assert!(error.to_string().contains("hardcover"));
    }

    #[test]
    fn accepts_a_key_with_or_without_the_bearer_prefix() {
        assert_eq!(normalize_api_key("  Bearer  abc "), "abc");
        assert_eq!(normalize_api_key("abc"), "abc");
    }

    #[test]
    fn never_prints_the_api_key() {
        let provider = HardcoverProvider::new("super-secret").expect("should build");
        let rendered = format!("{provider:?}");
        assert!(!rendered.contains("super-secret"));
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn builds_punctuation_insensitive_patterns() {
        assert_eq!(ilike_pattern(" Dune "), Some("%Dune%".to_owned()));
        assert_eq!(
            ilike_pattern("V. E. Schwab"),
            Some("%V%E%Schwab%".to_owned())
        );
        assert_eq!(ilike_pattern("V.E. Schwab"), ilike_pattern("V. E. Schwab"));
        assert_eq!(
            ilike_pattern("The Hitchhiker's Guide"),
            Some("%The%Hitchhiker%s%Guide%".to_owned())
        );
        assert_eq!(ilike_pattern("100%_pure"), Some("%100%pure%".to_owned()));
        assert_eq!(ilike_pattern("Émile Zola"), Some("%Émile%Zola%".to_owned()));
        assert_eq!(ilike_pattern("..."), None);
    }

    #[test]
    fn relaxing_an_author_drops_initials_only() {
        assert_eq!(
            relaxed_author_pattern("V. E. Schwab"),
            Some("%Schwab%".to_owned())
        );
        assert_eq!(
            relaxed_author_pattern("Ursula K. Le Guin"),
            Some("%Ursula%Le%Guin%".to_owned())
        );
        assert_eq!(
            relaxed_author_pattern("Frank Herbert"),
            ilike_pattern("Frank Herbert")
        );
        assert_eq!(relaxed_author_pattern("J K"), None);
    }

    #[test]
    fn recognises_hasura_permission_errors() {
        let error = classify_graphql_error("field 'books' not found in type: 'query_root'".into());
        assert!(matches!(error, Error::Api { .. }));

        let error = classify_graphql_error("access-denied".into());
        assert!(matches!(error, Error::Unauthorized { .. }));
    }
}

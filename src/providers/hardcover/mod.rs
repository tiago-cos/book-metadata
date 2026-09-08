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

pub const PROVIDER_NAME: &str = convert::SOURCE;

pub const DEFAULT_ENDPOINT: &str = "https://api.hardcover.app/v1/graphql";

pub const API_KEY_ENV: &str = "HARDCOVER_API_KEY";

const EDITIONS_PER_BOOK: u32 = 10;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct HardcoverProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
}

impl HardcoverProvider {
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Self::builder().api_key(api_key).build()
    }

    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var(API_KEY_ENV).map_err(|_| Error::NotConfigured {
            provider: PROVIDER_NAME,
            reason: format!("the `{API_KEY_ENV}` environment variable is not set"),
        })?;
        Self::new(api_key)
    }

    #[must_use]
    pub fn builder() -> HardcoverBuilder {
        HardcoverBuilder::default()
    }

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
    #[must_use]
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    #[must_use]
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    #[must_use]
    pub fn client(mut self, client: reqwest::Client) -> Self {
        self.client = Some(client);
        self
    }

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

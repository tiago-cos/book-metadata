//! [Open Library](https://openlibrary.org) metadata provider.
//!
//! Requires no API key, no registration, and has a catalogue large enough
//! that it usually has *something* for an ISBN nothing else knows. The
//! trade is quality — records are crowd-edited, subjects are a mix of
//! genres and cataloguing notes, and series are free text rather than a
//! structured field.
//!
//! ```no_run
//! # use book_metadata::{providers::openlibrary::OpenLibraryProvider, MetadataProvider, MetadataQuery};
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let provider = OpenLibraryProvider::new()?;
//! let book = provider.fetch(&MetadataQuery::isbn("9780140328721")).await?;
//! println!("{} — {:?}", book.title, book.series);
//! # Ok(())
//! # }
//! ```
//!
//! # How a lookup maps onto the API
//!
//! Open Library splits a book across separate records, so one lookup is
//! several requests. [`fetch`](MetadataProvider::fetch) narrows the search
//! to a single result before enriching it, so the common path costs three
//! or four requests. [`search`](MetadataProvider::search) pays that per
//! candidate; turn the extra requests off with [`OpenLibraryBuilder::detailed`]
//! when a cheap, work-level answer is enough.
//!
//! # Etiquette
//!
//! Open Library is a nonprofit and asks clients to identify themselves. Set a
//! `User-Agent` naming your application and a contact address:
//!
//! ```no_run
//! # use book_metadata::providers::openlibrary::OpenLibraryProvider;
//! # fn run() -> Result<(), book_metadata::Error> {
//! let provider = OpenLibraryProvider::builder()
//!     .user_agent("my-library/1.0 (me@example.com)")
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
use convert::Parts;
use model::{Author, Edition, SearchDoc, SearchResponse, Work};

/// Identifier reported by [`MetadataProvider::name`].
pub const PROVIDER_NAME: &str = convert::SOURCE;

/// Open Library's public API root.
pub const DEFAULT_BASE_URL: &str = "https://openlibrary.org";

/// Where cover images are served from.
pub const DEFAULT_COVERS_URL: &str = "https://covers.openlibrary.org";

const SEARCH_FIELDS: &str = "key,title,subtitle,author_name,first_publish_year,publisher,\
    language,number_of_pages_median,cover_i,cover_edition_key,edition_key,subject";

const MAX_AUTHOR_LOOKUPS: usize = 8;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// A configured Open Library client.
///
/// Needs no credentials. Cloning is cheap and one instance is meant to be
/// reused, so connections are pooled.
#[derive(Debug, Clone)]
pub struct OpenLibraryProvider {
    client: reqwest::Client,
    base_url: String,
    covers_url: String,
    detailed: bool,
}

impl OpenLibraryProvider {
    /// Builds a provider with the default settings.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] when the HTTP client cannot be
    /// constructed.
    pub fn new() -> Result<Self> {
        Self::builder().build()
    }

    /// Starts a builder for a custom user agent, timeout, endpoint or HTTP
    /// client.
    #[must_use]
    pub fn builder() -> OpenLibraryBuilder {
        OpenLibraryBuilder::default()
    }

    /// The API root this provider talks to.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<Option<T>> {
        self.get_with(path, &[]).await
    }

    async fn get_with<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Option<T>> {
        let url = format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        let request = self.client.get(url).query(query);

        http::send(PROVIDER_NAME, request).await?.map_or_else(
            || Ok(None),
            |body| http::decode(PROVIDER_NAME, &body).map(Some),
        )
    }

    async fn lookup_isbn(&self, isbn: &str) -> Result<Vec<BookMetadata>> {
        let Some(edition): Option<Edition> = self.get(&format!("isbn/{isbn}.json")).await? else {
            return Ok(Vec::new());
        };

        let work = self.work_for(&edition).await?;
        let author_names = self.author_names(&edition).await;

        Ok(convert::assemble(
            Parts {
                edition: Some(edition),
                work,
                author_names,
                queried_isbn: Some(isbn.to_owned()),
                ..Parts::default()
            },
            &self.covers_url,
        )
        .into_iter()
        .collect())
    }

    async fn search_title(
        &self,
        title: &str,
        author: Option<&str>,
        limit: usize,
    ) -> Result<Vec<BookMetadata>> {
        let mut params = vec![
            ("title", title.to_owned()),
            ("limit", limit.to_string()),
            ("fields", SEARCH_FIELDS.to_owned()),
        ];
        if let Some(author) = author {
            params.push(("author", author.to_owned()));
        }

        let response: Option<SearchResponse> = self.get_with("search.json", &params).await?;
        let docs = response
            .and_then(|response| response.docs)
            .unwrap_or_default();

        let mut results = Vec::with_capacity(docs.len());
        for doc in docs {
            if let Some(book) = self.assemble_search_hit(doc).await? {
                results.push(book);
            }
        }
        Ok(results)
    }

    async fn assemble_search_hit(&self, doc: SearchDoc) -> Result<Option<BookMetadata>> {
        let (edition, work) = if self.detailed {
            let edition = match doc.representative_edition() {
                Some(olid) => self.get(&format!("books/{olid}.json")).await?,
                None => None,
            };
            let work = match doc.key.as_deref() {
                Some(key) => self.get(&format!("{}.json", key.trim_matches('/'))).await?,
                None => None,
            };
            (edition, work)
        } else {
            (None, None)
        };

        Ok(convert::assemble(
            Parts {
                doc: Some(doc),
                edition,
                work,
                ..Parts::default()
            },
            &self.covers_url,
        ))
    }

    async fn work_for(&self, edition: &Edition) -> Result<Option<Work>> {
        let Some(key) = edition
            .works
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find_map(|key| key.key.as_deref())
        else {
            return Ok(None);
        };

        self.get(&format!("{}.json", key.trim_matches('/'))).await
    }

    async fn author_names(&self, edition: &Edition) -> Vec<String> {
        let mut names = Vec::new();

        for key in edition
            .authors
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|key| key.key.as_deref())
            .take(MAX_AUTHOR_LOOKUPS)
        {
            let author: Result<Option<Author>> =
                self.get(&format!("{}.json", key.trim_matches('/'))).await;
            if let Ok(Some(author)) = author
                && let Some(name) = author.name.or(author.personal_name)
            {
                names.push(name);
            }
        }

        names
    }
}

#[async_trait]
impl MetadataProvider for OpenLibraryProvider {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<BookMetadata>> {
        query.validate()?;

        let mut results = match query.kind() {
            QueryKind::Isbn(isbn) => self.lookup_isbn(isbn).await?,
            QueryKind::TitleAuthor { title, author } => {
                self.search_title(title, author.as_deref(), query.max_results())
                    .await?
            }
        };

        results.truncate(query.max_results());
        Ok(results)
    }

    async fn fetch(&self, query: &MetadataQuery) -> Result<BookMetadata> {
        let query = query.clone().with_max_results(1);
        self.search(&query)
            .await?
            .into_iter()
            .next()
            .ok_or(Error::NotFound)
    }
}

/// Builder for [`OpenLibraryProvider`].
#[derive(Debug, Clone)]
pub struct OpenLibraryBuilder {
    base_url: String,
    covers_url: String,
    timeout: Duration,
    user_agent: String,
    client: Option<reqwest::Client>,
    detailed: bool,
}

impl Default for OpenLibraryBuilder {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            covers_url: DEFAULT_COVERS_URL.to_owned(),
            timeout: DEFAULT_TIMEOUT,
            user_agent: concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")).to_owned(),
            client: None,
            detailed: true,
        }
    }
}

impl OpenLibraryBuilder {
    /// Overrides the API root. Mostly useful for tests.
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Overrides where cover images are served from.
    #[must_use]
    pub fn covers_url(mut self, covers_url: impl Into<String>) -> Self {
        self.covers_url = covers_url.into();
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
    ///
    /// Open Library asks that this name your application and a way to reach
    /// you.
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

    /// Whether a title search follows each hit through to its edition and work
    /// records. On by default.
    ///
    /// Turning it off makes a search exactly one request, at the cost of the
    /// fields Open Library only keeps on those records: description, series,
    /// ISBN and publisher.
    #[must_use]
    pub const fn detailed(mut self, detailed: bool) -> Self {
        self.detailed = detailed;
        self
    }

    /// Builds the provider.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] when the HTTP client cannot be
    /// constructed.
    pub fn build(self) -> Result<OpenLibraryProvider> {
        let client = match self.client {
            Some(client) => client,
            None => http::build_client(PROVIDER_NAME, &self.user_agent, self.timeout)?,
        };

        Ok(OpenLibraryProvider {
            client,
            base_url: self.base_url,
            covers_url: self.covers_url,
            detailed: self.detailed,
        })
    }
}

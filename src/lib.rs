//! A unified, extensible interface for fetching book metadata.
//!
//! Every source is reduced to one type ([`BookMetadata`]) and one trait
//! ([`MetadataProvider`]), so swapping or adding a backend never ripples into
//! the code that consumes the metadata.
//!
//! # Usage
//!
//! ```no_run
//! use book_metadata::{providers::hardcover::HardcoverProvider, MetadataProvider, MetadataQuery};
//!
//! # async fn run() -> Result<(), book_metadata::Error> {
//! let provider = HardcoverProvider::from_env()?;
//!
//! let book = provider.fetch(&MetadataQuery::isbn("978-0-441-01359-3")).await?;
//!
//! let candidates = provider
//!     .search(&MetadataQuery::title("Dune").with_author("Frank Herbert"))
//!     .await?;
//!
//! println!("{} — {:?}", book.title, book.series);
//! # let _ = candidates;
//! # Ok(())
//! # }
//! ```
//!
//! [`fetch`](MetadataProvider::fetch) returns the single best match and fails
//! with [`Error::NotFound`] when there is none; [`search`](MetadataProvider::search)
//! returns every candidate, best first, and an empty list is not an error.
//!
//! # Working with several sources
//!
//! Providers are object safe, so they can be stored together and tried in
//! order of preference:
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use book_metadata::{Error, MetadataProvider, MetadataQuery, BookMetadata};
//! async fn first_hit(
//!     providers: &[Arc<dyn MetadataProvider>],
//!     query: &MetadataQuery,
//! ) -> Result<BookMetadata, Error> {
//!     for provider in providers.iter().filter(|p| p.supports(query)) {
//!         match provider.fetch(query).await {
//!             Err(Error::NotFound) => continue,
//!             other => return other,
//!         }
//!     }
//!     Err(Error::NotFound)
//! }
//! ```
//!
//! # Adding a provider
//!
//! Implement [`MetadataProvider::search`], reading the request back through
//! [`MetadataQuery::as_isbn`] and [`MetadataQuery::as_title_author`], and map
//! the payload onto [`BookMetadata`]. Nothing else in the crate — or in your
//! application — needs to change. See [`providers::hardcover`] for a worked
//! example. Reuse the crate's internal support modules so every source behaves
//! the same way: `genres::normalize` for genre lists, `dates::parse` for the
//! free-text dates catalogues emit, `languages::name` to turn whichever code a
//! source speaks into one name, `series` for position labels, and `http` for
//! turning HTTP failures into the right [`Error`] variant.
//!
//! # Cargo features
//!
//! - `hardcover` *(default)* — the [Hardcover](https://hardcover.app) provider,
//!   the best free source of book metadata. Requires a free API key.
//! - `openlibrary` *(default)* — the [Open Library](https://openlibrary.org)
//!   provider, which needs no credentials and has the larger catalogue,
//!   at the cost of slightly lower quality metadata.
//! - `googlebooks` *(default)* — the [Google Books](https://books.google.com)
//!   provider, the best of the three for covers, and the only one with no
//!   series data at all. Needs a free API key.
//!
//! All three pull in `reqwest`. Disable default features to depend on the core
//! types alone, and enable just the providers you need.

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]

mod error;
mod metadata;
mod provider;
mod query;

macro_rules! support_modules {
    ($($name:ident),* $(,)?) => {
        $(
            #[cfg(feature = "_provider")]
            #[cfg_attr(
                not(all(
                    feature = "hardcover",
                    feature = "openlibrary",
                    feature = "googlebooks"
                )),
                allow(dead_code)
            )]
            mod $name;
        )*
    };
}

support_modules!(dates, genres, http, languages, series);

pub mod providers;

pub use error::{Error, Result};
pub use metadata::{BookContributor, BookMetadata, BookSeries};
pub use provider::MetadataProvider;
pub use query::MetadataQuery;

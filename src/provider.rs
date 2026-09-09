use async_trait::async_trait;

use crate::error::{Error, Result};
use crate::metadata::BookMetadata;
use crate::query::MetadataQuery;

/// The single interface every metadata source implements.
///
/// Adding a source means implementing [`search`](Self::search) and nothing
/// else; callers keep talking to `dyn MetadataProvider` and never learn which
/// backend answered.
///
/// Implementations must be cheap to clone or share (`Arc<dyn MetadataProvider>`
/// is the expected way to pass one around) and must not require any setup
/// beyond their constructor.
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    /// Stable, lowercase identifier of this source, e.g. `"hardcover"`.
    ///
    /// This is what lands in [`BookMetadata::source`].
    fn name(&self) -> &'static str;

    /// Whether this provider can answer this kind of query at all.
    ///
    /// The default says yes to everything. Override it for sources that, say,
    /// only do ISBN lookups, so a caller iterating over providers can skip
    /// them instead of paying for a round trip.
    fn supports(&self, _query: &MetadataQuery) -> bool {
        true
    }

    /// Runs the query and returns matches, best first.
    ///
    /// Read the request with [`MetadataQuery::as_isbn`] and
    /// [`MetadataQuery::as_title_author`] to pick a lookup strategy.
    ///
    /// Returns an empty vector when nothing matched; that is not an error.
    /// The number of records never exceeds
    /// [`MetadataQuery::max_results`].
    async fn search(&self, query: &MetadataQuery) -> Result<Vec<BookMetadata>>;

    /// Runs the query and returns the single best match.
    ///
    /// This is the "give me the metadata for this book" entry point. It fails
    /// with [`Error::NotFound`] when there is no match.
    async fn fetch(&self, query: &MetadataQuery) -> Result<BookMetadata> {
        self.search(query)
            .await?
            .into_iter()
            .next()
            .ok_or(Error::NotFound)
    }
}

#[async_trait]
impl<P: MetadataProvider + ?Sized> MetadataProvider for std::sync::Arc<P> {
    fn name(&self) -> &'static str {
        (**self).name()
    }

    fn supports(&self, query: &MetadataQuery) -> bool {
        (**self).supports(query)
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<BookMetadata>> {
        (**self).search(query).await
    }

    async fn fetch(&self, query: &MetadataQuery) -> Result<BookMetadata> {
        (**self).fetch(query).await
    }
}

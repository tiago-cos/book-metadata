use async_trait::async_trait;

use crate::error::{Error, Result};
use crate::metadata::BookMetadata;
use crate::query::MetadataQuery;

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn name(&self) -> &'static str;

    fn supports(&self, _query: &MetadataQuery) -> bool {
        true
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<BookMetadata>>;

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

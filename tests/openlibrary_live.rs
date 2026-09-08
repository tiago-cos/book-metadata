#![cfg(feature = "openlibrary")]

use std::future::Future;
use std::time::Duration;

use book_meta::providers::openlibrary::{OpenLibraryBuilder, OpenLibraryProvider};
use book_meta::{Error, MetadataProvider, MetadataQuery};
use tokio::sync::Mutex;

static API_LOCK: Mutex<()> = Mutex::const_new(());

const LIVE_TIMEOUT: Duration = Duration::from_secs(60);

const RETRY_DELAY: Duration = Duration::from_secs(3);

fn builder() -> OpenLibraryBuilder {
    OpenLibraryProvider::builder()
        .user_agent("book-metadata-tests/0.1 (https://github.com/tiago-cos/book-metadata)")
        .timeout(LIVE_TIMEOUT)
}

fn provider() -> OpenLibraryProvider {
    builder()
        .build()
        .expect("the provider needs no configuration")
}

async fn attempt<T, F, Fut>(provider: OpenLibraryProvider, operation: F) -> Result<T, Error>
where
    F: Fn(OpenLibraryProvider) -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    let _guard = API_LOCK.lock().await;

    let mut result = operation(provider.clone()).await;
    if let Err(error) = &result
        && error.is_retryable()
    {
        eprintln!("retrying after a transient failure: {error}");
        tokio::time::sleep(RETRY_DELAY).await;
        result = operation(provider).await;
    }
    result
}

#[tokio::test]
#[ignore = "requires network access"]
async fn looks_up_fantastic_mr_fox_by_isbn() {
    let book = attempt(provider(), |provider| async move {
        provider.fetch(&MetadataQuery::isbn("9780140328721")).await
    })
    .await
    .expect("this ISBN should resolve");

    println!("{book:#?}");
    assert!(book.title.to_lowercase().contains("fantastic mr"));
    assert_eq!(book.isbn.as_deref(), Some("9780140328721"));
    assert!(book.authors().any(|name| name.contains("Dahl")));
}

#[tokio::test]
#[ignore = "requires network access"]
async fn searches_by_title_and_author() {
    let results = attempt(provider(), |provider| async move {
        provider
            .search(&MetadataQuery::title_and_author("Dune", "Frank Herbert").with_max_results(3))
            .await
    })
    .await
    .expect("the search should succeed");

    println!("{results:#?}");
    assert!(!results.is_empty(), "expected at least one match");
    assert!(
        results
            .iter()
            .any(|book| book.title.to_lowercase().contains("dune"))
    );
}

#[tokio::test]
#[ignore = "requires network access"]
async fn a_plain_search_still_returns_usable_records() {
    let provider = builder()
        .detailed(false)
        .build()
        .expect("the provider needs no configuration");

    let results = attempt(provider, |provider| async move {
        provider.search(&MetadataQuery::title("The Hobbit")).await
    })
    .await
    .expect("the search should succeed");

    println!("{results:#?}");
    assert!(!results.is_empty());
    assert!(results.iter().any(|book| !book.contributors.is_empty()));
}

#[tokio::test]
#[ignore = "requires network access"]
async fn a_missing_isbn_is_not_found() {
    let error = attempt(provider(), |provider| async move {
        provider.fetch(&MetadataQuery::isbn("9799999999999")).await
    })
    .await
    .expect_err("this ISBN should not exist");

    println!("{error}");
    assert!(matches!(error, Error::NotFound));
}

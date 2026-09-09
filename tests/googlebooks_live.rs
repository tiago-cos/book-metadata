#![cfg(feature = "googlebooks")]

use book_metadata::providers::googlebooks::GoogleBooksProvider;
use book_metadata::{MetadataProvider, MetadataQuery};

fn provider() -> GoogleBooksProvider {
    GoogleBooksProvider::from_env().expect("set GOOGLE_BOOKS_API_KEY to run the live tests")
}

#[tokio::test]
#[ignore = "requires GOOGLE_BOOKS_API_KEY and network access"]
async fn looks_up_dune_by_isbn() {
    let book = provider()
        .fetch(&MetadataQuery::isbn("9780441013593"))
        .await
        .expect("this ISBN should resolve");

    println!("{book:#?}");
    assert!(book.title.to_lowercase().contains("dune"));
    assert_eq!(book.isbn.as_deref(), Some("9780441013593"));
    assert!(book.authors().any(|name| name.contains("Herbert")));
}

#[tokio::test]
#[ignore = "requires GOOGLE_BOOKS_API_KEY and network access"]
async fn searches_by_title_and_author() {
    let results = provider()
        .search(&MetadataQuery::title_and_author("The Hobbit", "Tolkien").with_max_results(5))
        .await
        .expect("the search should succeed");

    println!("{results:#?}");
    assert!(!results.is_empty(), "expected at least one match");
    assert!(
        results
            .iter()
            .any(|book| book.title.to_lowercase().contains("hobbit"))
    );
}

#[tokio::test]
#[ignore = "requires GOOGLE_BOOKS_API_KEY and network access"]
async fn descriptions_come_back_as_plain_text() {
    let book = provider()
        .fetch(&MetadataQuery::isbn("9780441013593"))
        .await
        .expect("this ISBN should resolve");

    let description = book.description.expect("Google should have a description");
    println!("{description}");
    assert!(!description.contains('<'), "markup survived: {description}");
    assert!(
        !description.contains("&amp;"),
        "entities survived: {description}"
    );
    assert!(
        !description.contains("  "),
        "collapsing failed: {description}"
    );
}

#[tokio::test]
#[ignore = "requires GOOGLE_BOOKS_API_KEY and network access"]
async fn a_missing_isbn_is_not_found() {
    let error = provider()
        .fetch(&MetadataQuery::isbn("9799999999999"))
        .await
        .expect_err("this ISBN should not exist");

    println!("{error}");
    assert!(matches!(error, book_metadata::Error::NotFound));
}

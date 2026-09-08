#![cfg(feature = "hardcover")]

use book_meta::providers::hardcover::HardcoverProvider;
use book_meta::{MetadataProvider, MetadataQuery};

fn provider() -> HardcoverProvider {
    HardcoverProvider::from_env().expect("set HARDCOVER_API_KEY to run the live tests")
}

#[tokio::test]
#[ignore = "requires HARDCOVER_API_KEY and network access"]
async fn looks_up_dune_by_isbn() {
    let book = provider()
        .fetch(&MetadataQuery::isbn("9780441013593"))
        .await
        .expect("Dune should be found by ISBN");

    println!("{book:#?}");
    assert!(book.title.to_lowercase().contains("dune"));
    assert_eq!(book.isbn.as_deref(), Some("9780441013593"));
    assert!(book.authors().any(|a| a.contains("Herbert")));
}

#[tokio::test]
#[ignore = "requires HARDCOVER_API_KEY and network access"]
async fn searches_by_title_and_author() {
    let results = provider()
        .search(&MetadataQuery::title_and_author(
            "The Fellowship of the Ring",
            "Tolkien",
        ))
        .await
        .expect("the search should succeed");

    println!("{results:#?}");
    assert!(!results.is_empty(), "expected at least one match");
    assert!(
        results
            .iter()
            .any(|b| b.title.to_lowercase().contains("fellowship"))
    );
}

#[tokio::test]
#[ignore = "requires HARDCOVER_API_KEY and network access"]
async fn resolves_series_position() {
    let book = provider()
        .fetch(&MetadataQuery::title_and_author(
            "A Clash of Kings",
            "Martin",
        ))
        .await
        .expect("the book should be found");

    println!("{:?}", book.series);
    let series = book.series.expect("Hardcover knows this book's series");
    assert!(series.title.to_lowercase().contains("song of ice and fire"));
    assert_eq!(series.number, Some(2.0));
}

#[tokio::test]
#[ignore = "requires HARDCOVER_API_KEY and network access"]
async fn finds_an_author_whose_initials_are_spaced_differently() {
    let provider = provider();

    for author in ["V. E. Schwab", "V.E. Schwab", "Schwab"] {
        let results = provider
            .search(&MetadataQuery::title_and_author("Vicious", author))
            .await
            .unwrap_or_else(|e| panic!("search for `{author}` failed: {e}"));

        println!("{author}: {} result(s)", results.len());
        assert!(
            !results.is_empty(),
            "`{author}` should find the book regardless of spacing"
        );
    }
}

#[tokio::test]
#[ignore = "requires HARDCOVER_API_KEY and network access"]
async fn a_title_search_resolves_a_usable_edition() {
    let book = provider()
        .fetch(&MetadataQuery::title_and_author("Dune", "Frank Herbert"))
        .await
        .expect("the search should succeed");

    println!("{book:#?}");
    assert!(book.title.to_lowercase().contains("dune"));
    assert!(
        book.publisher.is_some() || book.isbn.is_some(),
        "expected the readers' edition to carry publisher or ISBN data"
    );
}

#[tokio::test]
#[ignore = "requires HARDCOVER_API_KEY and network access"]
async fn a_missing_isbn_is_not_found() {
    let error = provider()
        .fetch(&MetadataQuery::isbn("9799999999999"))
        .await
        .expect_err("this ISBN should not exist");

    println!("{error}");
    assert!(matches!(error, book_meta::Error::NotFound));
}

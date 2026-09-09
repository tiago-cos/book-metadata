use std::sync::Arc;

use book_meta::providers::googlebooks::GoogleBooksProvider;
use book_meta::providers::hardcover::HardcoverProvider;
use book_meta::providers::openlibrary::OpenLibraryProvider;
use book_meta::{BookMetadata, MetadataProvider, MetadataQuery};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    let requested = take_flag(&mut args, "--provider");
    let Some(first) = args.first() else {
        eprintln!(
            "usage: search [--provider hardcover|openlibrary|googlebooks] <isbn> | <title> [author]"
        );
        std::process::exit(2);
    };

    let provider = build_provider(requested.as_deref())?;
    let query = build_query(first, args.get(1));

    for (index, book) in provider.search(&query).await?.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print(book);
    }

    Ok(())
}

fn build_provider(
    requested: Option<&str>,
) -> Result<Arc<dyn MetadataProvider>, Box<dyn std::error::Error>> {
    let has_hardcover_key = std::env::var("HARDCOVER_API_KEY").is_ok();
    let name = requested.unwrap_or(if has_hardcover_key {
        "hardcover"
    } else {
        "openlibrary"
    });

    match name {
        "hardcover" => Ok(Arc::new(HardcoverProvider::from_env()?)),
        "openlibrary" => Ok(Arc::new(
            OpenLibraryProvider::builder()
                .user_agent(
                    "book-metadata-example/0.1 (https://github.com/tiago-cos/book-metadata)",
                )
                .build()?,
        )),
        "googlebooks" => Ok(Arc::new(GoogleBooksProvider::from_env()?)),
        other => Err(format!("unknown provider `{other}`").into()),
    }
}

fn take_flag(args: &mut Vec<String>, name: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == name)?;
    let value = args.get(index + 1).cloned();
    args.drain(index..=(index + usize::from(value.is_some())));
    value
}

fn build_query(first: &str, second: Option<&String>) -> MetadataQuery {
    let as_isbn = MetadataQuery::isbn(first);
    if as_isbn.validate().is_ok() {
        return as_isbn;
    }

    let query = MetadataQuery::title(first);
    match second {
        Some(author) => query.with_author(author),
        None => query,
    }
}

fn print(book: &BookMetadata) {
    println!("{}", book.full_title());

    let authors: Vec<&str> = book.authors().collect();
    if !authors.is_empty() {
        println!("  by            {}", authors.join(", "));
    }
    if let Some(series) = &book.series {
        match series.number {
            Some(number) => println!("  series        {} #{number}", series.title),
            None => println!("  series        {}", series.title),
        }
    }
    if let Some(publisher) = &book.publisher {
        println!("  publisher     {publisher}");
    }
    if let Some(date) = book.publication_date {
        println!("  published     {}", date.format("%Y-%m-%d"));
    }
    if let Some(isbn) = &book.isbn {
        println!("  isbn          {isbn}");
    }
    if let Some(pages) = book.page_count {
        println!("  pages         {pages}");
    }
    if let Some(language) = &book.language {
        println!("  language      {language}");
    }
    if !book.genres.is_empty() {
        println!("  genres        {}", book.genres.join(", "));
    }
    if let Some(url) = &book.image_url {
        println!("  cover         {url}");
    }
    if let Some(source) = &book.source {
        println!("  source        {source}");
    }
}

# book-metadata

Look up book metadata from several sources through one small Rust API.

Every provider returns the same struct, so swapping or adding a source never
changes the code that consumes it.

## Providers

| Provider | Credentials | Notes |
| --- | --- | --- |
| [Hardcover](https://hardcover.app) | API key ([get one](https://hardcover.app/account/api)) | Best metadata quality |
| [Open Library](https://openlibrary.org) | none | Huge catalogue size |
| [Google Books](https://books.google.com) | API key ([Google Cloud console](https://console.cloud.google.com/), Books API enabled) | No series data |

## Install

```bash
cargo add book-metadata
```

Or add it to `Cargo.toml` directly:
```toml
[dependencies]
book-metadata = "0.1.0" 
```

All three providers are on by default. To pull in just one:

```toml
book-metadata = { version = "0.1.0", default-features = false, features = ["openlibrary"] }
```

Features are `hardcover`, `openlibrary` and `googlebooks`.

## Usage

```rust
use book_metadata::{providers::hardcover::HardcoverProvider, MetadataProvider, MetadataQuery};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = HardcoverProvider::from_env()?;   // HARDCOVER_API_KEY

    // By ISBN-10 or ISBN-13, hyphens welcome.
    let book = provider.fetch(&MetadataQuery::isbn("978-0-441-01359-3")).await?;
    println!("{} — {:?}", book.full_title(), book.series);

    // Or by title, ideally with an author.
    let candidates = provider
        .search(&MetadataQuery::title("Dune").with_author("Frank Herbert"))
        .await?;
    println!("{} candidates", candidates.len());

    Ok(())
}
```

- `fetch` gives you the single best match, or `Error::NotFound`.
- `search` gives you every candidate, best first. An empty list is not an error.
- `MetadataQuery::title(..).with_max_results(10)` raises the cap (default 5).

### Using another provider

Same trait, same struct — only the constructor differs:

```rust
use book_metadata::providers::googlebooks::GoogleBooksProvider;
use book_metadata::providers::openlibrary::OpenLibraryProvider;

let google = GoogleBooksProvider::from_env()?;   // GOOGLE_BOOKS_API_KEY
let open_library = OpenLibraryProvider::new()?;  // no credentials
```

Providers are object safe, so you can try them in order and take the first hit:

```rust
use std::sync::Arc;
use book_metadata::{BookMetadata, Error, MetadataProvider, MetadataQuery};

async fn first_hit(
    providers: &[Arc<dyn MetadataProvider>],
    query: &MetadataQuery,
) -> Result<BookMetadata, Error> {
    for provider in providers {
        match provider.fetch(query).await {
            Err(Error::NotFound) => continue,
            other => return other,
        }
    }
    Err(Error::NotFound)
}
```

## What you get back

Only `title` is guaranteed; coverage varies by source and by book.

```rust
pub struct BookMetadata {
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    /// First edition's date when known, else the matched edition's.
    pub publication_date: Option<DateTime<Utc>>,
    /// ISBN-13 preferred over ISBN-10.
    pub isbn: Option<String>,
    pub contributors: Vec<BookContributor>,  // { name, role }
    pub genres: Vec<String>,
    pub series: Option<BookSeries>,          // { title, number: Option<f32> }
    pub page_count: Option<i64>,
    /// Human readable, e.g. "English".
    pub language: Option<String>,
    pub image_url: Option<String>,
    /// Which provider answered, and its own id for the book.
    pub source: Option<String>,
    pub source_id: Option<String>,
}
```

Errors are provider-agnostic: `NotConfigured`, `InvalidQuery`, `NotFound`,
`Unauthorized`, `RateLimited`, `Transport`, `Api`, `UnexpectedResponse`.
`Error::is_retryable()` covers the transient ones.

## Try it

```sh
export HARDCOVER_API_KEY=...
cargo run --example search -- 9780441013593
cargo run --example search -- "Dune" "Frank Herbert"
cargo run --example search -- --provider openlibrary "Dune"
```

## Development

```sh
cargo test
cargo clippy --all-targets --all-features
cargo fmt
```

Tests run entirely offline against a local mock server. Live smoke tests against
the real APIs are ignored by default:

```sh
HARDCOVER_API_KEY=...     cargo test --test hardcover_live    -- --ignored
GOOGLE_BOOKS_API_KEY=...  cargo test --test googlebooks_live  -- --ignored
                          cargo test --test openlibrary_live  -- --ignored
```

## License

Licensed under the [MIT license](LICENSE).

#![cfg(feature = "openlibrary")]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc;
use std::thread;

use book_metadata::providers::openlibrary::OpenLibraryProvider;
use book_metadata::{Error, MetadataProvider, MetadataQuery};

struct MockServer {
    addr: SocketAddr,
    requests: mpsc::Receiver<String>,
}

impl MockServer {
    fn spawn(routes: Vec<(&'static str, &'static str)>) -> Self {
        let routes: HashMap<&str, &str> = routes.into_iter().collect();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let mut reader = BufReader::new(stream);

                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    break;
                }
                let target = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned();

                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    if let Some(value) = line
                        .to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(str::trim)
                    {
                        content_length = value.parse().unwrap_or(0);
                    }
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                }
                let mut body = vec![0u8; content_length];
                let _ = reader.read_exact(&mut body);

                if tx.send(target.clone()).is_err() {
                    break;
                }

                let path = target.split('?').next().unwrap_or_default();
                let (status, payload) = routes
                    .get(path)
                    .map_or(("404 Not Found", r#"{"error":"notfound"}"#), |body| {
                        ("200 OK", *body)
                    });

                let response = format!(
                    "HTTP/1.1 {status}\r\n\
                     Content-Type: application/json\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let mut stream = reader.into_inner();
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        Self { addr, requests: rx }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn requested(&self) -> Vec<String> {
        self.requests.try_iter().collect()
    }
}

fn provider(server: &MockServer, detailed: bool) -> OpenLibraryProvider {
    OpenLibraryProvider::builder()
        .base_url(server.base_url())
        .covers_url("https://covers.openlibrary.test")
        .detailed(detailed)
        .build()
        .expect("builder should succeed")
}

const EDITION: &str = r#"{
  "title": "Fantastic Mr Fox",
  "publishers": ["Puffin"],
  "publish_date": "September 1988",
  "number_of_pages": 96,
  "isbn_13": ["9780140328721"],
  "covers": [8739161],
  "languages": [{ "key": "/languages/eng" }],
  "series": ["Puffin Books ; 3"],
  "works": [{ "key": "/works/OL45804W" }],
  "authors": [{ "key": "/authors/OL34184A" }]
}"#;

const WORK: &str = r#"{
  "title": "Fantastic Mr Fox",
  "description": "A fox outwits three farmers.",
  "subjects": ["Children's stories", "Foxes -- Fiction"],
  "first_publish_date": "1970"
}"#;

const AUTHOR: &str = r#"{ "name": "Roald Dahl" }"#;

const SEARCH: &str = r#"{
  "numFound": 1,
  "docs": [{
    "key": "/works/OL45804W",
    "title": "Fantastic Mr Fox",
    "author_name": ["Roald Dahl"],
    "first_publish_year": 1970,
    "language": ["eng"],
    "number_of_pages_median": 96,
    "cover_i": 8739161,
    "cover_edition_key": "OL7353617M",
    "subject": ["Children's stories"]
  }]
}"#;

#[tokio::test]
async fn an_isbn_lookup_walks_edition_work_and_author() {
    let server = MockServer::spawn(vec![
        ("/isbn/9780140328721.json", EDITION),
        ("/works/OL45804W.json", WORK),
        ("/authors/OL34184A.json", AUTHOR),
    ]);
    let provider = provider(&server, true);

    let book = provider
        .fetch(&MetadataQuery::isbn("978-0-14-032872-1"))
        .await
        .expect("lookup should succeed");

    assert_eq!(
        server.requested(),
        vec![
            "/isbn/9780140328721.json",
            "/works/OL45804W.json",
            "/authors/OL34184A.json"
        ]
    );

    assert_eq!(book.title, "Fantastic Mr Fox");
    assert_eq!(
        book.description.as_deref(),
        Some("A fox outwits three farmers.")
    );
    assert_eq!(book.publisher.as_deref(), Some("Puffin"));
    assert_eq!(book.isbn.as_deref(), Some("9780140328721"));
    assert_eq!(book.page_count, Some(96));
    assert_eq!(book.language.as_deref(), Some("English"));
    assert_eq!(book.authors().collect::<Vec<_>>(), vec!["Roald Dahl"]);
    assert_eq!(book.genres, vec!["Children's stories", "Foxes", "Fiction"]);
    let series = book.series.expect("the edition names a series");
    assert_eq!(series.title, "Puffin Books");
    assert_eq!(series.number, Some(3.0));
    assert_eq!(
        book.image_url.as_deref(),
        Some("https://covers.openlibrary.test/b/id/8739161-L.jpg")
    );
    assert_eq!(book.source.as_deref(), Some("openlibrary"));
}

#[tokio::test]
async fn an_unknown_isbn_is_not_found() {
    let server = MockServer::spawn(vec![]);
    let provider = provider(&server, true);

    let error = provider
        .fetch(&MetadataQuery::isbn("9799999999999"))
        .await
        .expect_err("nothing should match");

    assert!(matches!(error, Error::NotFound));
    assert_eq!(server.requested(), vec!["/isbn/9799999999999.json"]);
}

#[tokio::test]
async fn a_title_search_enriches_each_hit() {
    let server = MockServer::spawn(vec![
        ("/search.json", SEARCH),
        ("/books/OL7353617M.json", EDITION),
        ("/works/OL45804W.json", WORK),
    ]);
    let provider = provider(&server, true);

    let results = provider
        .search(&MetadataQuery::title_and_author("Fantastic Mr Fox", "Dahl"))
        .await
        .expect("search should succeed");

    let requested = server.requested();
    let search = &requested[0];
    assert!(
        search.contains("title=Fantastic+Mr+Fox") || search.contains("title=Fantastic%20Mr%20Fox")
    );
    assert!(search.contains("author=Dahl"));
    assert!(search.contains("fields="));
    assert_eq!(
        &requested[1..],
        ["/books/OL7353617M.json", "/works/OL45804W.json"]
    );

    assert_eq!(results.len(), 1);
    let book = &results[0];
    assert_eq!(book.isbn.as_deref(), Some("9780140328721"));
    assert_eq!(
        book.description.as_deref(),
        Some("A fox outwits three farmers.")
    );
    assert_eq!(
        book.series.as_ref().map(|s| s.title.as_str()),
        Some("Puffin Books")
    );
    assert_eq!(book.authors().collect::<Vec<_>>(), vec!["Roald Dahl"]);
}

#[tokio::test]
async fn a_plain_search_is_a_single_request() {
    let server = MockServer::spawn(vec![("/search.json", SEARCH)]);
    let provider = provider(&server, false);

    let results = provider
        .search(&MetadataQuery::title("Fantastic Mr Fox"))
        .await
        .expect("search should succeed");

    assert_eq!(server.requested().len(), 1);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Fantastic Mr Fox");
    assert_eq!(results[0].isbn, None);
    assert_eq!(results[0].series, None);
    assert_eq!(results[0].description, None);
}

#[tokio::test]
async fn fetch_narrows_the_search_before_enriching() {
    let server = MockServer::spawn(vec![("/search.json", SEARCH)]);
    let provider = provider(&server, false);

    provider
        .fetch(&MetadataQuery::title("Fantastic Mr Fox").with_max_results(25))
        .await
        .expect("fetch should succeed");

    let requested = server.requested();
    assert_eq!(requested.len(), 1);
    assert!(
        requested[0].contains("limit=1"),
        "fetch should ask for one result, got {}",
        requested[0]
    );
}

#[tokio::test]
async fn an_unresolvable_author_does_not_sink_the_lookup() {
    let server = MockServer::spawn(vec![
        ("/isbn/9780140328721.json", EDITION),
        ("/works/OL45804W.json", WORK),
    ]);
    let provider = provider(&server, true);

    let book = provider
        .fetch(&MetadataQuery::isbn("9780140328721"))
        .await
        .expect("the book should still come back");

    assert_eq!(book.title, "Fantastic Mr Fox");
    assert!(book.contributors.is_empty());
}

#[tokio::test]
async fn a_timeout_says_so() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let _stalled = thread::spawn(move || {
        let held: Vec<_> = listener.incoming().take(1).filter_map(Result::ok).collect();
        thread::sleep(std::time::Duration::from_secs(30));
        drop(held);
    });

    let provider = OpenLibraryProvider::builder()
        .base_url(format!("http://{addr}"))
        .timeout(std::time::Duration::from_millis(250))
        .build()
        .expect("builder should succeed");

    let error = provider
        .search(&MetadataQuery::isbn("9780140328721"))
        .await
        .expect_err("the server never answers");

    match &error {
        Error::Transport { message, .. } => {
            assert!(
                message.contains("timed out"),
                "a timeout should say so, got: {message}"
            );
        }
        other => panic!("expected Error::Transport, got {other:?}"),
    }
    assert!(error.is_retryable());
}

#[tokio::test]
async fn a_bad_isbn_never_reaches_the_network() {
    let provider = OpenLibraryProvider::builder()
        .base_url("http://127.0.0.1:1")
        .build()
        .expect("builder should succeed");

    let error = provider
        .search(&MetadataQuery::isbn("not-an-isbn"))
        .await
        .expect_err("the query is invalid");

    assert!(matches!(error, Error::InvalidQuery(_)));
}

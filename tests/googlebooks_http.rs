#![cfg(feature = "googlebooks")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc;
use std::thread;

use book_metadata::providers::googlebooks::GoogleBooksProvider;
use book_metadata::{Error, MetadataProvider, MetadataQuery};

struct MockServer {
    addr: SocketAddr,
    targets: mpsc::Receiver<String>,
}

impl MockServer {
    fn spawn(status_line: &'static str, body: &'static str) -> Self {
        Self::spawn_many(vec![(status_line, body)])
    }

    fn spawn_many(responses: Vec<(&'static str, &'static str)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            for (status_line, body) in responses {
                let Ok((stream, _)) = listener.accept() else {
                    break;
                };
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
                let mut ignored = vec![0u8; content_length];
                let _ = reader.read_exact(&mut ignored);

                if tx.send(target).is_err() {
                    break;
                }

                let response = format!(
                    "HTTP/1.1 {status_line}\r\n\
                     Content-Type: application/json\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\r\n{body}",
                    body.len()
                );
                let mut stream = reader.into_inner();
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        Self { addr, targets: rx }
    }

    fn endpoint(&self) -> String {
        format!("http://{}/books/v1/volumes", self.addr)
    }

    fn requested(&self) -> String {
        let target = self.targets.recv().expect("a request should arrive");
        target
            .replace('+', " ")
            .replace("%22", "\"")
            .replace("%3A", ":")
    }

    fn assert_no_more_requests(&self) {
        assert!(
            self.targets.try_recv().is_err(),
            "the provider sent more requests than expected"
        );
    }
}

fn provider(server: &MockServer) -> GoogleBooksProvider {
    GoogleBooksProvider::builder()
        .endpoint(server.endpoint())
        .api_key("test-key")
        .build()
        .expect("builder should succeed with a key")
}

const DUNE: &str = r#"{
  "totalItems": 1,
  "items": [{
    "id": "B1hSG45JCX4C",
    "volumeInfo": {
      "title": "Dune",
      "authors": ["Frank Herbert"],
      "publisher": "Ace Books",
      "publishedDate": "2005-06",
      "description": "<p>Set on <i>Arrakis</i>.</p>",
      "industryIdentifiers": [{ "type": "ISBN_13", "identifier": "9780441013593" }],
      "pageCount": 604,
      "categories": ["Fiction / Science Fiction / General"],
      "language": "en",
      "imageLinks": {
        "thumbnail": "http://books.google.com/books/content?id=X&img=1&edge=curl"
      }
    }
  }]
}"#;

#[tokio::test]
async fn an_isbn_lookup_is_a_single_request() {
    let server = MockServer::spawn("200 OK", DUNE);
    let provider = provider(&server);

    let book = provider
        .fetch(&MetadataQuery::isbn("978-0-441-01359-3"))
        .await
        .expect("lookup should succeed");

    let target = server.requested();
    assert!(target.contains("q=isbn:9780441013593"), "got {target}");
    assert!(target.contains("printType=books"), "got {target}");
    assert!(target.contains("key=test-key"), "got {target}");
    server.assert_no_more_requests();

    assert_eq!(book.title, "Dune");
    assert_eq!(book.isbn.as_deref(), Some("9780441013593"));
    assert_eq!(book.publisher.as_deref(), Some("Ace Books"));
    assert_eq!(book.page_count, Some(604));
    assert_eq!(book.language.as_deref(), Some("English"));
    assert_eq!(book.genres, vec!["Fiction", "Science Fiction"]);
    assert_eq!(book.description.as_deref(), Some("Set on Arrakis ."));
    let cover = book.image_url.expect("a cover");
    assert!(cover.starts_with("https://"), "got {cover}");
    assert!(!cover.contains("edge=curl"), "got {cover}");
}

#[tokio::test]
async fn a_title_search_uses_googles_field_syntax() {
    let server = MockServer::spawn("200 OK", DUNE);
    let provider = provider(&server);

    provider
        .search(&MetadataQuery::title_and_author("Dune", "Frank Herbert").with_max_results(7))
        .await
        .expect("search should succeed");

    let target = server.requested();
    assert!(target.contains("q=intitle:\"Dune\""), "got {target}");
    assert!(
        target.contains("inauthor:\"Frank Herbert\""),
        "got {target}"
    );
    assert!(target.contains("maxResults=7"), "got {target}");
}

#[tokio::test]
async fn caps_max_results_at_googles_limit() {
    let server = MockServer::spawn("200 OK", DUNE);
    let provider = provider(&server);

    provider
        .search(&MetadataQuery::title("Dune").with_max_results(500))
        .await
        .expect("search should succeed");

    let target = server.requested();
    assert!(target.contains("maxResults=40"), "got {target}");
}

#[tokio::test]
async fn sends_the_country_when_configured() {
    let server = MockServer::spawn("200 OK", DUNE);
    let provider = GoogleBooksProvider::builder()
        .endpoint(server.endpoint())
        .api_key("test-key")
        .country("PT")
        .build()
        .expect("should build");

    provider
        .search(&MetadataQuery::title("Dune"))
        .await
        .expect("search should succeed");

    let target = server.requested();
    assert!(target.contains("country=PT"), "got {target}");
}

#[tokio::test]
async fn an_unauthenticated_refusal_is_a_configuration_problem() {
    let body = r#"{"error":{"code":403,
        "message":"Daily Limit for Unauthenticated Use Exceeded. Continued use requires signup.",
        "errors":[{"reason":"dailyLimitExceededUnreg"}]}}"#;
    let server = MockServer::spawn("403 Forbidden", body);
    let provider = provider(&server);

    let error = provider
        .search(&MetadataQuery::title("Dune"))
        .await
        .expect_err("Google refused the request");

    assert!(
        matches!(error, Error::NotConfigured { .. }),
        "got {error:?}; this is not a quota that clears"
    );
    assert!(!error.is_retryable(), "retrying will never help");
}

#[tokio::test]
async fn prefers_a_volume_that_actually_carries_the_isbn() {
    let body = r#"{"items":[
        { "volumeInfo": { "title": "A Study Guide", "industryIdentifiers":
            [{ "type": "ISBN_13", "identifier": "9999999999999" }] } },
        { "volumeInfo": { "title": "Dune", "industryIdentifiers":
            [{ "type": "ISBN_13", "identifier": "9780441013593" }] } }
    ]}"#;
    let server = MockServer::spawn("200 OK", body);
    let provider = provider(&server);

    let book = provider
        .fetch(&MetadataQuery::isbn("9780441013593"))
        .await
        .expect("lookup should succeed");

    assert_eq!(book.title, "Dune");
}

#[tokio::test]
async fn keeps_loose_matches_when_nothing_matches_exactly() {
    let body = r#"{"items":[{ "volumeInfo": { "title": "Dune" } }]}"#;
    let server = MockServer::spawn("200 OK", body);
    let provider = provider(&server);

    let book = provider
        .fetch(&MetadataQuery::isbn("9780441013593"))
        .await
        .expect("lookup should succeed");

    assert_eq!(book.title, "Dune");
    assert_eq!(book.isbn.as_deref(), Some("9780441013593"));
}

#[tokio::test]
async fn no_items_is_not_found_rather_than_an_error() {
    let server = MockServer::spawn("200 OK", r#"{"totalItems":0}"#);
    let provider = provider(&server);

    let error = provider
        .fetch(&MetadataQuery::isbn("9799999999999"))
        .await
        .expect_err("nothing matched");

    assert!(matches!(error, Error::NotFound));
}

#[tokio::test]
async fn an_exhausted_quota_is_a_rate_limit_not_a_credentials_failure() {
    let body = r#"{"error":{"code":403,"message":"Daily Limit Exceeded",
        "errors":[{"reason":"dailyLimitExceeded"}]}}"#;
    let server = MockServer::spawn("403 Forbidden", body);
    let provider = provider(&server);

    let error = provider
        .search(&MetadataQuery::title("Dune"))
        .await
        .expect_err("the quota is gone");

    assert!(
        matches!(error, Error::RateLimited { .. }),
        "got {error:?}; a 403 here is not about credentials"
    );
    assert!(error.is_retryable());
}

#[tokio::test]
async fn an_unsupported_region_reports_what_google_said() {
    let body = r#"{"error":{"code":403,
        "message":"It looks like you're making a request from a region that is not supported",
        "errors":[{"reason":"forbidden"}]}}"#;
    let server = MockServer::spawn("403 Forbidden", body);
    let provider = provider(&server);

    let error = provider
        .search(&MetadataQuery::title("Dune"))
        .await
        .expect_err("the region is refused");

    match &error {
        Error::Api { message, .. } => assert!(message.contains("region"), "got {message}"),
        other => panic!("expected Error::Api naming the region, got {other:?}"),
    }
}

#[tokio::test]
async fn a_bad_isbn_never_reaches_the_network() {
    let provider = GoogleBooksProvider::builder()
        .endpoint("http://127.0.0.1:1/books/v1/volumes")
        .api_key("test-key")
        .build()
        .expect("should build");

    let error = provider
        .search(&MetadataQuery::isbn("not-an-isbn"))
        .await
        .expect_err("the query is invalid");

    assert!(matches!(error, Error::InvalidQuery(_)));
}

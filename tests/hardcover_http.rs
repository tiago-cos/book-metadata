#![cfg(feature = "hardcover")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc;
use std::thread;

use book_metadata::providers::hardcover::HardcoverProvider;
use book_metadata::{Error, MetadataProvider, MetadataQuery};

struct MockServer {
    addr: SocketAddr,
    requests: mpsc::Receiver<RecordedRequest>,
}

struct RecordedRequest {
    headers: String,
    body: String,
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
                let (stream, _) = listener.accept().expect("accept");
                let mut reader = BufReader::new(stream);

                let mut headers = String::new();
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).expect("read header") == 0 {
                        break;
                    }
                    if let Some(value) = line
                        .to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(str::trim)
                    {
                        content_length = value.parse().unwrap_or(0);
                    }
                    let done = line == "\r\n" || line == "\n";
                    headers.push_str(&line);
                    if done {
                        break;
                    }
                }

                let mut raw_body = vec![0u8; content_length];
                reader.read_exact(&mut raw_body).expect("read body");

                if tx
                    .send(RecordedRequest {
                        headers,
                        body: String::from_utf8_lossy(&raw_body).into_owned(),
                    })
                    .is_err()
                {
                    return;
                }

                let mut stream = reader.into_inner();
                let response = format!(
                    "HTTP/1.1 {status_line}\r\n\
                     Content-Type: application/json\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write response");
                let _ = stream.flush();
            }
        });

        Self { addr, requests: rx }
    }

    fn endpoint(&self) -> String {
        format!("http://{}/v1/graphql", self.addr)
    }

    fn recorded(&self) -> RecordedRequest {
        self.requests
            .recv()
            .expect("server should record a request")
    }

    fn recorded_variables(&self) -> serde_json::Value {
        let request = self.recorded();
        let sent: serde_json::Value = serde_json::from_str(&request.body).expect("valid JSON body");
        sent["variables"].clone()
    }

    fn assert_no_more_requests(&self) {
        assert!(
            self.requests.try_recv().is_err(),
            "the provider sent more requests than expected"
        );
    }
}

fn provider(endpoint: String) -> HardcoverProvider {
    HardcoverProvider::builder()
        .api_key("Bearer test-token")
        .endpoint(endpoint)
        .build()
        .expect("builder should succeed with a key")
}

const ISBN_RESPONSE: &str = r##"{
  "data": {
    "editions": [
      {
        "isbn_13": "9780441013593",
        "pages": 704,
        "release_date": "2005-06-01",
        "edition_format": "Paperback",
        "publisher": { "name": "Ace" },
        "language": { "language": "English", "code3": "eng" },
        "image": { "url": "https://example.test/dune.jpg" },
        "book": {
          "id": 12345,
          "title": "Dune",
          "description": "Paul Atreides goes to Arrakis.",
          "release_date": "1965-08-01",
          "cached_tags": { "Genre": [{ "tag": "Science Fiction", "count": 500 }] },
          "contributions": [{ "contribution": null, "author": { "name": "Frank Herbert" } }],
          "book_series": [
            { "position": 1, "details": "#1", "series": { "name": "Dune" } }
          ]
        }
      }
    ]
  }
}"##;

#[tokio::test]
async fn isbn_lookup_sends_credentials_and_maps_the_response() {
    let server = MockServer::spawn("200 OK", ISBN_RESPONSE);
    let provider = provider(server.endpoint());

    let book = provider
        .fetch(&MetadataQuery::isbn("978-0-441-01359-3"))
        .await
        .expect("lookup should succeed");

    let request = server.recorded();
    let headers = request.headers.to_ascii_lowercase();
    assert!(
        headers.contains("authorization: bearer test-token"),
        "the bearer token should be sent, got:\n{}",
        request.headers
    );

    let sent: serde_json::Value = serde_json::from_str(&request.body).expect("valid JSON body");
    assert!(
        sent["query"]
            .as_str()
            .expect("a query document")
            .contains("query BookByIsbn")
    );
    assert_eq!(sent["variables"]["isbn"], "9780441013593");
    server.assert_no_more_requests();

    assert_eq!(book.title, "Dune");
    assert_eq!(book.isbn.as_deref(), Some("9780441013593"));
    assert_eq!(book.publisher.as_deref(), Some("Ace"));
    assert_eq!(book.page_count, Some(704));
    assert_eq!(book.language.as_deref(), Some("English"));
    assert_eq!(
        book.publication_date
            .map(|d| d.format("%Y-%m-%d").to_string()),
        Some("1965-08-01".to_owned())
    );
    assert_eq!(book.genres, vec!["Science Fiction"]);
    assert_eq!(book.authors().collect::<Vec<_>>(), vec!["Frank Herbert"]);
    let series = book.series.expect("Dune is part of a series");
    assert_eq!(series.title, "Dune");
    assert_eq!(series.number, Some(1.0));
    assert_eq!(book.source.as_deref(), Some("hardcover"));
    assert_eq!(book.source_id.as_deref(), Some("12345"));
}

#[tokio::test]
async fn title_search_filters_by_author() {
    let body = r#"{"data":{"books":[{"title":"Dune","editions":[]}]}}"#;
    let server = MockServer::spawn("200 OK", body);
    let provider = provider(server.endpoint());

    let results = provider
        .search(&MetadataQuery::title_and_author("Dune", "Frank Herbert").with_max_results(3))
        .await
        .expect("search should succeed");

    let request = server.recorded();
    let sent: serde_json::Value = serde_json::from_str(&request.body).expect("valid JSON body");
    assert!(
        sent["query"]
            .as_str()
            .expect("a query document")
            .contains("query BooksSearch")
    );
    assert_eq!(sent["variables"]["where"]["title"]["_ilike"], "%Dune%");
    assert_eq!(
        sent["variables"]["where"]["contributions"]["author"]["name"]["_ilike"],
        "%Frank%Herbert%"
    );
    assert_eq!(sent["variables"]["limit"], 3);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Dune");
    server.assert_no_more_requests();
}

#[tokio::test]
async fn retries_with_initials_dropped_when_the_strict_author_search_misses() {
    let hit = r#"{"data":{"books":[{"title":"Vicious","editions":[]}]}}"#;
    let server = MockServer::spawn_many(vec![
        ("200 OK", r#"{"data":{"books":[]}}"#),
        ("200 OK", hit),
    ]);
    let provider = provider(server.endpoint());

    let results = provider
        .search(&MetadataQuery::title_and_author("Vicious", "V. E. Schwab"))
        .await
        .expect("search should succeed");

    let strict = server.recorded_variables();
    assert_eq!(
        strict["where"]["contributions"]["author"]["name"]["_ilike"],
        "%V%E%Schwab%"
    );

    let relaxed = server.recorded_variables();
    assert_eq!(
        relaxed["where"]["contributions"]["author"]["name"]["_ilike"],
        "%Schwab%"
    );
    assert_eq!(relaxed["where"]["title"]["_ilike"], "%Vicious%");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Vicious");
    server.assert_no_more_requests();
}

#[tokio::test]
async fn ranks_editions_by_readership() {
    let server = MockServer::spawn("200 OK", r#"{"data":{"books":[]}}"#);
    let provider = provider(server.endpoint());

    provider
        .search(&MetadataQuery::title("Dune"))
        .await
        .expect("search should succeed");

    let request = server.recorded();
    let sent: serde_json::Value = serde_json::from_str(&request.body).expect("valid JSON body");
    let document = sent["query"].as_str().expect("a query document");

    let editions = document
        .split("editions(")
        .nth(1)
        .expect("the document should nest editions");
    assert!(
        editions.contains("users_count: desc_nulls_last"),
        "editions should be ordered by readership, got:\n{document}"
    );
    assert!(
        editions.contains("users_count\n"),
        "the count should be selected so it can rank ties, got:\n{document}"
    );
}

#[tokio::test]
async fn does_not_retry_when_there_are_no_initials_to_drop() {
    let server = MockServer::spawn("200 OK", r#"{"data":{"books":[]}}"#);
    let provider = provider(server.endpoint());

    let results = provider
        .search(&MetadataQuery::title_and_author("Dune", "Frank Herbert"))
        .await
        .expect("search should succeed");

    assert!(results.is_empty());
    let _ = server.recorded();
    server.assert_no_more_requests();
}

#[tokio::test]
async fn punctuation_in_a_title_becomes_a_wildcard() {
    let server = MockServer::spawn("200 OK", r#"{"data":{"books":[]}}"#);
    let provider = provider(server.endpoint());

    provider
        .search(&MetadataQuery::title("The Hitchhiker's Guide"))
        .await
        .expect("search should succeed");

    let variables = server.recorded_variables();
    assert_eq!(
        variables["where"]["title"]["_ilike"],
        "%The%Hitchhiker%s%Guide%"
    );
    assert!(variables["where"]["contributions"].is_null());
}

#[tokio::test]
async fn a_title_of_pure_punctuation_never_reaches_the_network() {
    let provider = provider("http://127.0.0.1:1/v1/graphql".to_owned());

    let error = provider
        .search(&MetadataQuery::title("..."))
        .await
        .expect_err("the query is invalid");

    assert!(matches!(error, Error::InvalidQuery(_)));
}

#[tokio::test]
async fn empty_results_are_not_an_error_but_fetch_reports_not_found() {
    let server = MockServer::spawn("200 OK", r#"{"data":{"editions":[]}}"#);
    let provider = provider(server.endpoint());

    let error = provider
        .fetch(&MetadataQuery::isbn("9780441013593"))
        .await
        .expect_err("nothing matched");

    assert!(matches!(error, Error::NotFound));
}

#[tokio::test]
async fn graphql_errors_surface_verbatim() {
    let body = r#"{"errors":[{"message":"field 'nope' not found in type: 'books'"}]}"#;
    let server = MockServer::spawn("200 OK", body);
    let provider = provider(server.endpoint());

    let error = provider
        .search(&MetadataQuery::title("Dune"))
        .await
        .expect_err("the schema rejected the query");

    match error {
        Error::Api { message, .. } => assert!(message.contains("field 'nope' not found")),
        other => panic!("expected Error::Api, got {other:?}"),
    }
}

#[tokio::test]
async fn permission_errors_map_to_unauthorized() {
    let body = r#"{"errors":[{"message":"access-denied: missing session variable"}]}"#;
    let server = MockServer::spawn("200 OK", body);
    let provider = provider(server.endpoint());

    let error = provider
        .search(&MetadataQuery::title("Dune"))
        .await
        .expect_err("credentials rejected");

    assert!(matches!(error, Error::Unauthorized { .. }));
}

#[tokio::test]
async fn http_401_maps_to_unauthorized() {
    let server = MockServer::spawn("401 Unauthorized", r#"{"message":"nope"}"#);
    let provider = provider(server.endpoint());

    let error = provider
        .search(&MetadataQuery::title("Dune"))
        .await
        .expect_err("credentials rejected");

    assert!(matches!(error, Error::Unauthorized { .. }));
}

#[tokio::test]
async fn a_bad_isbn_never_reaches_the_network() {
    let provider = provider("http://127.0.0.1:1/v1/graphql".to_owned());

    let error = provider
        .search(&MetadataQuery::isbn("not-an-isbn"))
        .await
        .expect_err("the query is invalid");

    assert!(matches!(error, Error::InvalidQuery(_)));
}

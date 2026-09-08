use serde_json::Value;

use super::model::{Book, BookSeriesRow, Edition};
use crate::metadata::{BookContributor, BookMetadata, BookSeries};
use crate::{dates, genres, series as series_parser};

pub const SOURCE: &str = "hardcover";

pub fn from_edition(mut edition: Edition) -> Option<BookMetadata> {
    let book = edition.book.take().map(|b| *b).unwrap_or_default();
    assemble(book, Some(edition))
}

pub fn from_book(mut book: Book) -> Option<BookMetadata> {
    let edition = book.editions.take().and_then(pick_best_edition);
    assemble(book, edition)
}

fn assemble(mut book: Book, edition: Option<Edition>) -> Option<BookMetadata> {
    let edition = edition.unwrap_or_default();

    let title = clean(book.title.take()).or_else(|| clean(edition.title))?;

    let contributors = contributors(&book);
    let genres = genres(book.cached_tags.as_ref());
    let series = series(book.series.as_deref().unwrap_or_default());
    let source_id = book.id.as_ref().and_then(id_to_string);

    let publication_date = book
        .release_date
        .as_deref()
        .and_then(dates::parse)
        .or_else(|| edition.release_date.as_deref().and_then(dates::parse));

    let language = clean(edition.language.as_ref().and_then(|l| l.language.clone()))
        .or_else(|| clean(edition.language.as_ref().and_then(|l| l.code3.clone())));

    Some(BookMetadata {
        title,
        subtitle: clean(book.subtitle).or_else(|| clean(edition.subtitle)),
        description: clean(book.description),
        publisher: clean(edition.publisher.and_then(|p| p.name)),
        publication_date,
        isbn: clean(edition.isbn_13).or_else(|| clean(edition.isbn_10)),
        contributors,
        genres,
        series,
        page_count: edition.pages.or(book.pages).filter(|p| *p > 0),
        language,
        image_url: clean(edition.image.and_then(|i| i.url))
            .or_else(|| clean(book.image.and_then(|i| i.url))),
        source: Some(SOURCE.to_owned()),
        source_id,
    })
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EditionRank {
    users_count: Option<i64>,
    completeness: i32,
}

fn pick_best_edition(editions: Vec<Edition>) -> Option<Edition> {
    let mut best: Option<(EditionRank, Edition)> = None;

    for edition in editions {
        let rank = rank_edition(&edition);
        let better = match &best {
            None => true,
            Some((best_rank, _)) => rank > *best_rank,
        };
        if better {
            best = Some((rank, edition));
        }
    }

    best.map(|(_, edition)| edition)
}

fn rank_edition(edition: &Edition) -> EditionRank {
    EditionRank {
        users_count: edition.users_count,
        completeness: completeness_score(edition),
    }
}

fn completeness_score(edition: &Edition) -> i32 {
    let mut score = 0;
    if edition.isbn_13.is_some() {
        score += 4;
    }
    if edition.isbn_10.is_some() {
        score += 2;
    }
    if edition
        .publisher
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .is_some()
    {
        score += 2;
    }
    if edition.release_date.is_some() {
        score += 1;
    }
    if edition.pages.is_some_and(|p| p > 0) {
        score += 1;
    }
    if edition
        .image
        .as_ref()
        .and_then(|i| i.url.as_ref())
        .is_some()
    {
        score += 1;
    }
    if let Some(format) = edition.format.as_deref() {
        let format = format.to_ascii_lowercase();
        if format.contains("hardcover") || format.contains("paperback") {
            score += 2;
        }
    }
    score
}

fn contributors(book: &Book) -> Vec<BookContributor> {
    let mut out: Vec<BookContributor> = Vec::new();
    for contribution in book.contributions.as_deref().unwrap_or_default() {
        let Some(name) = clean(contribution.author.as_ref().and_then(|a| a.name.clone())) else {
            continue;
        };
        let role = clean(contribution.contribution.clone())
            .unwrap_or_else(|| BookContributor::AUTHOR.to_owned());
        let contributor = BookContributor::new(name, role);
        if !out.contains(&contributor) {
            out.push(contributor);
        }
    }
    out
}

fn genres(cached_tags: Option<&Value>) -> Vec<String> {
    let Some(value) = cached_tags else {
        return Vec::new();
    };

    let list = match value {
        Value::Object(map) => map
            .iter()
            .find(|(key, _)| {
                key.eq_ignore_ascii_case("genre") || key.eq_ignore_ascii_case("genres")
            })
            .and_then(|(_, v)| v.as_array()),
        Value::Array(items) => Some(items),
        _ => None,
    };
    let Some(list) = list else {
        return Vec::new();
    };

    let mut tagged: Vec<(i64, String)> = list
        .iter()
        .filter_map(|item| match item {
            Value::String(name) => clean(Some(name.clone())).map(|n| (0, n)),
            Value::Object(map) => {
                let name = map
                    .get("tag")
                    .or_else(|| map.get("name"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                clean(name).map(|n| (map.get("count").and_then(Value::as_i64).unwrap_or(0), n))
            }
            _ => None,
        })
        .collect();
    tagged.sort_by_key(|a| std::cmp::Reverse(a.0));

    genres::normalize(tagged.into_iter().map(|(_, name)| name))
}

fn series(rows: &[BookSeriesRow]) -> Option<BookSeries> {
    let mut fallback: Option<BookSeries> = None;

    for row in rows {
        let Some(title) = clean(row.series.as_ref().and_then(|s| s.name.clone())) else {
            continue;
        };
        let number = row.position.as_ref().and_then(value_to_f32).or_else(|| {
            row.details
                .as_deref()
                .and_then(series_parser::position_in_label)
        });

        if number.is_some() {
            return Some(BookSeries { title, number });
        }
        fallback.get_or_insert_with(|| BookSeries::unnumbered(title));
    }

    fallback
}

fn value_to_f32(value: &Value) -> Option<f32> {
    match value {
        Value::Number(number) => number.to_string().parse::<f32>().ok(),
        Value::String(text) => text.trim().parse::<f32>().ok(),
        _ => None,
    }
}

fn id_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) => clean(Some(s.clone())),
        _ => None,
    }
}

fn clean(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};

    fn utc(year: i32, month: u32, day: u32) -> Option<DateTime<Utc>> {
        Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).single()
    }
    use serde_json::json;

    fn book_from(value: Value) -> Book {
        serde_json::from_value(value).expect("fixture should deserialize")
    }

    fn edition_from(value: Value) -> Edition {
        serde_json::from_value(value).expect("fixture should deserialize")
    }

    #[test]
    fn maps_a_full_isbn_hit() {
        let edition = edition_from(json!({
            "isbn_10": "0345339703",
            "isbn_13": "9780345339706",
            "pages": 398,
            "release_date": "1986-08-12",
            "edition_format": "Mass Market Paperback",
            "publisher": { "name": "Del Rey" },
            "language": { "language": "English", "code3": "eng" },
            "image": { "url": "https://example.test/edition.jpg" },
            "book": {
                "id": 7,
                "title": "The Fellowship of the Ring",
                "subtitle": "Being the First Part of The Lord of the Rings",
                "description": "A hobbit leaves the Shire.",
                "pages": 423,
                "release_date": "1954-07-29",
                "cached_tags": {
                    "Genre": [
                        { "tag": "Fantasy", "count": 900 },
                        { "tag": "Classics", "count": 120 }
                    ],
                    "Mood": [{ "tag": "Adventurous", "count": 400 }]
                },
                "image": { "url": "https://example.test/book.jpg" },
                "contributions": [
                    { "contribution": null, "author": { "name": "J.R.R. Tolkien" } },
                    { "contribution": "Illustrator", "author": { "name": "Alan Lee" } }
                ],
                "book_series": [
                    { "position": 1, "details": "#1", "series": { "name": "The Lord of the Rings" } }
                ]
            }
        }));

        let meta = from_edition(edition).expect("should map");

        assert_eq!(meta.title, "The Fellowship of the Ring");
        assert_eq!(
            meta.subtitle.as_deref(),
            Some("Being the First Part of The Lord of the Rings")
        );
        assert_eq!(meta.publisher.as_deref(), Some("Del Rey"));
        assert_eq!(meta.isbn.as_deref(), Some("9780345339706"));
        assert_eq!(meta.page_count, Some(398));
        assert_eq!(
            meta.image_url.as_deref(),
            Some("https://example.test/edition.jpg")
        );
        assert_eq!(meta.language.as_deref(), Some("English"));
        assert_eq!(meta.publication_date, utc(1954, 7, 29));
        assert_eq!(meta.genres, vec!["Fantasy", "Classics"]);
        assert_eq!(
            meta.contributors,
            vec![
                BookContributor::author("J.R.R. Tolkien"),
                BookContributor::new("Alan Lee", "Illustrator"),
            ]
        );
        assert_eq!(
            meta.series,
            Some(BookSeries::new("The Lord of the Rings", 1.0))
        );
        assert_eq!(meta.source.as_deref(), Some("hardcover"));
        assert_eq!(meta.source_id.as_deref(), Some("7"));
    }

    #[test]
    fn falls_back_to_the_work_when_the_edition_is_bare() {
        let edition = edition_from(json!({
            "book": { "title": "Dune", "pages": 412, "image": { "url": "https://example.test/dune.jpg" } }
        }));

        let meta = from_edition(edition).expect("should map");

        assert_eq!(meta.title, "Dune");
        assert_eq!(meta.page_count, Some(412));
        assert_eq!(
            meta.image_url.as_deref(),
            Some("https://example.test/dune.jpg")
        );
        assert_eq!(meta.isbn, None);
        assert_eq!(meta.publisher, None);
    }

    #[test]
    fn falls_back_to_the_edition_date_when_the_work_has_none() {
        let edition = edition_from(json!({
            "release_date": "1986-08-12",
            "book": { "title": "Undated Work" }
        }));

        let meta = from_edition(edition).expect("should map");

        assert_eq!(meta.publication_date, utc(1986, 8, 12));
    }

    #[test]
    fn the_most_read_edition_wins_over_the_most_complete_one() {
        let book = book_from(json!({
            "title": "Dune",
            "editions": [
                {
                    "isbn_13": "9780441013593",
                    "edition_format": "Paperback",
                    "publisher": { "name": "Obscure Press" },
                    "pages": 704,
                    "image": { "url": "https://example.test/obscure.jpg" },
                    "users_count": 3
                },
                {
                    "publisher": { "name": "Ace" },
                    "image": { "url": "https://example.test/popular.jpg" },
                    "users_count": 4200
                }
            ]
        }));

        let meta = from_book(book).expect("should map");

        assert_eq!(meta.publisher.as_deref(), Some("Ace"));
        assert_eq!(
            meta.image_url.as_deref(),
            Some("https://example.test/popular.jpg")
        );
    }

    #[test]
    fn an_uncounted_edition_loses_to_a_counted_one() {
        let book = book_from(json!({
            "title": "Dune",
            "editions": [
                { "publisher": { "name": "Uncounted" }, "isbn_13": "9780441013593", "pages": 704 },
                { "publisher": { "name": "Counted" }, "users_count": 1 }
            ]
        }));

        let meta = from_book(book).expect("should map");
        assert_eq!(meta.publisher.as_deref(), Some("Counted"));
    }

    #[test]
    fn completeness_decides_between_equally_read_editions() {
        let book = book_from(json!({
            "title": "Obscure",
            "editions": [
                { "publisher": { "name": "Sparse" }, "users_count": 0 },
                {
                    "publisher": { "name": "Complete" },
                    "isbn_13": "9780441013593",
                    "pages": 300,
                    "edition_format": "Hardcover",
                    "users_count": 0
                }
            ]
        }));

        let meta = from_book(book).expect("should map");
        assert_eq!(meta.publisher.as_deref(), Some("Complete"));
    }

    #[test]
    fn the_readers_edition_keeps_its_own_language() {
        let book = book_from(json!({
            "title": "Os Maias",
            "editions": [
                {
                    "publisher": { "name": "Livros do Brasil" },
                    "language": { "language": "Portuguese" },
                    "users_count": 900
                },
                {
                    "publisher": { "name": "Dedalus" },
                    "language": { "language": "English" },
                    "isbn_13": "9781873982860",
                    "pages": 714,
                    "users_count": 12
                }
            ]
        }));

        let meta = from_book(book).expect("should map");
        assert_eq!(meta.language.as_deref(), Some("Portuguese"));
        assert_eq!(meta.publisher.as_deref(), Some("Livros do Brasil"));
    }

    #[test]
    fn equal_candidates_keep_the_first_hardcover_returned() {
        let book = book_from(json!({
            "title": "Dune",
            "editions": [
                { "publisher": { "name": "First" }, "isbn_13": "9780441013593" },
                { "publisher": { "name": "Second" }, "isbn_13": "9780441013594" }
            ]
        }));

        let meta = from_book(book).expect("should map");
        assert_eq!(meta.publisher.as_deref(), Some("First"));
    }

    #[test]
    fn picks_the_most_complete_edition_for_a_title_hit() {
        let book = book_from(json!({
            "title": "Dune",
            "editions": [
                { "edition_format": "Audiobook" },
                {
                    "isbn_13": "9780441013593",
                    "edition_format": "Paperback",
                    "publisher": { "name": "Ace" },
                    "pages": 704
                },
                { "isbn_10": "0441172717" }
            ]
        }));

        let meta = from_book(book).expect("should map");

        assert_eq!(meta.isbn.as_deref(), Some("9780441013593"));
        assert_eq!(meta.publisher.as_deref(), Some("Ace"));
        assert_eq!(meta.page_count, Some(704));
    }

    #[test]
    fn drops_records_without_a_title() {
        assert!(from_book(book_from(json!({ "description": "no title here" }))).is_none());
        assert!(from_book(book_from(json!({ "title": "   " }))).is_none());
    }

    #[test]
    fn survives_missing_and_null_collections() {
        let book = book_from(json!({
            "title": "Sparse",
            "contributions": null,
            "book_series": null,
            "cached_tags": null,
            "editions": null
        }));

        let meta = from_book(book).expect("should map");

        assert!(meta.contributors.is_empty());
        assert!(meta.genres.is_empty());
        assert_eq!(meta.series, None);
    }

    #[test]
    fn reads_series_position_from_details_when_position_is_null() {
        let rows: Vec<BookSeriesRow> = serde_json::from_value(json!([
            { "position": null, "details": "Book 2.5", "series": { "name": "Vorkosigan Saga" } }
        ]))
        .expect("fixture should deserialize");

        assert_eq!(series(&rows), Some(BookSeries::new("Vorkosigan Saga", 2.5)));
    }

    #[test]
    fn prefers_a_numbered_series_over_an_unnumbered_one() {
        let rows: Vec<BookSeriesRow> = serde_json::from_value(json!([
            { "series": { "name": "Collected Works" } },
            { "position": "3", "series": { "name": "Discworld" } }
        ]))
        .expect("fixture should deserialize");

        assert_eq!(series(&rows), Some(BookSeries::new("Discworld", 3.0)));
    }

    #[test]
    fn keeps_an_unnumbered_series_when_no_position_exists_anywhere() {
        let rows: Vec<BookSeriesRow> = serde_json::from_value(json!([
            { "position": null, "details": null, "series": { "name": "The Culture" } }
        ]))
        .expect("fixture should deserialize");

        assert_eq!(series(&rows), Some(BookSeries::unnumbered("The Culture")));
    }

    #[test]
    fn reads_genres_from_alternative_tag_shapes() {
        assert_eq!(
            genres(Some(
                &json!({ "genres": ["Science Fiction", "Science fiction", " "] })
            )),
            vec!["Science Fiction"]
        );
        assert_eq!(genres(Some(&json!(["Horror"]))), vec!["Horror"]);
        assert!(genres(Some(&json!("nonsense"))).is_empty());
        assert!(genres(Some(&json!({ "Mood": [{ "tag": "Dark" }] }))).is_empty());
    }
}

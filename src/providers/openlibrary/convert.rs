use chrono::{DateTime, Utc};
use serde_json::Value;

use super::model::{Edition, Key, SearchDoc, Work};
use crate::metadata::{BookContributor, BookMetadata, BookSeries};
use crate::{dates, genres, languages, series as series_parser};

pub const SOURCE: &str = "openlibrary";

const MAX_GENRES: usize = 25;

const NON_GENRE_SUBJECTS: [&str; 12] = [
    "accessible book",
    "protected daisy",
    "in library",
    "internet archive wishlist",
    "overdrive",
    "large type books",
    "lending library",
    "popular print disabled books",
    "open library staff picks",
    "reading level",
    "physical object",
    "textbooks",
];

#[derive(Default)]
pub struct Parts {
    pub doc: Option<SearchDoc>,
    pub edition: Option<Edition>,
    pub work: Option<Work>,
    pub author_names: Vec<String>,
    pub queried_isbn: Option<String>,
}

pub fn assemble(parts: Parts, covers_url: &str) -> Option<BookMetadata> {
    let Parts {
        doc,
        edition,
        work,
        author_names,
        queried_isbn,
    } = parts;

    let doc = doc.unwrap_or_default();
    let edition = edition.unwrap_or_default();
    let work = work.unwrap_or_default();

    let title = clean(edition.title.clone())
        .or_else(|| clean(work.title.clone()))
        .or_else(|| clean(doc.title.clone()))?;

    let contributors = contributors(author_names, &doc, &edition);
    let genres = subjects(&work, &edition, &doc);
    let series = series(&edition);
    let image_url = cover_url(covers_url, &edition, &work, &doc);
    let source_id = olid(work_key(&doc, &edition));
    let description = description(work.description.as_ref())
        .or_else(|| description(edition.description.as_ref()));

    let publication_date = publication_date(&work, &edition, &doc);
    let language = language(&edition, &doc);
    let publisher = publisher(&edition, &doc);
    let isbn = queried_isbn.or_else(|| edition_isbn(&edition));

    Some(BookMetadata {
        title,
        subtitle: clean(edition.subtitle)
            .or_else(|| clean(work.subtitle))
            .or_else(|| clean(doc.subtitle)),
        description,
        publisher,
        publication_date,
        isbn,
        contributors,
        genres,
        series,
        page_count: edition
            .number_of_pages
            .or(doc.number_of_pages_median)
            .filter(|pages| *pages > 0),
        language,
        image_url,
        source: Some(SOURCE.to_owned()),
        source_id,
    })
}

fn publication_date(work: &Work, edition: &Edition, doc: &SearchDoc) -> Option<DateTime<Utc>> {
    work.first_publish_date
        .as_deref()
        .and_then(dates::parse)
        .or_else(|| {
            doc.first_publish_year
                .map(|year| year.to_string())
                .as_deref()
                .and_then(dates::parse)
        })
        .or_else(|| edition.publish_date.as_deref().and_then(dates::parse))
}

fn language(edition: &Edition, doc: &SearchDoc) -> Option<String> {
    first_of(
        edition
            .languages
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(Key::id),
    )
    .or_else(|| {
        first_of(
            doc.language
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(String::as_str),
        )
    })
    .map(|code| languages::name(&code))
}

fn publisher(edition: &Edition, doc: &SearchDoc) -> Option<String> {
    first_of(
        edition
            .publishers
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(String::as_str),
    )
    .or_else(|| {
        first_of(
            doc.publisher
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(String::as_str),
        )
    })
}

fn edition_isbn(edition: &Edition) -> Option<String> {
    first_of(
        edition
            .isbn_13
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(String::as_str),
    )
    .or_else(|| {
        first_of(
            edition
                .isbn_10
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(String::as_str),
        )
    })
}

fn description(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => clean(Some(text.clone())),
        Value::Object(map) => clean(map.get("value").and_then(Value::as_str).map(str::to_owned)),
        _ => None,
    }
}

fn contributors(
    author_names: Vec<String>,
    doc: &SearchDoc,
    edition: &Edition,
) -> Vec<BookContributor> {
    let mut out: Vec<BookContributor> = Vec::new();

    let searched_names = doc.author_name.clone().unwrap_or_default();
    for name in author_names.into_iter().chain(searched_names) {
        push_unique(&mut out, clean(Some(name)).map(BookContributor::author));
    }

    for contributor in edition.contributors.as_deref().unwrap_or_default() {
        let Some(name) = clean(contributor.name.clone()) else {
            continue;
        };
        let role =
            clean(contributor.role.clone()).unwrap_or_else(|| BookContributor::AUTHOR.to_owned());
        push_unique(&mut out, Some(BookContributor::new(name, role)));
    }

    out
}

fn push_unique(out: &mut Vec<BookContributor>, contributor: Option<BookContributor>) {
    if let Some(contributor) = contributor {
        if !out.contains(&contributor) {
            out.push(contributor);
        }
    }
}

fn subjects(work: &Work, edition: &Edition, doc: &SearchDoc) -> Vec<String> {
    let raw = work
        .subjects
        .clone()
        .unwrap_or_default()
        .into_iter()
        .chain(edition.subjects.clone().unwrap_or_default())
        .chain(doc.subject.clone().unwrap_or_default());

    genres::normalize(raw)
        .into_iter()
        .filter(|genre| {
            let lowered = genre.to_lowercase();
            !NON_GENRE_SUBJECTS.contains(&lowered.as_str())
        })
        .take(MAX_GENRES)
        .collect()
}

fn series(edition: &Edition) -> Option<BookSeries> {
    edition
        .series
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter_map(|raw| series_parser::parse_combined(raw))
        .reduce(|best, next| if best.number.is_some() { best } else { next })
}

fn cover_url(covers_url: &str, edition: &Edition, work: &Work, doc: &SearchDoc) -> Option<String> {
    let id = cover_id(edition.covers.as_deref())
        .or_else(|| cover_id(work.covers.as_deref()))
        .or_else(|| doc.cover_i.as_ref().and_then(positive_id))?;

    Some(format!(
        "{}/b/id/{id}-L.jpg",
        covers_url.trim_end_matches('/')
    ))
}

fn cover_id(covers: Option<&[Value]>) -> Option<i64> {
    covers?.iter().find_map(positive_id)
}

fn positive_id(value: &Value) -> Option<i64> {
    value.as_i64().filter(|id| *id > 0)
}

fn work_key(doc: &SearchDoc, edition: &Edition) -> Option<String> {
    doc.key.clone().or_else(|| {
        edition
            .works
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find_map(|key| key.key.clone())
    })
}

fn olid(key: Option<String>) -> Option<String> {
    clean(key?.rsplit('/').next().map(str::to_owned))
}

fn first_of<'a>(values: impl Iterator<Item = &'a str>) -> Option<String> {
    values
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_owned)
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

    const COVERS: &str = "https://covers.openlibrary.test";

    fn from<T: serde::de::DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).expect("fixture should deserialize")
    }

    #[test]
    fn maps_an_isbn_lookup_across_all_three_records() {
        let edition: Edition = from(json!({
            "title": "Fantastic Mr Fox",
            "publishers": ["Puffin"],
            "publish_date": "September 1988",
            "number_of_pages": 96,
            "isbn_10": ["0140328726"],
            "isbn_13": ["9780140328721"],
            "covers": [-1, 8_739_161],
            "languages": [{ "key": "/languages/eng" }],
            "series": ["Puffin Books ; 3"],
            "works": [{ "key": "/works/OL45804W" }],
            "contributors": [{ "role": "Illustrator", "name": "Quentin Blake" }],
            "subjects": ["Foxes -- Fiction"]
        }));
        let work: Work = from(json!({
            "title": "Fantastic Mr Fox",
            "description": { "type": "/type/text", "value": "A fox outwits three farmers." },
            "subjects": ["Children's stories", "Fiction, fantasy", "Accessible book"],
            "first_publish_date": "1970"
        }));

        let book = assemble(
            Parts {
                edition: Some(edition),
                work: Some(work),
                author_names: vec!["Roald Dahl".to_owned()],
                queried_isbn: Some("9780140328721".to_owned()),
                ..Parts::default()
            },
            COVERS,
        )
        .expect("should map");

        assert_eq!(book.title, "Fantastic Mr Fox");
        assert_eq!(
            book.description.as_deref(),
            Some("A fox outwits three farmers.")
        );
        assert_eq!(book.publisher.as_deref(), Some("Puffin"));
        assert_eq!(book.isbn.as_deref(), Some("9780140328721"));
        assert_eq!(book.page_count, Some(96));
        assert_eq!(book.language.as_deref(), Some("English"));
        assert_eq!(book.publication_date, utc(1970, 1, 1));
        assert_eq!(book.series, Some(BookSeries::new("Puffin Books", 3.0)));
        assert_eq!(
            book.contributors,
            vec![
                BookContributor::author("Roald Dahl"),
                BookContributor::new("Quentin Blake", "Illustrator"),
            ]
        );
        assert_eq!(
            book.genres,
            vec!["Children's stories", "Fiction", "fantasy", "Foxes"]
        );
        assert_eq!(
            book.image_url.as_deref(),
            Some("https://covers.openlibrary.test/b/id/8739161-L.jpg")
        );
        assert_eq!(book.source.as_deref(), Some("openlibrary"));
        assert_eq!(book.source_id.as_deref(), Some("OL45804W"));
    }

    #[test]
    fn maps_a_search_hit_without_any_enrichment() {
        let doc: SearchDoc = from(json!({
            "key": "/works/OL45804W",
            "title": "Fantastic Mr Fox",
            "author_name": ["Roald Dahl"],
            "first_publish_year": 1970,
            "publisher": ["Puffin"],
            "language": ["eng"],
            "number_of_pages_median": 96,
            "cover_i": 8_739_161,
            "subject": ["Children's stories"]
        }));

        let book = assemble(
            Parts {
                doc: Some(doc),
                ..Parts::default()
            },
            COVERS,
        )
        .expect("should map");

        assert_eq!(book.title, "Fantastic Mr Fox");
        assert_eq!(book.authors().collect::<Vec<_>>(), vec!["Roald Dahl"]);
        assert_eq!(book.publisher.as_deref(), Some("Puffin"));
        assert_eq!(book.language.as_deref(), Some("English"));
        assert_eq!(book.page_count, Some(96));
        assert_eq!(book.publication_date, utc(1970, 1, 1));
        assert_eq!(book.isbn, None);
        assert_eq!(book.series, None);
        assert_eq!(book.description, None);
    }

    #[test]
    fn the_edition_wins_over_the_search_hit_when_both_are_present() {
        let doc: SearchDoc = from(json!({
            "title": "Dune",
            "publisher": ["Chilton Books"],
            "number_of_pages_median": 412
        }));
        let edition: Edition = from(json!({
            "publishers": ["Ace"],
            "number_of_pages": 704,
            "isbn_13": ["9780441013593"]
        }));

        let book = assemble(
            Parts {
                doc: Some(doc),
                edition: Some(edition),
                ..Parts::default()
            },
            COVERS,
        )
        .expect("should map");

        assert_eq!(book.publisher.as_deref(), Some("Ace"));
        assert_eq!(book.page_count, Some(704));
        assert_eq!(book.isbn.as_deref(), Some("9780441013593"));
    }

    #[test]
    fn reads_a_plain_string_description() {
        let work: Work = from(json!({ "title": "Dune", "description": "Spice." }));

        let book = assemble(
            Parts {
                work: Some(work),
                ..Parts::default()
            },
            COVERS,
        )
        .expect("should map");

        assert_eq!(book.description.as_deref(), Some("Spice."));
    }

    #[test]
    fn skips_the_no_cover_marker() {
        let edition: Edition = from(json!({ "title": "Dune", "covers": [-1] }));

        let book = assemble(
            Parts {
                edition: Some(edition),
                ..Parts::default()
            },
            COVERS,
        )
        .expect("should map");

        assert_eq!(book.image_url, None);
    }

    #[test]
    fn prefers_a_numbered_series_entry() {
        let edition: Edition = from(json!({
            "title": "A Clash of Kings",
            "series": ["Everyman's Library", "A Song of Ice and Fire, book 2"]
        }));

        let book = assemble(
            Parts {
                edition: Some(edition),
                ..Parts::default()
            },
            COVERS,
        )
        .expect("should map");

        assert_eq!(
            book.series,
            Some(BookSeries::new("A Song of Ice and Fire", 2.0))
        );
    }

    #[test]
    fn caps_a_runaway_subject_list() {
        let subjects: Vec<String> = (0..100).map(|n| format!("Subject {n}")).collect();
        let work: Work = from(json!({ "title": "Tagged", "subjects": subjects }));

        let book = assemble(
            Parts {
                work: Some(work),
                ..Parts::default()
            },
            COVERS,
        )
        .expect("should map");

        assert_eq!(book.genres.len(), MAX_GENRES);
    }

    #[test]
    fn drops_records_without_a_title() {
        assert!(assemble(Parts::default(), COVERS).is_none());
    }

    #[test]
    fn survives_null_collections() {
        let edition: Edition = from(json!({
            "title": "Sparse",
            "publishers": null,
            "series": null,
            "covers": null,
            "languages": null,
            "contributors": null
        }));

        let book = assemble(
            Parts {
                edition: Some(edition),
                ..Parts::default()
            },
            COVERS,
        )
        .expect("should map");

        assert_eq!(book.title, "Sparse");
        assert!(book.contributors.is_empty());
        assert!(book.genres.is_empty());
        assert_eq!(book.series, None);
    }
}

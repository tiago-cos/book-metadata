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

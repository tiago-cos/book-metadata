use super::model::{ImageLinks, Volume};
use crate::metadata::{BookContributor, BookMetadata};
use crate::{dates, genres, languages};

pub const SOURCE: &str = "googlebooks";

const NON_GENRE_CATEGORIES: [&str; 4] = [
    "general",
    "miscellaneous",
    "general & miscellaneous",
    "nonclassifiable",
];

pub fn from_volume(volume: Volume, queried_isbn: Option<&str>) -> Option<BookMetadata> {
    let id = volume.id;
    let info = volume.volume_info?;

    let title = clean(info.title)?;

    let isbn = queried_isbn.map(str::to_owned).or_else(|| {
        let identifiers = info.industry_identifiers.as_deref().unwrap_or_default();
        let of_kind = |kind: &str| {
            identifiers
                .iter()
                .find(|identifier| identifier.is(kind))
                .and_then(|identifier| clean(identifier.identifier.clone()))
        };
        of_kind("ISBN_13").or_else(|| of_kind("ISBN_10"))
    });

    Some(BookMetadata {
        title,
        subtitle: clean(info.subtitle),
        description: clean(info.description).map(|text| strip_html(&text)),
        publisher: clean(info.publisher),
        publication_date: info.published_date.as_deref().and_then(dates::parse),
        isbn,
        contributors: info
            .authors
            .unwrap_or_default()
            .into_iter()
            .filter_map(|name| clean(Some(name)).map(BookContributor::author))
            .collect(),
        genres: categories(info.categories.unwrap_or_default()),
        series: None,
        page_count: info.page_count.filter(|pages| *pages > 0),
        language: clean(info.language).map(|code| languages::name(&code)),
        image_url: info.image_links.as_ref().and_then(cover_url),
        source: Some(SOURCE.to_owned()),
        source_id: clean(id),
    })
}

fn categories(categories: Vec<String>) -> Vec<String> {
    genres::normalize(categories)
        .into_iter()
        .filter(|genre| {
            let lowered = genre.to_lowercase();
            !NON_GENRE_CATEGORIES.contains(&lowered.as_str())
        })
        .collect()
}

fn cover_url(images: &ImageLinks) -> Option<String> {
    let url = images.best()?.trim();

    let url = url
        .strip_prefix("http://")
        .map_or_else(|| url.to_owned(), |rest| format!("https://{rest}"));

    Some(
        url.replace("&edge=curl", "")
            .replace("?edge=curl&", "?")
            .replace("&amp;edge=curl", ""),
    )
}

fn strip_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    let mut entity = String::new();

    for c in text.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            '&' => entity.push('&'),
            ';' if !entity.is_empty() => {
                out.push_str(&decode_entity(&entity[1..]));
                entity.clear();
            }
            _ if !entity.is_empty() => {
                if entity.len() > 12 || c.is_whitespace() {
                    out.push_str(&entity);
                    out.push(c);
                    entity.clear();
                } else {
                    entity.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    out.push_str(&entity);

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entity(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "amp" => "&".to_owned(),
        "lt" => "<".to_owned(),
        "gt" => ">".to_owned(),
        "quot" => "\"".to_owned(),
        "apos" | "#39" => "'".to_owned(),
        "nbsp" => " ".to_owned(),
        "mdash" => "—".to_owned(),
        "ndash" => "–".to_owned(),
        "hellip" => "…".to_owned(),
        _ => format!("&{name};"),
    }
}

fn clean(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

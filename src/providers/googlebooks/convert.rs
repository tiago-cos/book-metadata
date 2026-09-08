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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};

    fn utc(year: i32, month: u32, day: u32) -> Option<DateTime<Utc>> {
        Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).single()
    }
    use serde_json::{Value, json};

    fn volume(value: Value) -> Volume {
        serde_json::from_value(value).expect("fixture should deserialize")
    }

    #[test]
    fn maps_a_full_volume() {
        let volume = volume(json!({
            "id": "B1hSG45JCX4C",
            "volumeInfo": {
                "title": "Dune",
                "subtitle": "The Graphic Novel",
                "authors": ["Frank Herbert", "Brian Herbert"],
                "publisher": "Ace Books",
                "publishedDate": "2005-06",
                "description": "<p>Set on the desert planet <i>Arrakis</i>.</p>",
                "industryIdentifiers": [
                    { "type": "OTHER", "identifier": "OCLC:123" },
                    { "type": "ISBN_10", "identifier": "0441013597" },
                    { "type": "ISBN_13", "identifier": "9780441013593" }
                ],
                "pageCount": 604,
                "categories": ["Fiction / Science Fiction / General"],
                "language": "en",
                "imageLinks": {
                    "smallThumbnail": "http://books.google.com/books/content?id=X&img=1&zoom=5&edge=curl",
                    "thumbnail": "http://books.google.com/books/content?id=X&img=1&zoom=1&edge=curl"
                }
            }
        }));

        let book = from_volume(volume, None).expect("should map");

        assert_eq!(book.title, "Dune");
        assert_eq!(book.subtitle.as_deref(), Some("The Graphic Novel"));
        assert_eq!(
            book.description.as_deref(),
            Some("Set on the desert planet Arrakis .")
        );
        assert_eq!(book.publisher.as_deref(), Some("Ace Books"));
        assert_eq!(book.publication_date, utc(2005, 6, 1));
        assert_eq!(book.isbn.as_deref(), Some("9780441013593"));
        assert_eq!(
            book.contributors,
            vec![
                BookContributor::author("Frank Herbert"),
                BookContributor::author("Brian Herbert"),
            ]
        );
        assert_eq!(book.genres, vec!["Fiction", "Science Fiction"]);
        assert_eq!(book.page_count, Some(604));
        assert_eq!(book.language.as_deref(), Some("English"));
        assert_eq!(book.series, None);
        assert_eq!(book.source.as_deref(), Some("googlebooks"));
        assert_eq!(book.source_id.as_deref(), Some("B1hSG45JCX4C"));
    }

    #[test]
    fn cleans_up_the_cover_url() {
        let volume = volume(json!({
            "volumeInfo": {
                "title": "Dune",
                "imageLinks": {
                    "thumbnail": "http://books.google.com/books/content?id=X&img=1&edge=curl&source=gbs_api",
                    "large": "http://books.google.com/books/content?id=X&img=1&zoom=3"
                }
            }
        }));

        let book = from_volume(volume, None).expect("should map");

        assert_eq!(
            book.image_url.as_deref(),
            Some("https://books.google.com/books/content?id=X&img=1&zoom=3")
        );
    }

    #[test]
    fn drops_the_page_curl_overlay() {
        let volume = volume(json!({
            "volumeInfo": {
                "title": "Dune",
                "imageLinks": {
                    "thumbnail": "https://books.google.com/books/content?id=X&edge=curl&source=gbs_api"
                }
            }
        }));

        let book = from_volume(volume, None).expect("should map");
        let url = book.image_url.expect("a cover");
        assert!(!url.contains("edge=curl"), "got {url}");
        assert!(url.contains("source=gbs_api"));
    }

    #[test]
    fn the_queried_isbn_wins_over_the_volumes_own() {
        let volume = volume(json!({
            "volumeInfo": {
                "title": "Dune",
                "industryIdentifiers": [{ "type": "ISBN_13", "identifier": "9999999999999" }]
            }
        }));

        let book = from_volume(volume, Some("9780441013593")).expect("should map");
        assert_eq!(book.isbn.as_deref(), Some("9780441013593"));
    }

    #[test]
    fn flattens_html_descriptions() {
        assert_eq!(strip_html("<p>One.</p><p>Two.</p>"), "One. Two.");
        assert_eq!(strip_html("A<br>B"), "A B");
        assert_eq!(strip_html("Salt &amp; Pepper"), "Salt & Pepper");
        assert_eq!(strip_html("&quot;Quoted&quot;"), "\"Quoted\"");
        assert_eq!(strip_html("a&nbsp;b"), "a b");
        assert_eq!(strip_html("Tom &#39;s"), "Tom 's");
        assert_eq!(
            strip_html("<b>Bold</b> and <i>italic</i>"),
            "Bold and italic"
        );
        assert_eq!(strip_html("Plain text."), "Plain text.");
        assert_eq!(strip_html("Fish & chips"), "Fish & chips");
        assert_eq!(strip_html("&dagger;"), "&dagger;");
    }

    #[test]
    fn drops_volumes_without_a_title() {
        assert!(from_volume(volume(json!({ "id": "X" })), None).is_none());
        assert!(from_volume(volume(json!({ "volumeInfo": { "title": " " } })), None).is_none());
    }

    #[test]
    fn survives_a_bare_volume() {
        let book = from_volume(volume(json!({ "volumeInfo": { "title": "Sparse" } })), None)
            .expect("should map");

        assert_eq!(book.title, "Sparse");
        assert!(book.contributors.is_empty());
        assert!(book.genres.is_empty());
        assert_eq!(book.isbn, None);
        assert_eq!(book.image_url, None);
    }
}

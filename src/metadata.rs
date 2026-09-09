use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Book metadata in a provider-independent shape.
///
/// Every provider maps its own payload onto this struct, so switching or
/// adding a source never changes the type your application consumes.
/// Absolutely everything except [`title`](Self::title) is optional, because
/// coverage differs wildly between sources.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BookMetadata {
    /// The main title of the book.
    pub title: String,
    /// An optional subtitle of the book.
    pub subtitle: Option<String>,
    /// An optional description or summary of the book.
    pub description: Option<String>,
    /// The publisher of the book, if available.
    pub publisher: Option<String>,
    /// The publication date of the book, as a UTC datetime.
    ///
    /// This is the *first* edition's date whenever the provider knows it, so
    /// the value is stable no matter which printing a query happened to
    /// resolve; it falls back to the matched edition's date otherwise.
    ///
    /// Sources usually only expose a calendar date; in that case the time is
    /// set to midnight UTC.
    pub publication_date: Option<DateTime<Utc>>,
    /// The ISBN of the book, if available. ISBN-13 is preferred when the
    /// provider exposes both.
    pub isbn: Option<String>,
    /// A list of contributors to the book, each represented as a [`BookContributor`].
    pub contributors: Vec<BookContributor>,
    /// A list of genres associated with the book.
    pub genres: Vec<String>,
    /// The series information, if the book is part of a series.
    pub series: Option<BookSeries>,
    /// The number of pages in the book, if available.
    pub page_count: Option<i64>,
    /// The language of the book, if available.
    pub language: Option<String>,
    /// A URL to an image of the book's cover, if available.
    pub image_url: Option<String>,
    /// Name of the provider this record came from, e.g. `"hardcover"`.
    pub source: Option<String>,
    /// The provider's own identifier for this book, useful for caching or for
    /// linking back to the source.
    pub source_id: Option<String>,
}

impl BookMetadata {
    /// Creates a record with only a title set.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    /// The names of every contributor whose role is `"Author"`.
    pub fn authors(&self) -> impl Iterator<Item = &str> {
        self.contributors
            .iter()
            .filter(|c| c.is_author())
            .map(|c| c.name.as_str())
    }

    /// Title and subtitle joined with `": "`, or just the title when there is
    /// no subtitle.
    #[must_use]
    pub fn full_title(&self) -> String {
        match &self.subtitle {
            Some(subtitle) if !subtitle.is_empty() => format!("{}: {}", self.title, subtitle),
            _ => self.title.clone(),
        }
    }
}

/// A person (or organisation) credited on a book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookContributor {
    /// Display name of the contributor.
    pub name: String,
    /// Role played, e.g. `"Author"`, `"Illustrator"`, `"Translator"`,
    /// `"Narrator"`. Providers that do not distinguish roles report
    /// [`BookContributor::AUTHOR`].
    pub role: String,
}

impl BookContributor {
    /// The role used for a book's main writer, and the fallback for providers
    /// that do not report roles at all.
    pub const AUTHOR: &'static str = "Author";

    /// Creates a contributor with an explicit role.
    pub fn new(name: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            role: role.into(),
        }
    }

    /// Creates a contributor with the [`AUTHOR`](Self::AUTHOR) role.
    pub fn author(name: impl Into<String>) -> Self {
        Self::new(name, Self::AUTHOR)
    }

    /// Whether this contributor is credited as an author.
    #[must_use]
    pub fn is_author(&self) -> bool {
        self.role.eq_ignore_ascii_case(Self::AUTHOR)
    }
}

/// The series a book belongs to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookSeries {
    /// Name of the series.
    pub title: String,
    /// Position within the series.
    ///
    /// This is optional and not an `f32` on purpose: sources routinely list a
    /// book as part of a series without giving it a position (companion
    /// volumes, short stories, omnibus editions). It is a float rather than an
    /// integer so that half-entries such as `2.5` survive the round trip.
    pub number: Option<f32>,
}

impl BookSeries {
    /// Creates a series entry with a known position.
    pub fn new(title: impl Into<String>, number: f32) -> Self {
        Self {
            title: title.into(),
            number: Some(number),
        }
    }

    /// Creates a series entry whose position is unknown.
    pub fn unnumbered(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            number: None,
        }
    }
}

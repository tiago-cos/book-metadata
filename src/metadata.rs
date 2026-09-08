use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BookMetadata {
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub publication_date: Option<DateTime<Utc>>,
    pub isbn: Option<String>,
    pub contributors: Vec<BookContributor>,
    pub genres: Vec<String>,
    pub series: Option<BookSeries>,
    pub page_count: Option<i64>,
    pub language: Option<String>,
    pub image_url: Option<String>,
    pub source: Option<String>,
    pub source_id: Option<String>,
}

impl BookMetadata {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    pub fn authors(&self) -> impl Iterator<Item = &str> {
        self.contributors
            .iter()
            .filter(|c| c.is_author())
            .map(|c| c.name.as_str())
    }

    pub fn full_title(&self) -> String {
        match &self.subtitle {
            Some(subtitle) if !subtitle.is_empty() => format!("{}: {}", self.title, subtitle),
            _ => self.title.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookContributor {
    pub name: String,
    pub role: String,
}

impl BookContributor {
    pub const AUTHOR: &'static str = "Author";

    pub fn new(name: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            role: role.into(),
        }
    }

    pub fn author(name: impl Into<String>) -> Self {
        Self::new(name, Self::AUTHOR)
    }

    pub fn is_author(&self) -> bool {
        self.role.eq_ignore_ascii_case(Self::AUTHOR)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookSeries {
    pub title: String,
    pub number: Option<f32>,
}

impl BookSeries {
    pub fn new(title: impl Into<String>, number: f32) -> Self {
        Self {
            title: title.into(),
            number: Some(number),
        }
    }

    pub fn unnumbered(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            number: None,
        }
    }
}

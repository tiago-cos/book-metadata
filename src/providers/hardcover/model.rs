use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct GraphQlResponse<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<GraphQlError>>,
}

impl<T> GraphQlResponse<T> {
    pub fn error_message(&self) -> Option<String> {
        let errors = self.errors.as_ref()?;
        if errors.is_empty() {
            return None;
        }
        Some(
            errors
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

#[derive(Debug, Deserialize)]
pub struct GraphQlError {
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct EditionsData {
    pub editions: Option<Vec<Edition>>,
}

#[derive(Debug, Deserialize)]
pub struct BooksData {
    pub books: Option<Vec<Book>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Book {
    pub id: Option<Value>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub pages: Option<i64>,
    pub release_date: Option<String>,
    pub cached_tags: Option<Value>,
    pub image: Option<Image>,
    pub contributions: Option<Vec<Contribution>>,
    #[serde(rename = "book_series")]
    pub series: Option<Vec<BookSeriesRow>>,
    pub editions: Option<Vec<Edition>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Edition {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub isbn_10: Option<String>,
    pub isbn_13: Option<String>,
    pub pages: Option<i64>,
    pub release_date: Option<String>,
    #[serde(rename = "edition_format")]
    pub format: Option<String>,
    pub users_count: Option<i64>,
    pub publisher: Option<Named>,
    pub language: Option<Language>,
    pub image: Option<Image>,
    pub book: Option<Box<Book>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Named {
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Language {
    pub language: Option<String>,
    pub code3: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Image {
    pub url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Contribution {
    pub contribution: Option<String>,
    pub author: Option<Named>,
}

#[derive(Debug, Default, Deserialize)]
pub struct BookSeriesRow {
    pub position: Option<Value>,
    pub details: Option<String>,
    pub series: Option<Named>,
}

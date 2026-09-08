use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Default, Deserialize)]
pub struct Edition {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub publishers: Option<Vec<String>>,
    pub publish_date: Option<String>,
    pub number_of_pages: Option<i64>,
    pub isbn_10: Option<Vec<String>>,
    pub isbn_13: Option<Vec<String>>,
    pub covers: Option<Vec<Value>>,
    pub languages: Option<Vec<Key>>,
    pub series: Option<Vec<String>>,
    pub works: Option<Vec<Key>>,
    pub authors: Option<Vec<Key>>,
    pub contributors: Option<Vec<EditionContributor>>,
    pub subjects: Option<Vec<String>>,
    pub description: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Work {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<Value>,
    pub subjects: Option<Vec<String>>,
    pub first_publish_date: Option<String>,
    pub covers: Option<Vec<Value>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Author {
    pub name: Option<String>,
    pub personal_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Key {
    pub key: Option<String>,
}

impl Key {
    pub fn id(&self) -> Option<&str> {
        self.key
            .as_deref()?
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct EditionContributor {
    pub role: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchResponse {
    pub docs: Option<Vec<SearchDoc>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchDoc {
    pub key: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub author_name: Option<Vec<String>>,
    pub first_publish_year: Option<i32>,
    pub publisher: Option<Vec<String>>,
    pub language: Option<Vec<String>>,
    pub number_of_pages_median: Option<i64>,
    pub cover_i: Option<Value>,
    pub cover_edition_key: Option<String>,
    pub edition_key: Option<Vec<String>>,
    pub subject: Option<Vec<String>>,
}

impl SearchDoc {
    pub fn representative_edition(&self) -> Option<&str> {
        self.cover_edition_key
            .as_deref()
            .filter(|key| !key.is_empty())
            .or_else(|| {
                self.edition_key
                    .as_deref()?
                    .iter()
                    .map(String::as_str)
                    .find(|key| !key.is_empty())
            })
    }
}

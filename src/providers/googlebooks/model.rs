use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct VolumesResponse {
    pub items: Option<Vec<Volume>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Volume {
    pub id: Option<String>,
    #[serde(rename = "volumeInfo")]
    pub volume_info: Option<VolumeInfo>,
}

#[derive(Debug, Default, Deserialize)]
pub struct VolumeInfo {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub authors: Option<Vec<String>>,
    pub publisher: Option<String>,
    #[serde(rename = "publishedDate")]
    pub published_date: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "industryIdentifiers")]
    pub industry_identifiers: Option<Vec<IndustryIdentifier>>,
    #[serde(rename = "pageCount")]
    pub page_count: Option<i64>,
    pub categories: Option<Vec<String>>,
    pub language: Option<String>,
    #[serde(rename = "imageLinks")]
    pub image_links: Option<ImageLinks>,
}

#[derive(Debug, Default, Deserialize)]
pub struct IndustryIdentifier {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub identifier: Option<String>,
}

impl IndustryIdentifier {
    pub fn is(&self, kind: &str) -> bool {
        self.kind.as_deref().is_some_and(|k| k == kind)
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ImageLinks {
    #[serde(rename = "smallThumbnail")]
    pub small_thumbnail: Option<String>,
    pub thumbnail: Option<String>,
    pub small: Option<String>,
    pub medium: Option<String>,
    pub large: Option<String>,
    #[serde(rename = "extraLarge")]
    pub extra_large: Option<String>,
}

impl ImageLinks {
    pub fn best(&self) -> Option<&str> {
        [
            self.extra_large.as_deref(),
            self.large.as_deref(),
            self.medium.as_deref(),
            self.small.as_deref(),
            self.thumbnail.as_deref(),
            self.small_thumbnail.as_deref(),
        ]
        .into_iter()
        .flatten()
        .find(|url| !url.trim().is_empty())
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ErrorResponse {
    pub error: Option<ErrorBody>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ErrorBody {
    pub message: Option<String>,
    pub errors: Option<Vec<ErrorDetail>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ErrorDetail {
    pub reason: Option<String>,
}

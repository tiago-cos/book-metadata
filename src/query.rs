use crate::error::{Error, Result};

const DEFAULT_MAX_RESULTS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(feature = "_provider"), allow(dead_code))]
pub enum QueryKind {
    Isbn(String),
    TitleAuthor {
        title: String,
        author: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataQuery {
    kind: QueryKind,
    max_results: usize,
}

impl MetadataQuery {
    pub fn isbn(isbn: impl AsRef<str>) -> Self {
        Self::from_kind(QueryKind::Isbn(normalize_isbn(isbn.as_ref())))
    }

    pub fn title(title: impl Into<String>) -> Self {
        Self::from_kind(QueryKind::TitleAuthor {
            title: title.into().trim().to_owned(),
            author: None,
        })
    }

    pub fn title_and_author(title: impl Into<String>, author: impl Into<String>) -> Self {
        Self::title(title).with_author(author)
    }

    #[must_use]
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        if let QueryKind::TitleAuthor { author: slot, .. } = &mut self.kind {
            let author = author.into().trim().to_owned();
            *slot = (!author.is_empty()).then_some(author);
        }
        self
    }

    #[must_use]
    pub fn with_max_results(mut self, max_results: usize) -> Self {
        self.max_results = max_results.max(1);
        self
    }

    #[must_use]
    pub fn as_isbn(&self) -> Option<&str> {
        match &self.kind {
            QueryKind::Isbn(isbn) => Some(isbn),
            QueryKind::TitleAuthor { .. } => None,
        }
    }

    #[must_use]
    pub fn as_title_author(&self) -> Option<(&str, Option<&str>)> {
        match &self.kind {
            QueryKind::TitleAuthor { title, author } => Some((title, author.as_deref())),
            QueryKind::Isbn(_) => None,
        }
    }

    #[must_use]
    pub const fn max_results(&self) -> usize {
        self.max_results
    }

    pub fn validate(&self) -> Result<()> {
        match &self.kind {
            QueryKind::Isbn(isbn) => {
                if is_plausible_isbn(isbn) {
                    Ok(())
                } else {
                    Err(Error::InvalidQuery(format!(
                        "`{isbn}` is not a valid ISBN-10 or ISBN-13"
                    )))
                }
            }
            QueryKind::TitleAuthor { title, .. } => {
                if title.is_empty() {
                    Err(Error::InvalidQuery("the title is empty".to_owned()))
                } else if !title.chars().any(char::is_alphanumeric) {
                    Err(Error::InvalidQuery(format!(
                        "`{title}` contains no searchable characters"
                    )))
                } else {
                    Ok(())
                }
            }
        }
    }

    #[cfg(feature = "_provider")]
    pub(crate) const fn kind(&self) -> &QueryKind {
        &self.kind
    }

    const fn from_kind(kind: QueryKind) -> Self {
        Self {
            kind,
            max_results: DEFAULT_MAX_RESULTS,
        }
    }
}

fn normalize_isbn(raw: &str) -> String {
    raw.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn is_plausible_isbn(isbn: &str) -> bool {
    match isbn.len() {
        10 => {
            isbn[..9].chars().all(|c| c.is_ascii_digit())
                && isbn[9..].chars().all(|c| c.is_ascii_digit() || c == 'X')
        }
        13 => isbn.chars().all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_isbns() {
        assert_eq!(
            MetadataQuery::isbn("978-0-345-33970-6").as_isbn(),
            Some("9780345339706")
        );
        assert_eq!(
            MetadataQuery::isbn(" 0 345 33970 x ").as_isbn(),
            Some("034533970X")
        );
    }

    #[test]
    fn validates_isbn_shapes() {
        assert!(MetadataQuery::isbn("978-0-345-33970-6").validate().is_ok());
        assert!(MetadataQuery::isbn("034533970X").validate().is_ok());
        assert!(MetadataQuery::isbn("12345").validate().is_err());
        assert!(MetadataQuery::isbn("978034533970X").validate().is_err());
    }

    #[test]
    fn rejects_unsearchable_titles() {
        assert!(MetadataQuery::title("   ").validate().is_err());
        assert!(MetadataQuery::title("...").validate().is_err());
        assert!(MetadataQuery::title("Dune").validate().is_ok());
        assert!(MetadataQuery::title("1Q84").validate().is_ok());
    }

    #[test]
    fn exposes_the_query_through_accessors() {
        let isbn = MetadataQuery::isbn("9780441013593");
        assert_eq!(isbn.as_isbn(), Some("9780441013593"));
        assert_eq!(isbn.as_title_author(), None);

        let title = MetadataQuery::title_and_author("Dune", "Frank Herbert");
        assert_eq!(title.as_isbn(), None);
        assert_eq!(
            title.as_title_author(),
            Some(("Dune", Some("Frank Herbert")))
        );
    }

    #[test]
    fn author_is_only_kept_when_non_empty() {
        let query = MetadataQuery::title("Dune").with_author("  ");
        assert_eq!(query.as_title_author(), Some(("Dune", None)));
    }

    #[test]
    fn max_results_is_clamped() {
        assert_eq!(
            MetadataQuery::title("Dune")
                .with_max_results(0)
                .max_results(),
            1
        );
        assert_eq!(
            MetadataQuery::title("Dune").max_results(),
            DEFAULT_MAX_RESULTS
        );
    }
}

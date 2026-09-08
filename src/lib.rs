#![forbid(unsafe_code)]

mod error;
mod metadata;
mod provider;
mod query;

macro_rules! support_modules {
    ($($name:ident),* $(,)?) => {
        $(
            #[cfg(feature = "_provider")]
            #[cfg_attr(
                not(all(
                    feature = "hardcover",
                    feature = "openlibrary",
                    feature = "googlebooks"
                )),
                allow(dead_code)
            )]
            mod $name;
        )*
    };
}

support_modules!(dates, genres, http, series);

pub mod providers;

pub use error::{Error, Result};
pub use metadata::{BookContributor, BookMetadata, BookSeries};
pub use provider::MetadataProvider;
pub use query::MetadataQuery;

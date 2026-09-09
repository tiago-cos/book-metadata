//! Concrete metadata sources.
//!
//! Each provider lives behind its own cargo feature so you only compile (and
//! only depend on) the ones you actually use.

#[cfg(feature = "hardcover")]
pub mod hardcover;

#[cfg(feature = "openlibrary")]
pub mod openlibrary;

#[cfg(feature = "googlebooks")]
pub mod googlebooks;

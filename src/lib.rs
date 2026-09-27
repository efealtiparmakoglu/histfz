//! histfz — fuzzy shell history search.
//!
//! The library half is deliberately separable: parsing, scoring and
//! ranking are all pure functions over strings, which is what makes the
//! behaviour testable without touching a real history file.

pub mod hist;
pub mod score;
pub mod search;

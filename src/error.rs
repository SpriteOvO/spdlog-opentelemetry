use thiserror::Error;

/// Represents errors that can occur in this crate.
#[derive(Error, Debug)]
pub enum Error {
    /// Returned when value bag encounters an error.
    #[error("{0}")]
    ValueBag(value_bag::Error),
}

/// Represents the result type for this crate.
pub type Result<T> = std::result::Result<T, Error>;

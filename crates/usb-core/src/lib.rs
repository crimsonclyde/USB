//! Read-only archive indexing and deterministic, hardware-independent planning.
pub mod catalogue;
pub mod compatibility;
pub mod planner;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("{0}")]
    Invalid(String),
    #[error("size arithmetic overflow")]
    Overflow,
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn io<T>(path: &std::path::Path, result: std::io::Result<T>) -> Result<T> {
    result.map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })
}

use thiserror::Error;

use crate::PathBuf;

#[derive(Error, Debug)]
pub enum Error {
    #[error("field not found")]
    FieldNotFound,
    #[error("select target is not an array")]
    SelectTargetIsNotArray,
    #[error("select matched multiple items")]
    SelectMatchedMultipleItems,
    #[error("select matched no items")]
    SelectMatchedNoItems,
    #[error("index out of bounds")]
    OutOfBounds,
    #[error("at {0}: {1}")]
    AtPath(PathBuf, Box<Error>),
}
pub type Result<T> = std::result::Result<T, Error>;

mod error;
pub use error::*;
mod element;
pub use element::Element;
mod ext;
mod path;
pub use ext::FieldpathExt;
pub use path::{Path, PathBuf};
mod parse;
pub use parse::parse;

/// Construct &Path without parsing
#[macro_export]
macro_rules! path {
    ($(.$text:literal)+) => {
        &[$(::fieldpath::Element::StaticField($text)),+][..] as &fieldpath::Path
    };
}

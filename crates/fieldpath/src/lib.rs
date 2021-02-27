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

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

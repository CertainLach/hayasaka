use crate::Element;
use peg::str::LineCol;
use std::{
    fmt::{self, Display},
    ops::Deref,
    str::FromStr,
};

#[derive(Debug, PartialEq)]
pub struct PathBuf(pub Vec<Element>);
pub type Path = [Element];

impl From<&Path> for PathBuf {
    fn from(p: &Path) -> Self {
        PathBuf(p.into())
    }
}

impl Display for PathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for part in self.0.iter() {
            write!(f, "{}", part)?;
        }
        Ok(())
    }
}
impl Deref for PathBuf {
    type Target = [Element];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for PathBuf {
    type Err = peg::error::ParseError<LineCol>;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        crate::parse(s)
    }
}

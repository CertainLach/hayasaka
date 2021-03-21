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

impl PathBuf {
    pub fn from_rfc6901(str: String) -> Self {
        let mut out = Vec::new();
        for part in str.split('/').skip(1) {
            out.push(Element::Field(part.replace("~1", "/").replace("~0", "~")))
        }
        Self(out)
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

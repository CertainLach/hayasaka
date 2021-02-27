use serde_json::Value;
use std::fmt::{self, Display};

#[derive(Debug, PartialEq, Clone)]
pub enum Element {
    Field(String),
    Select(String, Value),
    Index(usize),
}
impl Display for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Field(field) => write!(f, ".{}", field),
            Self::Select(key, value) => write!(f, "[{}={}]", key, value),
            Self::Index(idx) => write!(f, "{}", idx),
        }
    }
}

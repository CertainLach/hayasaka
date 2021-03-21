use serde_json::Value;
use std::fmt::{self, Display};

#[derive(Debug, PartialEq, Clone)]
pub enum Element {
    Field(String),
    StaticField(&'static str),
    Select(String, Value),
    Index(usize),
}

fn write_field(f: &mut fmt::Formatter<'_>, n: &str) -> fmt::Result {
    if n.contains(|c| c == '"' || c == '.') {
        write!(f, ".\"{}\"", n.replace("\"", "\\\""))
    } else {
        write!(f, ".{}", n)
    }
}

impl Display for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaticField(field) => write_field(f, field),
            Self::Field(field) => write_field(f, field),
            Self::Select(key, value) => write!(f, "[{}={}]", key, value),
            Self::Index(idx) => write!(f, "{}", idx),
        }
    }
}

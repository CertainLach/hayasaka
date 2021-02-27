use crate::{path::PathBuf, Element};
use peg::str::LineCol;
use serde_json::Value;

peg::parser! {
    pub grammar parser() for str {
        rule field() -> Element
            = "." name:$((!['.' | '\n' | '['][_])+) {
                Element::Field(name.to_owned())
            };
        rule index() -> Element
            = "[" idx:$(['0'..='9']+) "]" {
                Element::Index(idx.parse().unwrap())
            }
        rule selector() -> Element
            = "[" key:$((!['='][_])+) "=\"" value:$((!['"'][_])+) "\"]" {
                Element::Select(key.to_owned(), Value::String(value.to_owned()))
            }
        rule element() -> Element
            = field()
            / index()
            / selector()

        pub rule path() -> PathBuf
            = path:element()+ { PathBuf(path) }
    }
}

pub fn parse(input: &str) -> Result<PathBuf, peg::error::ParseError<LineCol>> {
    parser::path(input)
}

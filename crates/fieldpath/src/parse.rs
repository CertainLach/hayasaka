use crate::{path::PathBuf, Element};
use peg::str::LineCol;
use serde_json::Value;

peg::parser! {
    pub grammar parser() for str {
        rule string() -> String
            = "\"" str:$((("\\\"")+ / (!['"' | '\\'][_])+)+) "\"" {
                str.replace("\\\"", "\"")
            }
            / "'" str:$((("\\'")+ / (!['\''][_])+)+) "'" {
                str.replace("\\'", "'")
            }
       rule field() -> Element
            = "." name:string() {
                Element::Field(name)
            }
            / "." name:$((!['.' | '\n' | '['][_])+) {
                Element::Field(name.to_owned())
            }
        rule index() -> Element
            = "[" idx:$(['0'..='9']+) "]" {
                Element::Index(idx.parse().unwrap())
            }
        rule selector() -> Element
            = "[" key:$((!['='][_])+) "=\"" value:string() "]" {
                Element::Select(key.to_owned(), Value::String(value))
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

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::{Element::*, PathBuf};

    #[test]
    fn escaping() {
        assert_eq!(
            parse(r#".aaa."aaa"."aa.aa"."aa\"aa""#).unwrap(),
            PathBuf(vec![
                Field("aaa".to_owned()),
                Field("aaa".to_owned()),
                Field("aa.aa".to_owned()),
                Field("aa\"aa".to_owned()),
            ])
        )
    }
}

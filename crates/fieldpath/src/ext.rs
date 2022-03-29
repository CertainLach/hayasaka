use crate::{Element, Error, Path, Result};
use duplicate::duplicate_item;
use serde_json::Value;

fn matches_select(item: &Value, key: &str, value: &Value) -> bool {
    match item {
        Value::Object(obj) => obj.get(key).map(|found| found == value).unwrap_or(false),
        _ => false,
    }
}

pub trait FieldpathExt: Sized {
    fn get_comp(&self, comp: &Element) -> Result<&Self>;
    fn get_path(&self, path: &Path) -> Result<&Self>;

    fn get_comp_mut(&mut self, comp: &Element) -> Result<&mut Self>;
    fn get_path_mut(&mut self, path: &Path) -> Result<&mut Self>;

    fn remove_comp(&mut self, comp: &Element) -> Result<Option<Self>>;
    fn remove_path(&mut self, path: &Path) -> Result<Option<Self>>;

    fn set_comp(&mut self, comp: &Element, value: Self) -> Result<Option<Self>>;
    fn set_path(&mut self, path: &Path, value: Self) -> Result<Option<Self>>;

    fn has_path(&self, path: &Path) -> bool;
}
impl FieldpathExt for Value {
    #[duplicate_item(
        this_method method reference(type) ret_type;
        [get_comp] [get] [&type] [&Self];
        [get_comp_mut] [get_mut] [&mut type] [&mut Self]
    )]
    fn this_method(self: reference([Self]), comp: &Element) -> Result<ret_type> {
        match comp {
            Element::Field(field) => self.method(&field).ok_or(Error::FieldNotFound),
            Element::StaticField(field) => self.method(&field).ok_or(Error::FieldNotFound),
            Element::Select(key, value) => match self {
                Value::Array(items) => {
                    let mut found = None;
                    for item in items {
                        if matches_select(item, &key, &value) {
                            if found.is_some() {
                                return Err(Error::SelectMatchedMultipleItems);
                            }
                            found.replace(item);
                        }
                    }
                    found.ok_or(Error::SelectMatchedNoItems)
                }
                _ => return Err(Error::SelectTargetIsNotArray),
            },
            Element::Index(idx) => match self {
                Value::Array(items) => items.method(*idx as usize).ok_or(Error::OutOfBounds),
                _ => return Err(Error::SelectTargetIsNotArray),
            },
        }
    }

    #[duplicate_item(
        this_method method reference(type) ret_type;
        [get_path] [get_comp] [&type] [&Self];
        [get_path_mut] [get_comp_mut] [&mut type] [&mut Self]
    )]
    fn this_method(self: reference([Self]), path: &Path) -> Result<ret_type> {
        let mut found = self;
        for (idx, elem) in path.iter().enumerate() {
            found = found
                .method(elem)
                .map_err(|e| Error::AtPath((&path[0..idx]).into(), Box::new(e)))?;
        }
        Ok(found)
    }

    fn remove_comp(&mut self, comp: &Element) -> Result<Option<Self>> {
        match comp {
            Element::StaticField(field) => match self {
                Value::Object(obj) => Ok(obj.remove(field.to_owned())),
                _ => Err(Error::FieldNotFound),
            },
            Element::Field(field) => match self {
                Value::Object(obj) => Ok(obj.remove(field)),
                _ => Err(Error::FieldNotFound),
            },
            Element::Select(key, value) => match self {
                Value::Array(items) => {
                    let mut found = None;
                    for (idx, item) in items.iter().enumerate() {
                        if matches_select(item, &key, &value) {
                            if found.is_some() {
                                return Err(Error::SelectMatchedMultipleItems);
                            }
                            found.replace(idx);
                        }
                    }
                    let found = found.ok_or(Error::SelectMatchedNoItems)?;
                    Ok(Some(items.remove(found)))
                }
                _ => return Err(Error::SelectTargetIsNotArray),
            },
            Element::Index(idx) => match self {
                Value::Array(arr) => {
                    if *idx < arr.len() {
                        Ok(Some(arr.remove(*idx)))
                    } else {
                        Err(Error::OutOfBounds)
                    }
                }
                _ => Err(Error::SelectTargetIsNotArray),
            },
        }
    }
    fn remove_path(&mut self, path: &Path) -> Result<Option<Self>> {
        let (el, path) = path.split_last().expect("empty path");
        let this = self.get_path_mut(&path)?;
        this.remove_comp(el)
            .map_err(|e| Error::AtPath(path.into(), Box::new(e)))
    }

    fn set_comp(&mut self, comp: &Element, target: Self) -> Result<Option<Self>> {
        match comp {
            Element::StaticField(field) => match self {
                Value::Object(obj) => Ok(obj.insert(field.to_string(), target)),
                _ => Err(Error::FieldNotFound),
            },
            Element::Field(field) => match self {
                Value::Object(obj) => Ok(obj.insert(field.to_owned(), target)),
                _ => Err(Error::FieldNotFound),
            },
            Element::Select(key, value) => match self {
                Value::Array(items) => {
                    let mut found = None;
                    for (idx, item) in items.iter().enumerate() {
                        if matches_select(item, &key, &value) {
                            if found.is_some() {
                                return Err(Error::SelectMatchedMultipleItems);
                            }
                            found.replace(idx);
                        }
                    }
                    let found = found.ok_or(Error::SelectMatchedNoItems)?;
                    Ok(Some(std::mem::replace(&mut items[found], target)))
                }
                _ => return Err(Error::SelectTargetIsNotArray),
            },
            Element::Index(idx) => match self {
                Value::Array(arr) => {
                    if *idx < arr.len() {
                        Ok(Some(std::mem::replace(&mut arr[*idx], target)))
                    } else {
                        Err(Error::OutOfBounds)
                    }
                }
                _ => Err(Error::SelectTargetIsNotArray),
            },
        }
    }
    fn set_path(&mut self, path: &Path, target: Self) -> Result<Option<Self>> {
        let (el, path) = path.split_last().expect("empty path");
        let this = self.get_path_mut(&path)?;
        this.set_comp(el, target)
            .map_err(|e| Error::AtPath(path.into(), Box::new(e)))
    }

    fn has_path(&self, path: &Path) -> bool {
        self.get_path(path).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::FieldpathExt;
    use crate::PathBuf;
    use std::str::FromStr;

    #[test]
    fn remove() {
        use serde_json::json;

        let mut obj = json!({
            "test": 3,
            "test2": 4,
        });
        obj.remove_path(&PathBuf::from_str(".test").unwrap())
            .unwrap();
        assert_eq!(
            obj,
            json!({
                "test2": 4,
            })
        );
    }
}

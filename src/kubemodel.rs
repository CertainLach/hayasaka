use std::rc::Rc;

/// Identifies object type in cluster
#[derive(PartialEq, Hash, Debug)]
pub struct ObjectType {
	pub api_version: Rc<str>,
	pub kind: Rc<str>,
	pub namespaced: bool,
}

/// Identifies object in cluster
#[derive(PartialEq, Hash, Debug)]
pub struct ObjectId {
	pub ty: ObjectType,

	pub name: Rc<str>,
	pub namespace: Option<Rc<str>>,
}

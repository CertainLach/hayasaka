use std::{
	fs,
	io::Read,
	process::{Command, Stdio},
	rc::Rc,
};

use blake2::{Blake2s256, Digest};
use gcmodule::Trace;
use jrsonnet_evaluator::{
	error::Error::RuntimeError,
	error::Result,
	native::{NativeCallback, NativeCallbackHandler},
	throw,
	typed::{Any, VecVal},
	unwrap_type, IStr, ObjValue, Val,
};
use jrsonnet_macros::builtin;
use jrsonnet_parser::{ExprLocation, Param, ParamsDesc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use serde_yaml_with_quirks::DeserializingQuirks;

use crate::{anyhow, bail, helm::obj_list_to_map};

#[builtin(fields(
    smth: bool,
))]
pub fn is_namespaced(api_version: IStr, kind: IStr) -> Result<bool> {
	Ok(true)
}

#[builtin(fields(
    default_ns: String,
))]
pub fn import_yaml_dir(
	#[self] this: &import_yaml_dir,
	#[location] from: Option<&ExprLocation>,
	dir: String,
) -> Result<Any> {
	let mut cwd = from.unwrap().0.to_path_buf();
	cwd.pop();
	cwd.push(&dir);

	let mut objects = vec![];
	for entry in walkdir::WalkDir::new(&cwd) {
		let entry = entry.map_err(|e| RuntimeError(format!("entry failed: {e}").into()))?;
		let ext = entry.path().extension().and_then(|s| s.to_str());
		if ext == Some("yaml") || ext == Some("yml") {
			let contents = fs::read_to_string(&entry.path())
				.map_err(|e| RuntimeError(format!("read failed: {e}").into()))?;
			for value in serde_yaml_with_quirks::Deserializer::from_str_with_quirks(
				&contents,
				DeserializingQuirks { old_octals: true },
			) {
				let value = Value::deserialize(value).unwrap();
				objects.push(Val::try_from(&value)?);
			}
		}
	}
	Ok(Any(obj_list_to_map(objects, &this.default_ns)?))
}

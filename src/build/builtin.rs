use std::{
	io::Read,
	process::{Command, Stdio},
};

use blake2::Blake2s256;
use digest::Digest;
use jrsonnet_evaluator::{error::Result, Val};
use jrsonnet_macros::builtin;
use jrsonnet_parser::ExprLocation;
use serde::{Deserialize, Serialize};

use crate::build::registry::RegistryData;
use crate::{anyhow, bail};

struct DeferRemoveTag(String);
impl Drop for DeferRemoveTag {
	fn drop(&mut self) {
		let v = Command::new("docker").arg("rmi").arg(&self.0).spawn();
		if v.is_err() {
			return;
		}
		let mut v = v.unwrap();
		let _ = v.wait();
	}
}

#[builtin(fields(
	data: RegistryData,
))]
pub fn docker_build(
	#[location] from: Option<&ExprLocation>,
	name: String,
	context: String,
	dockerfile: Option<String>,
) -> Result<String> {
	let mut full_context = from.unwrap().0.to_path_buf();
	full_context.pop();
	full_context.push(context);

	let tag_id = uuid::Uuid::new_v4().to_string();
	let tag = format!("haya-docker:{tag_id}");
	let _cleanup = DeferRemoveTag(tag.clone());

	{
		let mut cmd = Command::new("docker");
		cmd.env("DOCKER_BUILDKIT", "1")
			.arg("build")
			.arg(full_context)
			.arg("-t")
			.arg(&tag);

		if let Some(dockerfile) = dockerfile {
			let mut full_file = from.unwrap().0.to_path_buf();
			full_file.pop();
			full_file.push(dockerfile);

			cmd.arg("-f").arg(full_file);
		}

		let mut child = cmd
			.spawn()
			.map_err(|e| anyhow!("failed to build docker: {e}"))?;

		if let Err(e) = child.wait() {
			bail!("docker build failed: {e}")
		}
	}
	{
		let mut cmd = Command::new("docker");
		cmd.arg("inspect")
			.arg("--format='{{json .RootFS}}'")
			.arg(&tag)
			.stdout(Stdio::piped());

		let mut child = cmd
			.spawn()
			.map_err(|e| anyhow!("failed to inspect container: {e}"))?;

		let mut stdout = child.stdout.take().unwrap();
		let mut out = String::new();
		stdout
			.read_to_string(&mut out)
			.map_err(|e| anyhow!("failed to read inspect output: {e}"))?;

		let out = out.trim();
		let out = if out.starts_with("'") && out.ends_with("'") {
			&out[1..out.len() - 1]
		} else {
			&out
		};

		if let Err(e) = child.wait() {
			bail!("docker inspect failed: {e}")
		}

		#[derive(Deserialize, Serialize)]
		struct Layers {
			#[serde(rename = "Layers")]
			layers: Vec<String>,
		}
		let mut l: Layers = serde_json::from_str(&out)
			.map_err(|e| anyhow!("failed to parse docker inspect output: {e}\n{out}"))?;
		l.layers.sort();
		let normalized = serde_json::to_string(&l).unwrap();

		let mut hash = Blake2s256::new();
		hash.update(normalized.as_bytes());
		let hash = hash.finalize();
		let hash = hex::encode(&hash);

		let tag = format!("name:{}", hash);

		let command = Command::new("docker").arg("push").arg(tag);

		// let mut hash = blake2::Blake2s256::new();
		// for layer in l.layers {}
		// dbg!(l.layers);
	}

	todo!();
}

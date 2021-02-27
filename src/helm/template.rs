// TODO: Link with helm maybe?

use std::fmt::Display;

use jrsonnet_evaluator::error::LocError;
use serde::Deserialize;
use serde_json::Value;
use serde_yaml::DeserializingQuirks;
use subprocess::{Exec, PopenError};
use thiserror::Error;

#[derive(Debug)]
pub struct IdentStr(String);
impl Display for IdentStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for line in self.0.lines() {
            writeln!(f, "\t{}", line)?;
        }

        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("templating failed:\n{0}")]
    TemplatingError(IdentStr),
    #[error("helm binary not found in path: {0}")]
    HelmBinaryNotFound(std::io::Error),
    #[error("failed to parse helm output: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("spawn error: {0}")]
    PopenError(PopenError),
}
impl From<Error> for jrsonnet_evaluator::error::Error {
    fn from(s: Error) -> Self {
        Self::RuntimeError(s.to_string().into())
    }
}
impl From<Error> for LocError {
    fn from(s: Error) -> Self {
        Self::new(s.into())
    }
}

pub fn template_helm(
    namespace: &str,
    name: &str,
    package: &str,
    values: &Value,
) -> std::result::Result<Vec<Value>, Error> {
    let json_capture = {
        Exec::cmd("helm")
            .arg("template")
            .arg(&name)
            .arg(&package)
            .arg("-n")
            .arg(&namespace)
            .arg("--values")
            .arg("-")
    }
    .stdin(serde_json::to_string(&values).unwrap().as_ref())
    .capture()
    .map_err(|e| match e {
        PopenError::IoError(io) => Error::HelmBinaryNotFound(io),
        e => Error::PopenError(e),
    })?;

    if !json_capture.success() {
        return Err(Error::TemplatingError(IdentStr(json_capture.stderr_str().to_owned())));
    }

    let mut objects = Vec::new();
    for value in serde_yaml::Deserializer::from_str_with_quirks(
        &json_capture.stdout_str(),
        DeserializingQuirks { old_octals: true },
    ) {
        objects.push(Value::deserialize(value)?);
    }
    Ok(objects)
}

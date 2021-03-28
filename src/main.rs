mod apply;
mod helm;

use chrono::{SecondsFormat, Utc};
use clap::Clap;
use helm::create_helm_template;
use jrsonnet_cli::{ConfigureState, GeneralOpts, InputOpts};
use jrsonnet_evaluator::{error::Result, LazyBinding, LazyVal, ObjMember, ObjValue};
use jrsonnet_evaluator::{EvaluationState, Val};
use jrsonnet_interner::IStr;
use kube::Config;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    convert::{TryFrom, TryInto},
    path::PathBuf,
    rc::Rc,
};
use tokio::runtime::Builder;

#[macro_export]
macro_rules! bail {
    ($($err: tt)*) => {
        return Err(anyhow::anyhow!($($err)*).into())
    };
}

macro_rules! anyhow {
    ($($err: tt)*) => {
        jrsonnet_evaluator::error::LocError::from(anyhow::anyhow!($($err)*))
    };
}

#[macro_export]
macro_rules! unwrap {
    ($value: expr, $err: tt) => {
        match $value {
            Some(v) => v,
            None => bail!($err),
        }
    };
}

#[derive(Clap)]
#[clap(help_heading = "DEPLOY")]
struct DeployOpts {
    /// Set deployment name
    /// It is used for Gc (pruning), server-side apply, and as default namespace name
    name: String,
    /// Remove objects which present in apiserver, but missing in templated array
    #[clap(long)]
    prune: bool,
    /// Ignore changes applied by specified controllers
    #[clap(long)]
    ignore_changes_by: Vec<String>,
}

#[derive(Clap)]
#[clap(version = "0.1.0", author = "Lach")]
struct Opts {
    #[clap(flatten)]
    deploy: DeployOpts,
    #[clap(flatten)]
    jsonnet: GeneralOpts,
    #[clap(flatten)]
    input: InputOpts,
}

fn flatten(val: Val, out: &mut Vec<Val>) -> Result<()> {
    match val {
        Val::Arr(a) => {
            for (idx, item) in a.iter().enumerate() {
                jrsonnet_evaluator::push_stack_frame(
                    None,
                    || format!("[{}]", idx),
                    || {
                        flatten(item?, out)?;
                        Ok(())
                    },
                )?;
            }
        }
        Val::Obj(obj) => {
            let vis = obj.fields_visibility();
            if vis.get(&IStr::from("kind")).is_some()
                && vis.get(&IStr::from("apiVersion")).is_some()
            {
                out.push(Val::Obj(obj));
            } else {
                for field in vis {
                    jrsonnet_evaluator::push_stack_frame(
                        None,
                        || format!(".{}", field.0),
                        || {
                            flatten(obj.get(field.0.clone())?.unwrap(), out)?;
                            Ok(())
                        },
                    )?;
                }
            }
        }
        _ => bail!(
            "top level objects should be either arrays or objects, got {}",
            val.value_type(),
        ),
    }

    Ok(())
}

fn main_template(
    evaluator: EvaluationState,
    opts: &GeneralOpts,
    input: &InputOpts,
) -> Result<Vec<Value>> {
    opts.configure(&evaluator).unwrap();

    let value = evaluator.evaluate_file_raw(&PathBuf::from(&input.input))?;
    let value = evaluator.with_tla(value)?;
    let mut out = Vec::new();

    evaluator.run_in_state(|| flatten(value, &mut out))?;

    let mut json_out = Vec::new();
    evaluator.run_in_state(|| {
        for value in out {
            json_out.push((&value).try_into()?);
        }
        Ok(()) as jrsonnet_evaluator::error::Result<()>
    })?;

    Ok(json_out)
}

#[derive(Serialize, Deserialize)]
struct UnstructuredMetadata {
    name: String,
    namespace: Option<String>,
}

async fn main_real() -> Result<()> {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info");
    }

    env_logger::init();
    let opts: Opts = Opts::parse();

    let mut config = Config::infer()
        .await
        .map_err(|e| anyhow!("failed to load config: {}", e))?;
    config.default_ns = opts.deploy.name.clone();
    let client =
        kube::Client::try_from(config).map_err(|e| anyhow!("failed to construct client: {}", e))?;

    let es = EvaluationState::default();
    es.with_stdlib();
    let deployment_obj = ObjValue::new_empty()
        .extend_with_field(
            "name".into(),
            ObjMember {
                add: false,
                visibility: jrsonnet_parser::Visibility::Normal,
                invoke: LazyBinding::Bound(LazyVal::new_resolved(Val::Str(
                    opts.deploy.name.clone().into(),
                ))),
                location: None,
            },
        )
        .extend_with_field(
            "deployedAt".into(),
            ObjMember {
                add: false,
                visibility: jrsonnet_parser::Visibility::Normal,
                invoke: LazyBinding::Bound(LazyVal::new_resolved(Val::Str({
                    let utc = Utc::now()
                        .to_rfc3339_opts(SecondsFormat::Millis, true)
                        .replace(|c| c == 'T' || c == ':' || c == '.', "-");
                    (&utc[..utc.len() - 1]).into()
                }))),
                location: None,
            },
        );

    let haya_obj = ObjValue::new_empty().extend_with_field(
        "deployment".into(),
        ObjMember {
            add: false,
            visibility: jrsonnet_parser::Visibility::Normal,
            invoke: LazyBinding::Bound(LazyVal::new_resolved(Val::Obj(deployment_obj))),
            location: None,
        },
    );

    es.settings_mut()
        .globals
        .insert("_".into(), Val::Obj(haya_obj));

    es.add_native(
        "kubers.helmTemplate".into(),
        Rc::new(create_helm_template(opts.deploy.name.clone().into())),
    );

    let kubers_obj = es.evaluate_snippet_raw(
        Rc::new(PathBuf::from("kubers prelude")),
        include_str!("kubersApi.jsonnet").into(),
    )?;
    es.settings_mut()
        .globals
        .insert("hayasaka".into(), kubers_obj);
    let templated = match es.run_in_state(|| main_template(es.clone(), &opts.jsonnet, &opts.input))
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", es.stringify_err(&e));
            std::process::exit(1);
        }
    };

    let legacy_manager = format!("hayasaka.lach.pw/{}", opts.deploy.name);
    match apply::apply_multi(
        client,
        &opts.deploy.name,
        &format!("hayasaka.delta.rocks/{}", opts.deploy.name),
        ("hayasaka.delta.rocks", &opts.deploy.name),
        templated,
        |obj, manager, path| {
            if manager == legacy_manager {
                log::warn!("upgrading hayasaka version in {}", obj);
                return apply::ResolutionStrategy::Force;
            }
            if manager == "k3s" || opts.deploy.ignore_changes_by.contains(&manager.to_owned()) {
                log::warn!(
                    "using changes at {} in {} (made by {})",
                    fieldpath::PathBuf(path.to_owned()),
                    obj,
                    manager,
                );
                return apply::ResolutionStrategy::Ignore;
            }
            apply::ResolutionStrategy::Error(format!(
                "conflict with {} in {} at {}",
                manager,
                obj,
                fieldpath::PathBuf(path.to_owned())
            ))
        },
        true,
    )
    .await
    .map_err(anyhow::Error::from)
    {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

fn main_tokio() {
    Builder::new_current_thread()
        .enable_time()
        .enable_io()
        .build()
        .unwrap()
        .block_on(main_real())
        .unwrap();
}

fn main() {
    std::thread::Builder::new()
        .stack_size(500 * 1024 * 1024)
        .spawn(main_tokio)
        .unwrap()
        .join()
        .unwrap();
}

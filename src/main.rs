#![feature(bindings_after_at)]

mod apply;
mod helm;

use clap::Clap;
use helm::create_helm_template;
use jrsonnet_cli::{ConfigureState, GeneralOpts, InputOpts};
use jrsonnet_evaluator::error::Result;
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
    /// Add value for kubers label
    #[clap(long, short = 'n')]
    namespace: String,
    /// Remove objects which present in apiserver, but missing in templated array
    #[clap(long)]
    prune: bool,
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
            for item in a.iter() {
                let val = item.unwrap();
                flatten(val, out)?;
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
                    flatten(obj.get(field.0)?.unwrap(), out)?;
                }
            }
        }
        _ => unreachable!(),
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

    evaluator.run_in_state(|| {
        flatten(value, &mut out).unwrap();
    });
    let mut json_out = Vec::new();
    evaluator.run_in_state(|| {
        for value in out {
            json_out.push((&value).try_into().unwrap());
        }
    });

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
    config.default_ns = opts.deploy.namespace.clone();
    let client =
        kube::Client::try_from(config).map_err(|e| anyhow!("failed to construct client: {}", e))?;

    let es = EvaluationState::default();
    es.with_stdlib();
    es.add_native(
        "kubers.helmTemplate".into(),
        Rc::new(create_helm_template(opts.deploy.namespace.clone().into())),
    );

    let kubers_obj = es.evaluate_snippet_raw(
        Rc::new(PathBuf::from("kubers prelude")),
        include_str!("kubersApi.jsonnet").into(),
    )?;
    es.settings_mut()
        .globals
        .insert("kubers".into(), kubers_obj);
    let templated = match es.run_in_state(|| main_template(es.clone(), &opts.jsonnet, &opts.input))
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", es.stringify_err(&e));
            std::process::exit(1);
        }
    };

    apply::apply_multi(
        client,
        &opts.deploy.namespace,
        &format!("hayasaka.delta.rocks/{}", opts.deploy.namespace),
        ("hayasaka.delta.rocks", &opts.deploy.namespace),
        templated,
        |obj, manager, path| {
            log::warn!(
                "conflict with {} in {} at {}, ignoring",
                manager,
                obj,
                fieldpath::PathBuf(path.to_owned())
            );
            apply::ResolutionStrategy::Ignore
        },
        true,
    )
    .await
    .map_err(anyhow::Error::from)?;

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

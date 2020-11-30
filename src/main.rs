#![feature(async_closure)]

mod find;
mod kubemodel;

use clap::Clap;
use http::{Request, Response};
use jrsonnet_cli::{ConfigureState, GeneralOpts, InputOpts};
use jrsonnet_evaluator::{EvaluationState, ObjValue, Val};
use k8s_openapi::{
	apimachinery::pkg::apis::meta::v1::{APIGroup, APIGroupList, APIResourceList, ListMeta},
	List,
};
use kube::{
	api::{Api, ListParams, Meta, PatchParams, PatchStrategy, WatchEvent},
	Client, Resource,
};
use kubemodel::{ObjectId, ObjectType};
use quick_error::quick_error;
use std::{path::PathBuf, rc::Rc};

quick_error! {
	#[derive(Debug)]
	pub enum Error {
		JrsonnetError(err: jrsonnet_evaluator::error::LocError) {
			from()
		}
		MissingFieldError(err: String) {}
	}
}
type Result<T> = std::result::Result<T, Error>;

#[derive(Clap)]
#[clap(version = "0.1.0", author = "Lach")]
struct Opts {
	#[clap(flatten)]
	jsonnet: GeneralOpts,
	#[clap(flatten)]
	input: InputOpts,
	// JsonnetArgs
	// #[clap(subcommand)]
	// sub: SubCommand,
}

trait Unstructured {
	fn is_list(&self) -> Result<bool>;
	fn obj_type(&self) -> Result<ObjectType>;
	fn id(&self) -> Result<ObjectId>;
}

impl Unstructured for ObjValue {
	fn is_list(&self) -> Result<bool> {
		// This check is enought
		// https://github.com/kubernetes/apimachinery/blob/master/pkg/apis/meta/v1/unstructured/unstructured.go#L54
		Ok(self.get("items".into())?.is_some())
	}
	fn obj_type(&self) -> Result<ObjectType> {
		let api_version = self
			.get("apiVersion".into())?
			.ok_or_else(|| Error::MissingFieldError("apiVersion".into()))?
			.try_cast_str("kind should be string")?;
		let kind = self
			.get("kind".into())?
			.ok_or_else(|| Error::MissingFieldError("kind".into()))?
			.try_cast_str("kind should be string")?;
		Ok(ObjectType { api_version, kind })
	}
	fn id(&self) -> Result<ObjectId> {
		// let name =
		let metadata = self
			.get("metadata".into())?
			.ok_or_else(|| Error::MissingFieldError("metadata".into()))?
			.try_cast_obj("metadata should be object")?;
		let name = metadata
			.get("name".into())?
			.ok_or_else(|| Error::MissingFieldError("name".into()))?
			.try_cast_str("name")?;
		let namespace = metadata.get("name".into())?.map(|v| v.try_cast_str("name"));
		if let Some(namespace) = namespace {
			Ok(ObjectId {
				ty: self.obj_type()?,
				name,
				namespace: Some(namespace?),
			})
		} else {
			Ok(ObjectId {
				ty: self.obj_type()?,
				name,
				namespace: None,
			})
		}
	}
}

pub fn flatten(items: ObjValue) -> Result<Vec<ObjValue>> {
	let mut out = vec![];
	for key in items.visible_fields().iter().cloned() {
		let item = items
			.get(key)?
			.unwrap()
			.try_cast_obj("top level object should have objects as keys")?;
		if item.is_list()? {
			let items = item
				.get("items".into())?
				.unwrap()
				.try_cast_array("list object items should be array")?;
			for item in items.iter() {
				out.push(
					item.clone()
						.try_cast_obj("list object items elements should be object")?,
				);
			}
		} else {
			out.push(item);
		}
	}
	Ok(out)
}

#[tokio::main]
async fn main() -> Result<()> {
	use slog::{o, Drain};
	let decorator = slog_term::TermDecorator::new().build();
	let drain = slog_term::FullFormat::new(decorator).build().fuse();
	let drain = slog_async::Async::new(drain).build().fuse();

	let log = slog::Logger::root(drain, o!());

	let opts: Opts = Opts::parse();

	let evaluator = EvaluationState::default();
	opts.jsonnet.configure(&evaluator).unwrap();

	let value = evaluator
		.evaluate_file_raw(&PathBuf::from(opts.input.input))
		.unwrap();
	let value = evaluator.with_tla(value).unwrap();

	evaluator.run_in_state(|| {
		let obj = value.try_cast_obj("Top level output should be object")?;
		let items = flatten(obj)?;
		for item in items {
			println!("Type = {:?}", item.id()?);
			// println!("{}",)
		}
		Ok(()) as Result<()>
	})?;
	// opts.input.evaluate()
	// match opts.sub {
	// 	SubCommand::Test(cmd) => {
	// 		slog::info!(log, "Searching for already deployed");
	// 		let client = Client::try_default().await?;
	// 		let labelled =
	// 			find::find_all_labeled_items(log.clone(), client, "test".to_owned()).await?;
	// 		// println!("{:?}", labelled);
	// 		slog::info!(log, "Found {} already deployed resources", labelled.len());
	// 		// let apiGroups: Api<APIGroup> = Api::all(client);
	// 		// let list = apiGroups.list().await?;
	// 	}
	// }
	Ok(())
}

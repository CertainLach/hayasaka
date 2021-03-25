mod find;
mod parse;

use fieldpath::{path, Element, FieldpathExt, Path, PathBuf};
use find::{Object, ObjectKind, RuntimeTypeData};
use kube::{api::DeleteParams, Client};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use thiserror::Error;

/// How to deal with conflict
pub enum ResolutionStrategy {
    /// Ignore change, field will be left as-is
    Ignore,
    /// Ignore change, but add current manager as shared owner of field
    Share,
    /// Force change, override field value, become owner
    Force,
    /// Cancel apply, return error
    Error(String),
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to parse object: {0}")]
    ObjectParseFailed(serde_json::Error),
    #[error("unknown object kind: {0}")]
    UnknownObjectKind(ObjectKind),
    #[error("conflict resolution failed: {0}")]
    ConflictResolverError(String),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("kube error: {0}")]
    Kube(#[from] kube::Error),
    #[error("path error: {0}")]
    Path(#[from] fieldpath::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

fn make_url(namespace: &str, object: &Object, types: &RuntimeTypeData) -> String {
    let ns_prefix = object
        .metadata
        .namespace
        .clone()
        .or_else(|| {
            if types.get(&object.kind).unwrap().namespaced {
                Some(namespace.to_owned())
            } else {
                None
            }
        })
        .map(|ns| format!("namespaces/{}/", ns))
        .unwrap_or(String::new());

    format!(
        "/{prefix}/{group_version}/{ns_prefix}{kind}/{name}",
        prefix = if object.kind.api_version.contains('/') {
            "apis"
        } else {
            "api"
        },
        group_version = object.kind.api_version,
        ns_prefix = ns_prefix,
        kind = types.get(&object.kind).unwrap().plural,
        name = object.metadata.name,
    )
}

/// Perform dry-run with conflict resolution
async fn apply_internal_resolve_conflicts(
    client: Client,
    namespace: &str,
    manager: &str,
    target: &mut Value,
    types: &RuntimeTypeData,
    conflict_resolver: impl Fn(&str, &Path) -> ResolutionStrategy,
) -> Result<()> {
    let object: Object = serde_json::from_value(target.clone())?;

    log::trace!("Dry-run apply for {}", object);

    let base_url = make_url(namespace, &object, types);
    // dry run
    let dry_run_base_url = format!("{}?fieldManager={}&dryRun=All", base_url, manager,);

    let get_req = http::Request::get(&base_url)
        .header("Accept", "application/json")
        .body(vec![])
        .map_err(kube::Error::HttpError)?;

    let patch_req = http::Request::patch(&dry_run_base_url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/apply-patch+yaml")
        .body(serde_json::to_vec(&target)?)
        .map_err(kube::Error::HttpError)?;

    log::trace!("Loading current obj version");
    let old_obj: Option<_> = match client.request(get_req).await {
        Ok(v) => Some(v),
        Err(kube::Error::Api(apierror)) if apierror.code == 404 => None,
        Err(e) => return Err(e.into()),
    }
    .map(|mut v: Value| {
        for path in [
            path!(."metadata"."managedFields"),
            path!(."metadata"."selfLink"),
            path!(."metadata"."uid"),
            path!(."metadata"."resourceVersion"),
            path!(."metadata"."generation"),
            path!(."metadata"."creationTimestamp"),
            path!(."metadata"."annotations.kubectl.kubernetes.io/last-applied-configuration"),
            path!(."status"),
        ]
        .iter()
        {
            let _res = v.remove_path(path);
        }
        v
    });

    log::trace!("= {}", serde_json::to_string_pretty(&old_obj).unwrap());

    log::trace!("Running dry-run");
    match client.request(patch_req).await {
        Ok(v) => {
            let _result: Value = v;
            return Ok(());
        }
        Err(kube::Error::Api(apierror)) if apierror.code == 409 => {
            let mut removed_paths = Vec::<PathBuf>::new();
            log::warn!("{}", apierror.message);
            for conflict in parse::conflict_error_parser::message(&apierror.message).unwrap() {
                for path in conflict.1 {
                    if removed_paths
                        .iter()
                        .any(|removed| path.starts_with(removed) || &path == removed)
                    {
                        log::trace!("Path was already removed {}", path);
                        // this path was already removed during earlier conflict resolution,
                        // can't do anything more with it
                        continue;
                    }
                    log::trace!("Handling conflict with {} at {}", conflict.0, path);
                    match conflict_resolver(&conflict.0, &path) {
                        ResolutionStrategy::Ignore => {
                            log::trace!("- Ignoring");
                            let mut path: &[Element] = &path;
                            if path.ends_with(&[Element::Field("value".to_owned())])
                                && !target.has_path(&path)
                            {
                                path = &path[0..path.len() - 1];
                            }
                            target.remove_path(&path)?;
                            removed_paths.push(path.into());
                        }
                        ResolutionStrategy::Share => {
                            log::trace!("- Sharing");
                            unimplemented!("not needed for hayasaka")
                        }
                        ResolutionStrategy::Force => log::trace!("- Forcing"),
                        ResolutionStrategy::Error(e) => {
                            log::trace!("- Erroring with {}", e);
                            return Err(Error::ConflictResolverError(e));
                        }
                    }
                }
            }
            Ok(())
        }
        Err(e) => return Err(e.into()),
    }
}

async fn apply_internal_force(
    client: Client,
    namespace: &str,
    manager: &str,
    target: Value,
    types: &RuntimeTypeData,
) -> Result<()> {
    let object: Object = serde_json::from_value(target.clone())?;

    // force run
    let force_base_url = format!(
        "{}?fieldManager={}&force=true",
        make_url(namespace, &object, types),
        manager,
    );

    let req = http::Request::patch(&force_base_url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/apply-patch+yaml")
        .body(serde_json::to_vec(&target)?)
        .map_err(kube::Error::HttpError)?;

    let _result: Value = client.request(req).await?;
    Ok(())
}

async fn remove(client: Client, object: &Object, types: &RuntimeTypeData) -> Result<()> {
    let url = format!("{}", make_url("", object, types));

    let req = http::Request::delete(&url)
        .header("Accept", "application/json")
        .body(
            serde_json::to_vec(&DeleteParams {
                grace_period_seconds: Some(0),
                ..Default::default()
            })
            .unwrap(),
        )
        .map_err(kube::Error::HttpError)?;

    let _result: Value = client.request(req).await?;
    Ok(())
}

pub async fn apply_multi(
    client: Client,
    namespace: &str,
    manager: &str,
    label: (&str, &str),
    mut target: Vec<Value>,
    conflict_resolver: impl Fn(&Object, &str, &Path) -> ResolutionStrategy,

    prune: bool,
) -> Result<()> {
    let types = find::list_apis(client.clone()).await?;

    let mut created = BTreeSet::new();

    for item in target.iter_mut() {
        let unstructured: Object =
            serde_json::from_value(item.clone()).map_err(Error::ObjectParseFailed)?;

        {
            // metadata field should exist, this field is already used while parsing object
            let metadata = item["metadata"].as_object_mut().unwrap();

            if !metadata.contains_key("labels") {
                metadata.insert("labels".to_owned(), json!({}));
            }
            let labels = metadata["labels"].as_object_mut().unwrap();

            labels.insert(label.0.to_owned(), json!(label.1));

            if !types.contains_key(&unstructured.kind) {
                return Err(Error::UnknownObjectKind(unstructured.kind));
            }
            if types.get(&unstructured.kind).unwrap().namespaced
                && !metadata.contains_key("namespace")
            {
                metadata.insert("namespace".to_owned(), json!(namespace));
            }
        };
        let unstructured: Object =
            serde_json::from_value(item.clone()).map_err(Error::ObjectParseFailed)?;

        apply_internal_resolve_conflicts(
            client.clone(),
            &namespace,
            &manager,
            item,
            &types,
            |manager, path| conflict_resolver(&unstructured, manager, path),
        )
        .await?;

        created.insert(unstructured);
    }

    for item in target {
        apply_internal_force(client.clone(), &namespace, &manager, item, &types).await?;
    }

    if prune {
        let found = find::find_all_labeled_items(client.clone(), label).await?;
        let to_remove = found.difference(&created);

        for item in to_remove {
            // Endpoints copies Service labels
            if item.kind.api_version == "v1" && item.kind.kind == "Endpoints" {
                continue;
            } else if item.kind.api_version == "discovery.k8s.io/v1beta1" && item.kind.kind == "EndpointSlice" {
                continue;
            }

            log::warn!("pruning {}", item);
            remove(client.clone(), &item, &types).await?
        }
    }

    Ok(())
}

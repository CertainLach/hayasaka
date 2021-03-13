use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Display},
};

use http::Request;
use kube::Client;
use serde::Deserialize;

pub type RuntimeTypeData = BTreeMap<ObjectKind, ObjectData>;

/// Represents object runtime type
#[derive(Clone, Debug, Deserialize, Eq)]
pub struct ObjectKind {
    // extensions/v1
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    // CustomResourceDefinition
    pub kind: String,
}
impl ObjectKind {
    fn versionless_version(&self) -> &str {
        let index = self
            .api_version
            .find("/")
            .unwrap_or_else(|| self.api_version.len());
        let api_version = &self.api_version[0..index];
        if api_version == "extensions" && self.kind == "Ingress" {
            return "networking.k8s.io";
        }
        api_version
    }
}

impl PartialEq for ObjectKind {
    fn eq(&self, other: &Self) -> bool {
        self.versionless_version() == other.versionless_version() && self.kind == other.kind
    }
}
impl PartialOrd for ObjectKind {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(
            self.versionless_version()
                .cmp(&other.versionless_version())
                .then_with(|| self.kind.cmp(&other.kind)),
        )
    }
}
impl Ord for ObjectKind {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Display for ObjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.api_version, self.kind)
    }
}

/// Represents object location
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct ObjectLocation {
    pub name: String,
    pub namespace: Option<String>,
}

impl Display for ObjectLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(ns) = &self.namespace {
            write!(f, " in {}", ns)?;
        }
        Ok(())
    }
}

/// Represents unique object
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct Object {
    #[serde(flatten)]
    pub kind: ObjectKind,
    pub metadata: ObjectLocation,
}

impl Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.kind, self.metadata)
    }
}

/// Represents object list item
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct ObjectListItem {
    pub metadata: ObjectLocation,
}

/// Represents object list
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
struct ObjectList {
    pub items: Vec<ObjectListItem>,
}

/// Represents object kind metadata
pub struct ObjectData {
    pub namespaced: bool,
    pub is_core: bool,
    pub plural: String,
}

/// List all defined object kinds with additional meta
pub async fn list_apis(client: Client) -> super::Result<RuntimeTypeData> {
    let mut out = BTreeMap::new();

    for version in client.list_core_api_versions().await?.versions {
        for resource in client.list_core_api_resources(&version).await?.resources {
            if resource.name.find("/").is_some() {
                continue;
            }
            out.insert(
                ObjectKind {
                    api_version: version.clone(),
                    kind: resource.kind,
                },
                ObjectData {
                    namespaced: resource.namespaced,
                    plural: resource.name.clone(),
                    is_core: true,
                },
            );
        }
    }

    for group in client.list_api_groups().await?.groups {
        for version in group.versions {
            for resource in client
                .list_api_group_resources(&version.group_version)
                .await?
                .resources
            {
                if resource.name.contains("/") {
                    continue;
                }
                out.insert(
                    ObjectKind {
                        api_version: version.group_version.clone(),
                        kind: resource.kind.clone(),
                    },
                    ObjectData {
                        namespaced: resource.namespaced,
                        plural: resource.name.clone(),
                        is_core: false,
                    },
                );
            }
        }
    }

    Ok(out)
}

/// Find all objects, which matches given label selector
pub async fn find_all_labeled_items(
    client: Client,
    label: (&str, &str),
) -> Result<BTreeSet<Object>, anyhow::Error> {
    let mut out = BTreeSet::new();
    let label_selector = format!("{}={}", label.0, label.1);

    for version in client.list_core_api_versions().await?.versions {
        let client = client.clone();
        for resource in client.list_core_api_resources(&version).await?.resources {
            if resource.name.find("/").is_some() {
                continue;
            }
            let data = client
                .request::<ObjectList>(
                    Request::builder()
                        .uri(&format!(
                            "/api/{}/{}?labelSelector={}",
                            version, resource.name, label_selector,
                        ))
                        .body(vec![])
                        .map_err(kube::Error::HttpError)?,
                )
                .await;
            if let Ok(data) = data {
                for object in data.items {
                    out.insert(Object {
                        kind: ObjectKind {
                            api_version: version.clone(),
                            kind: resource.kind.clone(),
                        },
                        metadata: object.metadata,
                    });
                }
            } else if !(version == "v1" && resource.name == "bindings") {
                log::warn!(
                    "No access, assuming there should be no {} {} deployed",
                    version,
                    resource.name
                );
            }
        }
    }
    for group in client.list_api_groups().await?.groups {
        let version = group
            .preferred_version
            .as_ref()
            .unwrap_or_else(|| group.versions.last().unwrap());
        for resource in client
            .list_api_group_resources(&version.group_version)
            .await?
            .resources
        {
            if resource.name.contains('/') {
                continue;
            }
            let data = client
                .request::<ObjectList>(
                    Request::builder()
                        .uri(&format!(
                            "/apis/{}/{}?labelSelector={}",
                            version.group_version, resource.name, label_selector,
                        ))
                        .body(vec![])
                        .map_err(kube::Error::HttpError)?,
                )
                .await;
            if let Ok(data) = data {
                for object in data.items {
                    out.insert(Object {
                        kind: ObjectKind {
                            api_version: version.group_version.clone(),
                            kind: resource.kind.clone(),
                        },
                        metadata: object.metadata,
                    });
                }
            } else if !(group.name == "authentication.k8s.io"
                || group.name == "authorization.k8s.io")
            {
                log::warn!(
                    "No access, assuming there should be no {} {} deployed",
                    group.name,
                    resource.name
                );
            }
        }
    }
    Ok(out)
}

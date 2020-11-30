use http::Request;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{APIGroupList, APIResourceList};
use kube::Client;
use serde::Deserialize;
use std::error::Error;

#[derive(Debug, Deserialize)]
struct AnyResourceMeta {
	name: String,
	namespace: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnyResource {
	metadata: AnyResourceMeta,
}

#[derive(Debug, Deserialize)]
struct AnyResourceList {
	items: Vec<AnyResource>,
}

#[derive(Clone, Debug)]
pub struct PruneTarget {
	group: String,
	version: Stringw,

	namespace: Option<String>,
	name: String,
}

pub async fn find_all_labeled_items(
	log: slog::Logger,
	client: Client,
	tag: String,
) -> Result<Vec<PruneTarget>, Box<dyn Error>> {
	let mut tasks = vec![];
	let data = client
		.request::<APIGroupList>(Request::builder().uri("/apis/").body(vec![])?)
		.await?;
	// Groups processed in parallel
	for group in data.groups {
		let client = client.clone();
		let log = log.clone();
		let tag = tag.clone();
		tasks.push(tokio::spawn(async move {
			let mut out = vec![];
			for version in group.versions {
				let data = client
					.request::<APIResourceList>(
						Request::builder()
							.uri(&format!("/apis/{}", version.group_version))
							.body(vec![])
							.unwrap(),
					)
					.await
					.unwrap();
				for api_resource in data.resources {
					if api_resource.name.contains('/') {
						continue;
					}
					let data = client
						.request::<AnyResourceList>(
							Request::builder()
								.uri(&format!(
									"/apis/{}/{}?labelSelector=hayasaka.lach.pw/tag={}",
									version.group_version, api_resource.name, tag,
								))
								.body(vec![])?,
						)
						.await;
					if let Ok(data) = data {
						for resource in data.items {
							out.push(PruneTarget {
								group: group.name.clone(),
								version: version.version.clone(),

								namespace: resource.metadata.namespace,
								name: resource.metadata.name,
							});
						}
					} else if !(group.name == "authentication.k8s.io"
						|| group.name == "authorization.k8s.io")
					{
						slog::warn!(
							log,
							"No access, assuming there should be no {} {} deployed",
							group.name,
							api_resource.name
						);
					}
				}
			}
			Ok(out) as Result<_, http::Error>
		}));
	}
	let results = futures::future::join_all(tasks)
		.await
		.into_iter()
		.collect::<Result<Vec<_>, _>>()?
		.into_iter()
		.collect::<Result<Vec<_>, _>>()?
		.concat();
	Ok(results)
}

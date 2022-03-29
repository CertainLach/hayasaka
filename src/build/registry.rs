use gcmodule::Trace;

#[derive(Clone, Trace)]
pub struct RegistryData {
	/// Used as image prefix
	host: String,
	/// Used by hayasaka to push image
	push_config: String,
	/// Used by kubernetes to pull image
	pull_config: String,
}

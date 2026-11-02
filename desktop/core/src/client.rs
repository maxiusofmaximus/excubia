mod errors;
mod host;
mod pairing;
mod validate;

pub(crate) use errors::{expect_json, status_error};
pub use host::{
    normalize_base_url, normalize_endpoint, CatalogAgent, HostClient, WorkspaceCreated,
};
pub use pairing::{pair_device, PairResponse};
pub use validate::{validate_device_id, validate_pane_id};

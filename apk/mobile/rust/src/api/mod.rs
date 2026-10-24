pub mod approvals;
pub mod catalog;
pub mod chat;
pub mod error;
pub mod pinned;
pub mod policy;
pub mod signing;
pub mod state;
pub mod stream;
mod validate;
pub mod workspace;
pub mod write;

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}

/// The crate version, shown under "Avanzado" as proof that the Rust core is
/// the code actually running on the phone.
#[flutter_rust_bridge::frb(sync)]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

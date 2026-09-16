//! Internal Pactrun architecture facade.
//!
//! This crate does not expose a stable library API. Modules remain crate-private
//! until real contracts justify a narrower interface.

mod application;
mod authoring;
mod cli;
mod domain;
mod executor;
mod hook;
mod managed_data;
mod opened_files;
mod output_publication;
mod persistence;
mod retirement_fs;
mod revision_content;
mod revision_core_v1;
mod revision_core_v2;
mod service_storage;
mod snapshot_bundle;
mod snapshot_integrity;
mod strict_json;
mod workflow;

/// Internal binary composition entry point. This is deliberately not a stable
/// Pactrun Rust API; the library target exists to preserve the modular crate
/// topology and test boundaries.
#[doc(hidden)]
pub fn run_cli_from_env() -> i32 {
    cli::run_from_env()
}

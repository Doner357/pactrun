//! Core domain model and invariant ownership.
//!
//! This module must not depend on CLI, persistence, or third-party adapter types.

#![allow(dead_code)]

mod catalog;
pub(crate) use catalog::*;
mod deletion;
mod diagnostics;
pub(crate) use diagnostics::*;
mod error;
mod execution;
mod identity;
mod local_names;
#[allow(unused_imports)]
pub(crate) use local_names::*;
mod managed_input;
mod migration;
mod migration_paths;
mod object_lifecycle;
mod revision_content;
mod revision_declarations;
mod revision_metadata;
mod run;
mod service_migration;
mod service_storage;
mod service_storage_state;
mod snapshot;
mod snapshot_capability;
mod snapshot_execution;
mod versioning;
pub(crate) use versioning::*;

#[allow(unused_imports)]
pub(crate) use deletion::*;
pub(crate) use error::PactrunErrorRef;
#[allow(unused_imports)]
pub(crate) use execution::*;
#[allow(unused_imports)]
pub(crate) use identity::{
    InstanceId, InstanceStateVersion, ManagedInputPayloadId, PackageId, RevisionContentDigest,
    RevisionIdentity, RunId, ServiceAllocationId, SnapshotId,
};
#[allow(unused_imports)]
pub(crate) use managed_input::*;
#[allow(unused_imports)]
pub(crate) use migration::*;
#[allow(unused_imports)]
pub(crate) use migration_paths::*;
pub(crate) use object_lifecycle::*;
#[allow(unused_imports)]
pub(crate) use revision_content::*;
pub(crate) use revision_declarations::*;
#[allow(unused_imports)]
pub(crate) use revision_metadata::*;
#[allow(unused_imports)]
pub(crate) use run::*;
#[allow(unused_imports)]
pub(crate) use service_migration::*;
#[allow(unused_imports)]
pub(crate) use service_storage::*;
#[allow(unused_imports)]
pub(crate) use service_storage_state::*;
#[allow(unused_imports)]
pub(crate) use snapshot::*;
#[allow(unused_imports)]
pub(crate) use snapshot_capability::*;
pub(crate) use snapshot_execution::*;

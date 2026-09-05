//! Core domain model and invariant ownership.
//!
//! This module must not depend on CLI, persistence, or third-party adapter types.

// M1-A establishes production domain contracts before the application layer
// consumes them. Later milestones remove this transitional allowance as each
// contract gains a production caller.
#![allow(dead_code)]

mod error;
mod execution;
mod identity;
mod managed_input;
mod revision_core_v1;
mod revision_metadata;
mod run;

pub(crate) use error::PactrunErrorRefV1;
#[allow(unused_imports)]
pub(crate) use execution::*;
#[allow(unused_imports)]
pub(crate) use identity::{
    InstanceId, InstanceStateVersion, ManagedInputPayloadId, PackageId, RevisionContentDigest,
    RevisionIdentity, RunId,
};
#[allow(unused_imports)]
pub(crate) use managed_input::*;
pub(crate) use revision_core_v1::*;
#[allow(unused_imports)]
pub(crate) use revision_metadata::*;
#[allow(unused_imports)]
pub(crate) use run::*;

//! Persistence boundary and storage-adapter ownership.

// M1-B establishes the production storage contract before M1-C adds its first
// application caller.
#![allow(dead_code)]

mod chunked_blob;
mod runtime_content_store;
mod sqlite_instances;
mod sqlite_revision_metadata;
mod sqlite_revision_store;
mod sqlite_runs;

#[allow(unused_imports)]
pub(crate) use runtime_content_store::{
    RuntimeContentStore, RuntimeContentStoreError, StoredRuntimeBlob, VerifiedRuntimeBlob,
    validate_supported_storage_root,
};
#[allow(unused_imports)]
pub(crate) use sqlite_instances::ManagedInputWrite;
#[allow(unused_imports)]
pub(crate) use sqlite_revision_store::{
    PactrunPersistence, PersistenceError, StoredRevisionContentV1,
};
#[allow(unused_imports)]
pub(crate) use sqlite_runs::{RunArtifactWrite, RunFinishReceipt};

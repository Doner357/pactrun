//! Persistence boundary and storage-adapter ownership.

// M1-B establishes the production storage contract before M1-C adds its first
// application caller.
#![allow(dead_code)]

mod runtime_content_store;
mod sqlite_revision_store;

#[allow(unused_imports)]
pub(crate) use runtime_content_store::{
    RuntimeContentStore, RuntimeContentStoreError, StoredRuntimeBlob, VerifiedRuntimeBlob,
};
#[allow(unused_imports)]
pub(crate) use sqlite_revision_store::{
    PactrunPersistence, PersistenceError, StoredRevisionContentV1,
};

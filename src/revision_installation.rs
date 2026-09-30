//! Private, frontend-independent installation candidate after identity projection.
//! Neither YAML nor a container format owns this application boundary.
use crate::{
    domain::{PortableMetadataTemplate, RevisionIdentity, Sha256Digest, ValidatedRevisionContent},
    managed_data::StagedRuntimeSource,
};
use std::collections::BTreeMap;

pub(crate) struct PreparedRevision {
    pub(crate) identity: RevisionIdentity,
    pub(crate) content: ValidatedRevisionContent,
    pub(crate) metadata: PortableMetadataTemplate,
    pub(crate) blobs: BTreeMap<Sha256Digest, StagedRuntimeSource>,
}

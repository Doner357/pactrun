//! Data-facing use of the existing opaque immutable store. Never expose digests,
//! filesystem paths or underlying diagnostic text through Input/Snapshot errors.
use super::{PersistenceError, RuntimeContentStore, RuntimeContentStoreError, StoredRuntimeBlob};
use crate::domain::{Sha256Digest, SnapshotCapability};
use std::io::{self, Read, Write};

pub(super) fn error(error: RuntimeContentStoreError) -> PersistenceError {
    use RuntimeContentStoreError::*;
    match error {
        IncomingDigestMismatch { .. }
        | MissingBlob(_)
        | CorruptBlob { .. }
        | UnacceptableStoreEntry { .. } => {
            PersistenceError::CorruptSnapshot("immutable data content or representation is invalid")
        }
        Io { source, .. } | PersistenceBarrier { source, .. } | Lock { source, .. } => {
            PersistenceError::Io {
                operation: "access immutable managed data",
                source: io::Error::from(source.kind()),
            }
        }
        LengthOverflow => SnapshotCapability::StoredBlob
            .check(u64::MAX)
            .unwrap_err()
            .into(),
        LockPoisoned => PersistenceError::DatabaseLockPoisoned,
        UnsupportedStorageProfile(_) => PersistenceError::Io {
            operation: "access immutable managed data",
            source: io::ErrorKind::Unsupported.into(),
        },
        RandomStagingName(_) => PersistenceError::Io {
            operation: "stage immutable managed data",
            source: io::ErrorKind::Other.into(),
        },
    }
}

pub(super) fn publish(
    store: &RuntimeContentStore,
    digest: &Sha256Digest,
    length: u64,
    mut source: &mut dyn Read,
) -> Result<StoredRuntimeBlob, PersistenceError> {
    SnapshotCapability::StoredBlob.check(length)?;
    let published = store.put_verified(digest, &mut source).map_err(error)?;
    if published.byte_len() != length {
        return Err(PersistenceError::CorruptSnapshot(
            "immutable data length changed",
        ));
    }
    Ok(published)
}

pub(super) fn stream(
    store: &RuntimeContentStore,
    digest: &Sha256Digest,
    length: u64,
    target: &mut dyn Write,
) -> Result<(), PersistenceError> {
    let actual = store.stream_verified(digest, target).map_err(error)?;
    if actual != length {
        return Err(PersistenceError::CorruptSnapshot(
            "immutable data length mismatch",
        ));
    }
    Ok(())
}

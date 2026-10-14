//! Closed failure reasons: never interpolate backend messages or payload paths.
use super::{ApplicationError, CliError, io};
use crate::{persistence::PersistenceError, snapshot_integrity::SnapshotCodecError};

pub(super) fn before_publication(stage: &str, reason: &str) -> CliError {
    CliError::operation(format!("{stage}: {reason}. No report was published"))
}
pub(super) fn read(error: ApplicationError) -> CliError {
    let reason = match error {
        ApplicationError::Persistence(error) => persistence_reason(&error),
        ApplicationError::Io { source, .. } => io_reason(source.kind()),
        _ => "report sources could not be inspected safely; inspect the retained Run and Store",
    };
    before_publication("Run report could not be completed", reason)
}
pub(super) fn encode(error: PersistenceError) -> CliError {
    before_publication(
        "Run report could not be encoded",
        persistence_reason(&error),
    )
}
pub(super) fn io_reason(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::PermissionDenied => "access was denied; check permissions and file access",
        io::ErrorKind::StorageFull => "storage is full; free space before retrying",
        io::ErrorKind::NotFound => {
            "a required source or staging file is missing; inspect retained sources"
        }
        io::ErrorKind::InvalidData => {
            "data or report encoding is invalid; preserve the Store for inspection"
        }
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => {
            "I/O did not complete; check storage availability before retrying"
        }
        _ => "I/O failed; check source and staging storage without assuming data corruption",
    }
}
fn persistence_reason(error: &PersistenceError) -> &'static str {
    match error {
        PersistenceError::MissingRun(_) => {
            "the requested Run is unavailable; list retained Runs before retrying"
        }
        PersistenceError::UnauthorizedSecretExport => {
            "selected Secret bytes require separate export authorization; review the selection before authorizing"
        }
        PersistenceError::Io { source, .. } => io_reason(source.kind()),
        PersistenceError::CorruptManagedInput(_) => {
            "stored Input content failed validation; preserve the Store and inspect retained history"
        }
        PersistenceError::CorruptRun(_) | PersistenceError::InvalidRunArtifact(_) => {
            "retained Run or Artifact data failed validation; preserve the Store for inspection"
        }
        PersistenceError::CorruptSnapshot(_)
        | PersistenceError::SnapshotCodec(SnapshotCodecError::Validation(_)) => {
            "stored Snapshot content failed validation; preserve the Snapshot for inspection"
        }
        PersistenceError::SnapshotCodec(SnapshotCodecError::ContentIo(kind)) => io_reason(*kind),
        PersistenceError::SnapshotCodec(SnapshotCodecError::Capability(_)) => {
            "Snapshot content exceeds this build's capacity; verification is incomplete"
        }
        PersistenceError::SnapshotCodec(SnapshotCodecError::UnsupportedVersion(_)) => {
            "Snapshot format is unsupported; use an explicitly supporting release"
        }
        PersistenceError::SchemaMismatch(_) | PersistenceError::DatabaseOwnership(_) => {
            "Store format or ownership is unsupported; preserve the Store for inspection"
        }
        PersistenceError::Sqlite { source, .. } => match source.sqlite_error_code() {
            Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
                "Store is busy; retry after other Store operations finish"
            }
            Some(rusqlite::ErrorCode::DiskFull) => io_reason(io::ErrorKind::StorageFull),
            Some(rusqlite::ErrorCode::PermissionDenied | rusqlite::ErrorCode::ReadOnly) => {
                io_reason(io::ErrorKind::PermissionDenied)
            }
            Some(rusqlite::ErrorCode::DatabaseCorrupt) => {
                "Store database failed integrity checks; preserve it for inspection"
            }
            Some(rusqlite::ErrorCode::NotADatabase) => {
                "Store database format could not be read; inspect its ownership without overwriting it"
            }
            _ => {
                "Store database access failed; inspect storage availability without assuming data corruption"
            }
        },
        _ => "report data could not be processed safely; inspect the retained Run and Store",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0794
    // Verifies: PR-REQ-0405, PR-REQ-0406
    #[test]
    fn report_failures_keep_actionable_typed_reasons_without_backend_text() {
        let private = "private-path-or-payload-sentinel";
        for (error, expected) in [
            (
                PersistenceError::CorruptManagedInput(private.into()),
                "Input content failed validation",
            ),
            (
                PersistenceError::CorruptRun(private.into()),
                "Run or Artifact data failed validation",
            ),
            (
                PersistenceError::Io {
                    operation: private,
                    source: io::Error::new(io::ErrorKind::PermissionDenied, private),
                },
                "access was denied",
            ),
            (
                PersistenceError::Io {
                    operation: private,
                    source: io::Error::new(io::ErrorKind::StorageFull, private),
                },
                "storage is full",
            ),
            (
                PersistenceError::Sqlite {
                    operation: private,
                    source: rusqlite::Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                        Some(private.into()),
                    ),
                },
                "Store is busy",
            ),
        ] {
            let message = read(ApplicationError::Persistence(error)).message;
            assert!(message.contains(expected), "{message}");
            assert!(message.contains("No report was published"));
            assert!(!message.contains(private));
        }
        let message = encode(PersistenceError::Io {
            operation: private,
            source: io::Error::new(io::ErrorKind::StorageFull, private),
        })
        .message;
        assert!(message.contains("could not be encoded") && message.contains("storage is full"));
        assert!(!message.contains(private));
    }
}

CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 5),
    PRIMARY KEY (owner_session)
) STRICT, WITHOUT ROWID;

CREATE TABLE run_operation_kinds (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    operation_kind INTEGER NOT NULL CHECK(operation_kind IN (0, 1, 2)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_capture_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_restore_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_recovery_consequence_versions (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    consequence_version INTEGER NOT NULL CHECK(consequence_version >= 0),
    PRIMARY KEY (instance_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE snapshots (
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    integrity_format INTEGER NOT NULL CHECK(integrity_format IN (1, 2)),
    integrity_digest BLOB NOT NULL CHECK(length(integrity_digest) = 32),
    canonical_manifest BLOB NOT NULL CHECK(length(canonical_manifest) > 0),
    PRIMARY KEY (snapshot_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE snapshot_blobs (
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    blob_digest BLOB NOT NULL CHECK(length(blob_digest) = 32),
    byte_length INTEGER NOT NULL CHECK(byte_length >= 0),
    PRIMARY KEY (snapshot_id, blob_digest),
    FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE snapshot_blob_chunks (
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    blob_digest BLOB NOT NULL CHECK(length(blob_digest) = 32),
    chunk_index INTEGER NOT NULL CHECK(chunk_index >= 0),
    chunk_bytes BLOB NOT NULL CHECK(length(chunk_bytes) BETWEEN 1 AND 1048576),
    PRIMARY KEY (snapshot_id, blob_digest, chunk_index),
    FOREIGN KEY (snapshot_id, blob_digest)
        REFERENCES snapshot_blobs(snapshot_id, blob_digest) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_restore_admissions (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    admitted_state_version BLOB NOT NULL CHECK(length(admitted_state_version) = 16),
    admitted_consequence_version INTEGER NOT NULL CHECK(admitted_consequence_version >= 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_executions(run_id) ON DELETE CASCADE,
    FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_capture_results (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

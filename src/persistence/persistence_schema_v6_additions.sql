DROP TABLE writable_admissions;
CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 6),
    PRIMARY KEY (owner_session)
) STRICT, WITHOUT ROWID;

CREATE TABLE v6_run_operation_kinds (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    operation_kind INTEGER NOT NULL CHECK(operation_kind IN (0, 1, 2, 3)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE v6_run_capture_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES v6_run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE v6_run_restore_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES v6_run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
INSERT INTO v6_run_operation_kinds SELECT * FROM run_operation_kinds;
INSERT INTO v6_run_capture_invocations SELECT * FROM run_capture_invocations;
INSERT INTO v6_run_restore_invocations SELECT * FROM run_restore_invocations;
DROP TABLE run_capture_invocations;
DROP TABLE run_restore_invocations;
DROP TABLE run_operation_kinds;
ALTER TABLE v6_run_operation_kinds RENAME TO run_operation_kinds;
ALTER TABLE v6_run_capture_invocations RENAME TO run_capture_invocations;
ALTER TABLE v6_run_restore_invocations RENAME TO run_restore_invocations;

CREATE TABLE run_migration_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    source_revision_digest BLOB NOT NULL CHECK(length(source_revision_digest) = 32),
    target_revision_digest BLOB NOT NULL CHECK(length(target_revision_digest) = 32),
    authorize_declassification INTEGER NOT NULL CHECK(authorize_declassification IN (0, 1)),
    PRIMARY KEY (run_id),
    CHECK(source_revision_digest <> target_revision_digest),
    FOREIGN KEY (run_id) REFERENCES run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE run_migration_edges (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    source_revision_digest BLOB NOT NULL CHECK(length(source_revision_digest) = 32),
    target_revision_digest BLOB NOT NULL CHECK(length(target_revision_digest) = 32),
    PRIMARY KEY (run_id, edge_index),
    CHECK(source_revision_digest <> target_revision_digest),
    FOREIGN KEY (run_id) REFERENCES run_migration_invocations(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE run_migration_progress (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    committed_edge_count INTEGER NOT NULL CHECK(committed_edge_count >= 0),
    step_rank INTEGER NOT NULL CHECK(step_rank BETWEEN 0 AND 4),
    boundary_revision_digest BLOB NOT NULL CHECK(length(boundary_revision_digest) = 32),
    boundary_state_version BLOB NOT NULL CHECK(length(boundary_state_version) = 16),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_migration_invocations(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE run_migration_boundaries (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    instance_state_version BLOB NOT NULL CHECK(length(instance_state_version) = 16),
    PRIMARY KEY (run_id, edge_index),
    FOREIGN KEY (run_id, edge_index) REFERENCES run_migration_edges(run_id, edge_index) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE run_migration_revision_pins (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    PRIMARY KEY (run_id, package_id, revision_content_digest),
    FOREIGN KEY (run_id) REFERENCES run_migration_invocations(run_id) ON DELETE CASCADE,
    FOREIGN KEY (package_id, revision_content_digest) REFERENCES revisions(package_id, revision_content_digest) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;
CREATE TABLE run_migration_payload_pins (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    payload_id BLOB NOT NULL CHECK(length(payload_id) = 16),
    PRIMARY KEY (run_id, instance_id, payload_id),
    FOREIGN KEY (run_id) REFERENCES run_migration_invocations(run_id) ON DELETE CASCADE,
    FOREIGN KEY (instance_id, payload_id) REFERENCES managed_input_payloads(instance_id, payload_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;
CREATE TABLE run_migration_checkpoint_bindings (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    input_identity BLOB NOT NULL CHECK(length(input_identity) > 0),
    payload_id BLOB NOT NULL CHECK(length(payload_id) = 16),
    PRIMARY KEY (run_id, input_identity),
    FOREIGN KEY (run_id) REFERENCES run_migration_progress(run_id) ON DELETE CASCADE,
    FOREIGN KEY (instance_id, payload_id) REFERENCES managed_input_payloads(instance_id, payload_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

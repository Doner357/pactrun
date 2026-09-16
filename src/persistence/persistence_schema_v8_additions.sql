DROP TABLE writable_admissions;
CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 8),
    PRIMARY KEY (owner_session)
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_history_identities (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    instance_name BLOB NOT NULL CHECK(length(instance_name) BETWEEN 1 AND 128),
    PRIMARY KEY (instance_id)
) STRICT, WITHOUT ROWID;
INSERT INTO instance_history_identities SELECT instance_id, instance_name FROM instances;

CREATE TABLE v8_runs (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    accepted_state_version BLOB NOT NULL CHECK(length(accepted_state_version) = 16),
    accepted_at_unix_ms INTEGER NOT NULL CHECK(accepted_at_unix_ms >= 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (instance_id) REFERENCES instance_history_identities(instance_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;
INSERT INTO v8_runs SELECT * FROM runs;
DROP TABLE runs;
ALTER TABLE v8_runs RENAME TO runs;

CREATE TABLE v8_run_operation_kinds (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    operation_kind INTEGER NOT NULL CHECK(operation_kind IN (0, 1, 2, 3, 4)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
INSERT INTO v8_run_operation_kinds SELECT * FROM run_operation_kinds;
DROP TABLE run_operation_kinds;
ALTER TABLE v8_run_operation_kinds RENAME TO run_operation_kinds;

CREATE TABLE run_deletion_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    deletion_mode INTEGER NOT NULL CHECK(deletion_mode IN (0, 1)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_deletion_obligations (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    attempt_run_id BLOB NOT NULL CHECK(length(attempt_run_id) = 16),
    phase INTEGER NOT NULL CHECK(phase IN (0, 1, 2)),
    PRIMARY KEY (instance_id),
    UNIQUE (attempt_run_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (attempt_run_id) REFERENCES runs(run_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE deletion_finalization_authorizations (
    attempt_run_id BLOB NOT NULL CHECK(length(attempt_run_id) = 16),
    authority_source INTEGER NOT NULL CHECK(authority_source IN (0, 1, 2)),
    authorized_state_version BLOB NOT NULL CHECK(length(authorized_state_version) = 16),
    authorized_at_unix_ms INTEGER NOT NULL CHECK(authorized_at_unix_ms >= 0),
    PRIMARY KEY (attempt_run_id),
    FOREIGN KEY (attempt_run_id) REFERENCES runs(run_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE deletion_finalization_allocations (
    attempt_run_id BLOB NOT NULL CHECK(length(attempt_run_id) = 16),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    finished INTEGER NOT NULL CHECK(finished IN (0, 1)),
    root_identity BLOB CHECK(root_identity IS NULL OR length(root_identity) = 40),
    PRIMARY KEY (attempt_run_id, allocation_id),
    UNIQUE (allocation_id),
    FOREIGN KEY (attempt_run_id) REFERENCES deletion_finalization_authorizations(attempt_run_id) ON DELETE RESTRICT,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_retirement_receipts (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    retirement_run_id BLOB NOT NULL CHECK(length(retirement_run_id) = 16),
    retirement_mode INTEGER NOT NULL CHECK(retirement_mode IN (0, 1)),
    partially_finalized INTEGER NOT NULL CHECK(partially_finalized IN (0, 1)),
    retired_at_unix_ms INTEGER NOT NULL CHECK(retired_at_unix_ms >= 0),
    PRIMARY KEY (instance_id),
    FOREIGN KEY (instance_id) REFERENCES instance_history_identities(instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (retirement_run_id) REFERENCES runs(run_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE detached_service_allocations (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    PRIMARY KEY (allocation_id),
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT,
    FOREIGN KEY (instance_id) REFERENCES instance_retirement_receipts(instance_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE allocation_discard_receipts (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    authorized_at_unix_ms INTEGER NOT NULL CHECK(authorized_at_unix_ms >= 0),
    finished INTEGER NOT NULL CHECK(finished IN (0, 1)),
    root_identity BLOB CHECK(root_identity IS NULL OR length(root_identity) = 40),
    PRIMARY KEY (allocation_id),
    FOREIGN KEY (allocation_id) REFERENCES detached_service_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

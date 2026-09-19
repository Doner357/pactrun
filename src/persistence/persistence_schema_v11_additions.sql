CREATE TABLE run_diagnostic_collections (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    retain_text INTEGER NOT NULL CHECK(retain_text IN (0, 1)),
    started INTEGER NOT NULL DEFAULT 0 CHECK(started IN (0, 1)),
    closed INTEGER NOT NULL DEFAULT 0 CHECK(closed IN (0, 1)),
    observed INTEGER NOT NULL DEFAULT 0 CHECK(observed >= 0),
    failed INTEGER NOT NULL DEFAULT 0 CHECK(failed IN (0, 1)),
    PRIMARY KEY(run_id),
    FOREIGN KEY(run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE run_diagnostic_events (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    sequence INTEGER NOT NULL CHECK(sequence > 0),
    received_at_unix_ms INTEGER CHECK(received_at_unix_ms >= 0),
    stage TEXT NOT NULL,
    kind INTEGER NOT NULL CHECK(kind IN (0, 1, 2)),
    severity INTEGER CHECK(severity IN (0, 1, 2)),
    code TEXT,
    message TEXT,
    truncated INTEGER NOT NULL CHECK(truncated IN (0, 1)),
    truncated_prefix_bytes INTEGER NOT NULL CHECK(truncated_prefix_bytes BETWEEN 0 AND 32768),
    completion_status INTEGER CHECK(completion_status IN (0, 1)),
    PRIMARY KEY(run_id, sequence),
    FOREIGN KEY(run_id) REFERENCES run_diagnostic_collections(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
DROP TABLE writable_admissions;
CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 11),
    PRIMARY KEY(owner_session)
) STRICT, WITHOUT ROWID;

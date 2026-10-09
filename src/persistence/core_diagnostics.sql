CREATE TABLE run_core_diagnostic_collections (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    started INTEGER NOT NULL DEFAULT 0 CHECK(started IN (0, 1)),
    observed INTEGER NOT NULL CHECK(observed >= 0),
    closed INTEGER NOT NULL CHECK(closed IN (0, 1)),
    failed INTEGER NOT NULL CHECK(failed IN (0, 1)),
    PRIMARY KEY(run_id),
    FOREIGN KEY(run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_core_diagnostic_events (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    sequence INTEGER NOT NULL CHECK(sequence > 0),
    received_at_unix_ms INTEGER CHECK(received_at_unix_ms >= 0),
    stage TEXT NOT NULL,
    failure TEXT NOT NULL CHECK(length(failure) <= 512 AND json_valid(failure)),
    PRIMARY KEY(run_id, sequence),
    FOREIGN KEY(run_id) REFERENCES run_core_diagnostic_collections(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

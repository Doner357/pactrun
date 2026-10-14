CREATE TABLE run_input_contexts (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= -1),
    scope_rank INTEGER NOT NULL CHECK(scope_rank BETWEEN 0 AND 4),
    coverage_rank INTEGER NOT NULL CHECK(coverage_rank IN (0, 1)),
    snapshot_id BLOB CHECK(snapshot_id IS NULL OR length(snapshot_id) = 16),
    snapshot_integrity BLOB CHECK(snapshot_integrity IS NULL OR length(snapshot_integrity) = 32),
    PRIMARY KEY(run_id, edge_index, scope_rank),
    CHECK((scope_rank IN (0, 1) AND edge_index = -1) OR (scope_rank IN (2, 3, 4) AND edge_index >= 0)),
    CHECK((scope_rank = 1) = (snapshot_id IS NOT NULL)),
    CHECK((scope_rank = 1) = (snapshot_integrity IS NOT NULL)),
    FOREIGN KEY(run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_input_references (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL,
    scope_rank INTEGER NOT NULL,
    input_id BLOB NOT NULL CHECK(length(input_id) BETWEEN 1 AND 128),
    role_rank INTEGER NOT NULL CHECK(role_rank IN (0, 1)),
    protection_rank INTEGER NOT NULL CHECK(protection_rank IN (0, 1)),
    origin_rank INTEGER NOT NULL CHECK(origin_rank BETWEEN 0 AND 4),
    payload_id BLOB CHECK(payload_id IS NULL OR length(payload_id) = 16),
    PRIMARY KEY(run_id, edge_index, scope_rank, input_id),
    CHECK((origin_rank = 1) = (payload_id IS NOT NULL)),
    CHECK(origin_rank <> 2 OR scope_rank = 1),
    CHECK(scope_rank <> 1 OR origin_rank IN (0, 2)),
    CHECK(origin_rank NOT IN (3, 4) OR scope_rank = 3),
    FOREIGN KEY(run_id, edge_index, scope_rank) REFERENCES run_input_contexts(run_id, edge_index, scope_rank) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

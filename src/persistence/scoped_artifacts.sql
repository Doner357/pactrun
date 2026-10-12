CREATE TABLE run_scoped_artifacts (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    artifact_key BLOB NOT NULL CHECK(length(artifact_key) = 8 + length(output_identity)),
    edge_index INTEGER NOT NULL CHECK(edge_index >= -1),
    output_identity BLOB NOT NULL CHECK(length(output_identity) > 0),
    byte_length INTEGER NOT NULL CHECK(byte_length BETWEEN 0 AND 536870912),
    PRIMARY KEY (run_id, artifact_key),
    UNIQUE (run_id, edge_index, output_identity),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_scoped_artifact_chunks (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    artifact_key BLOB NOT NULL CHECK(length(artifact_key) > 8),
    chunk_index INTEGER NOT NULL CHECK(chunk_index >= 0),
    chunk_bytes BLOB NOT NULL CHECK(length(chunk_bytes) BETWEEN 1 AND 1048576),
    PRIMARY KEY (run_id, artifact_key, chunk_index),
    FOREIGN KEY (run_id, artifact_key) REFERENCES run_scoped_artifacts(run_id, artifact_key) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

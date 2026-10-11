CREATE TABLE run_missing_input_causes (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    input_id BLOB NOT NULL CHECK(length(input_id) > 0),
    PRIMARY KEY (run_id, ordinal),
    UNIQUE (run_id, input_id),
    FOREIGN KEY (run_id) REFERENCES run_primary_failures(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

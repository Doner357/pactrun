CREATE TABLE run_service_prerequisite_causes (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    view_rank INTEGER NOT NULL CHECK(view_rank BETWEEN 0 AND 2),
    role_rank INTEGER NOT NULL CHECK(role_rank IN (0, 1)),
    resource_id BLOB NOT NULL CHECK(length(resource_id) BETWEEN 1 AND 128),
    expected_presence INTEGER NOT NULL CHECK(expected_presence IN (0, 1)),
    observed_presence INTEGER NOT NULL CHECK(observed_presence IN (0, 1)),
    source_revision_digest BLOB CHECK(source_revision_digest IS NULL OR length(source_revision_digest) = 32),
    target_revision_digest BLOB CHECK(target_revision_digest IS NULL OR length(target_revision_digest) = 32),
    PRIMARY KEY(run_id),
    CHECK(expected_presence <> observed_presence),
    CHECK((source_revision_digest IS NULL) = (target_revision_digest IS NULL)),
    CHECK(source_revision_digest IS NULL OR source_revision_digest <> target_revision_digest),
    FOREIGN KEY(run_id) REFERENCES run_primary_failures(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

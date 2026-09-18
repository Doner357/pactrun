ALTER TABLE snapshot_blobs ADD COLUMN storage_kind INTEGER NOT NULL DEFAULT 0
    CHECK(storage_kind IN (0, 1));
ALTER TABLE managed_input_payloads ADD COLUMN content_digest BLOB
    CHECK(content_digest IS NULL OR length(content_digest) = 32);
DROP TABLE writable_admissions;
CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 10),
    PRIMARY KEY (owner_session)
) STRICT, WITHOUT ROWID;

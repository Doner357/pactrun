CREATE TABLE package_local_names (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 31 AND length(CAST(name AS BLOB)) = length(name) AND name GLOB '[A-Za-z0-9]*' AND name NOT GLOB '*[^A-Za-z0-9_.-]*'),
    PRIMARY KEY (package_id),
    UNIQUE (name),
    FOREIGN KEY (package_id) REFERENCES packages(package_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_local_names (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 31 AND length(CAST(name AS BLOB)) = length(name) AND name GLOB '[A-Za-z0-9]*' AND name NOT GLOB '*[^A-Za-z0-9_.-]*'),
    PRIMARY KEY (package_id, revision_content_digest),
    UNIQUE (package_id, name),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_installations (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    installed_at_unix_ms INTEGER CHECK(installed_at_unix_ms IS NULL OR installed_at_unix_ms >= 0),
    PRIMARY KEY (package_id, revision_content_digest),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

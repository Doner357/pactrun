CREATE TABLE instances (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    instance_name BLOB NOT NULL CHECK(length(instance_name) BETWEEN 1 AND 128),
    active_package_id BLOB NOT NULL CHECK(length(active_package_id) = 16),
    active_revision_content_digest BLOB NOT NULL CHECK(length(active_revision_content_digest) = 32),
    instance_state_version BLOB NOT NULL CHECK(length(instance_state_version) = 16),
    PRIMARY KEY (instance_id),
    UNIQUE (instance_name),
    UNIQUE (instance_state_version),
    FOREIGN KEY (active_package_id, active_revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE managed_input_payloads (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    payload_id BLOB NOT NULL CHECK(length(payload_id) = 16),
    protection_rank INTEGER NOT NULL CHECK(protection_rank IN (0, 1)),
    byte_length INTEGER NOT NULL CHECK(byte_length BETWEEN 0 AND 536870912),
    PRIMARY KEY (instance_id, payload_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE managed_input_payload_chunks (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    payload_id BLOB NOT NULL CHECK(length(payload_id) = 16),
    chunk_index INTEGER NOT NULL CHECK(chunk_index >= 0),
    chunk_bytes BLOB NOT NULL CHECK(length(chunk_bytes) BETWEEN 1 AND 1048576),
    PRIMARY KEY (instance_id, payload_id, chunk_index),
    FOREIGN KEY (instance_id, payload_id)
        REFERENCES managed_input_payloads(instance_id, payload_id)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE managed_input_bindings (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    input_identity BLOB NOT NULL CHECK(length(input_identity) > 0),
    payload_id BLOB NOT NULL CHECK(length(payload_id) = 16),
    PRIMARY KEY (instance_id, input_identity),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE CASCADE,
    FOREIGN KEY (instance_id, payload_id)
        REFERENCES managed_input_payloads(instance_id, payload_id)
        ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

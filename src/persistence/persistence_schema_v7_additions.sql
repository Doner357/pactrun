DROP TABLE writable_admissions;
CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 7),
    PRIMARY KEY (owner_session)
) STRICT, WITHOUT ROWID;

CREATE TABLE service_storage_allocations (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    origin_revision_digest BLOB NOT NULL CHECK(length(origin_revision_digest) = 32),
    origin_storage_identity BLOB NOT NULL CHECK(length(origin_storage_identity) BETWEEN 1 AND 128),
    PRIMARY KEY (allocation_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE service_storage_preparations (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    PRIMARY KEY (allocation_id),
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE CASCADE,
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE service_storage_protections (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    PRIMARY KEY (allocation_id),
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE service_storage_run_origins (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    origin_run_id BLOB NOT NULL CHECK(length(origin_run_id) = 16),
    PRIMARY KEY (allocation_id),
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_service_storages (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    storage_identity BLOB NOT NULL CHECK(length(storage_identity) BETWEEN 1 AND 128),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    declaration_package_id BLOB NOT NULL CHECK(length(declaration_package_id) = 16),
    declaration_revision_digest BLOB NOT NULL CHECK(length(declaration_revision_digest) = 32),
    PRIMARY KEY (instance_id, storage_identity),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT,
    FOREIGN KEY (declaration_package_id, declaration_revision_digest)
        REFERENCES revisions(package_id, revision_content_digest) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_service_resources (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    resource_identity BLOB NOT NULL CHECK(length(resource_identity) BETWEEN 1 AND 128),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    declaration_package_id BLOB NOT NULL CHECK(length(declaration_package_id) = 16),
    declaration_revision_digest BLOB NOT NULL CHECK(length(declaration_revision_digest) = 32),
    PRIMARY KEY (instance_id, resource_identity),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT,
    FOREIGN KEY (declaration_package_id, declaration_revision_digest)
        REFERENCES revisions(package_id, revision_content_digest) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_service_storage_pins (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    PRIMARY KEY (run_id, allocation_id),
    FOREIGN KEY (run_id) REFERENCES run_executions(run_id) ON DELETE CASCADE,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_service_storage_targets (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    storage_identity BLOB NOT NULL CHECK(length(storage_identity) BETWEEN 1 AND 128),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    PRIMARY KEY (run_id, edge_index, storage_identity),
    FOREIGN KEY (run_id) REFERENCES run_executions(run_id) ON DELETE CASCADE,
    FOREIGN KEY (run_id, edge_index) REFERENCES run_migration_edges(run_id, edge_index) ON DELETE CASCADE,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_service_resource_targets (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    resource_identity BLOB NOT NULL CHECK(length(resource_identity) BETWEEN 1 AND 128),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    PRIMARY KEY (run_id, edge_index, resource_identity),
    FOREIGN KEY (run_id) REFERENCES run_executions(run_id) ON DELETE CASCADE,
    FOREIGN KEY (run_id, edge_index) REFERENCES run_migration_edges(run_id, edge_index) ON DELETE CASCADE,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_service_edge_commits (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    PRIMARY KEY (run_id, edge_index),
    FOREIGN KEY (run_id, edge_index) REFERENCES run_migration_boundaries(run_id, edge_index) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

-- Fresh Persistence 1.0-alpha.1. This is complete DDL, not an upgrade ladder.

CREATE TABLE allocation_discard_receipts (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    authorized_at_unix_ms INTEGER NOT NULL CHECK(authorized_at_unix_ms >= 0),
    finished INTEGER NOT NULL CHECK(finished IN (0, 1)),
    root_identity BLOB CHECK(root_identity IS NULL OR length(root_identity) = 40),
    PRIMARY KEY (allocation_id),
    FOREIGN KEY (allocation_id) REFERENCES detached_service_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE deletion_finalization_allocations (
    attempt_run_id BLOB NOT NULL CHECK(length(attempt_run_id) = 16),
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    finished INTEGER NOT NULL CHECK(finished IN (0, 1)),
    root_identity BLOB CHECK(root_identity IS NULL OR length(root_identity) = 40),
    PRIMARY KEY (attempt_run_id, allocation_id),
    UNIQUE (allocation_id),
    FOREIGN KEY (attempt_run_id) REFERENCES deletion_finalization_authorizations(attempt_run_id) ON DELETE RESTRICT,
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE deletion_finalization_authorizations (
    attempt_run_id BLOB NOT NULL CHECK(length(attempt_run_id) = 16),
    authority_source INTEGER NOT NULL CHECK(authority_source IN (0, 1, 2)),
    authorized_state_version BLOB NOT NULL CHECK(length(authorized_state_version) = 16),
    authorized_at_unix_ms INTEGER NOT NULL CHECK(authorized_at_unix_ms >= 0),
    PRIMARY KEY (attempt_run_id),
    FOREIGN KEY (attempt_run_id) REFERENCES runs(run_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE detached_service_allocations (
    allocation_id BLOB NOT NULL CHECK(length(allocation_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    PRIMARY KEY (allocation_id),
    FOREIGN KEY (allocation_id) REFERENCES service_storage_allocations(allocation_id) ON DELETE RESTRICT,
    FOREIGN KEY (instance_id) REFERENCES instance_retirement_receipts(instance_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_deletion_obligations (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    attempt_run_id BLOB NOT NULL CHECK(length(attempt_run_id) = 16),
    phase INTEGER NOT NULL CHECK(phase IN (0, 1, 2)),
    PRIMARY KEY (instance_id),
    UNIQUE (attempt_run_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (attempt_run_id) REFERENCES runs(run_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_history_identities (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    instance_name BLOB NOT NULL CHECK(length(instance_name) BETWEEN 1 AND 128),
    PRIMARY KEY (instance_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_recovery_consequence_versions (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    consequence_version INTEGER NOT NULL CHECK(consequence_version >= 0),
    PRIMARY KEY (instance_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_recovery_guards (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    trigger_rank INTEGER NOT NULL CHECK(trigger_rank IN (0, 1, 2)),
    entered_at_unix_ms INTEGER NOT NULL CHECK(entered_at_unix_ms >= 0),
    PRIMARY KEY (instance_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE CASCADE,
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE instance_retirement_receipts (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    retirement_run_id BLOB NOT NULL CHECK(length(retirement_run_id) = 16),
    retirement_mode INTEGER NOT NULL CHECK(retirement_mode IN (0, 1)),
    partially_finalized INTEGER NOT NULL CHECK(partially_finalized IN (0, 1)),
    retired_at_unix_ms INTEGER NOT NULL CHECK(retired_at_unix_ms >= 0),
    PRIMARY KEY (instance_id),
    FOREIGN KEY (instance_id) REFERENCES instance_history_identities(instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (retirement_run_id) REFERENCES runs(run_id) ON DELETE RESTRICT
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

CREATE TABLE managed_input_payloads (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    payload_id BLOB NOT NULL CHECK(length(payload_id) = 16),
    protection_rank INTEGER NOT NULL CHECK(protection_rank IN (0, 1)),
    byte_length INTEGER NOT NULL CHECK(byte_length BETWEEN 0 AND 536870912), content_digest BLOB
    CHECK(content_digest IS NULL OR length(content_digest) = 32),
    PRIMARY KEY (instance_id, payload_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE packages (
    package_id BLOB NOT NULL
        CHECK(length(package_id) = 16),

    PRIMARY KEY (package_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_action_parameter_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    action_id_utf8 BLOB NOT NULL CHECK(length(action_id_utf8) > 0),
    parameter_id_utf8 BLOB NOT NULL CHECK(length(parameter_id_utf8) > 0),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        action_id_utf8,
        parameter_id_utf8
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_action_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    action_id_utf8 BLOB NOT NULL CHECK(length(action_id_utf8) > 0),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (package_id, revision_content_digest, action_id_utf8),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_attribution_claims (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    attribution_text_utf8 BLOB NOT NULL
        CHECK(length(attribution_text_utf8) > 0),
    source_uri_present INTEGER NOT NULL CHECK(source_uri_present IN (0, 1)),
    source_uri_ascii BLOB NOT NULL,

    CHECK (
        (source_uri_present = 0 AND length(source_uri_ascii) = 0)
        OR
        (source_uri_present = 1 AND length(source_uri_ascii) > 0)
    ),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        attribution_text_utf8,
        source_uri_present,
        source_uri_ascii
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_cleanup_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (package_id, revision_content_digest),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_input_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    input_id_utf8 BLOB NOT NULL CHECK(length(input_id_utf8) > 0),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (package_id, revision_content_digest, input_id_utf8),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_local_aliases (
    alias_utf8 BLOB NOT NULL CHECK(length(alias_utf8) > 0),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),

    PRIMARY KEY (alias_utf8),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_local_current_notes (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    note_utf8 BLOB NOT NULL CHECK(length(note_utf8) > 0),

    PRIMARY KEY (package_id, revision_content_digest),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_local_current_trust (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    trust_rank INTEGER NOT NULL CHECK(trust_rank IN (0, 1)),

    PRIMARY KEY (package_id, revision_content_digest),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_managed_output_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    action_id_utf8 BLOB NOT NULL CHECK(length(action_id_utf8) > 0),
    output_id_utf8 BLOB NOT NULL CHECK(length(output_id_utf8) > 0),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        action_id_utf8,
        output_id_utf8
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_migration_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    source_revision_content_digest BLOB NOT NULL
        CHECK(length(source_revision_content_digest) = 32),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        source_revision_content_digest
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (package_id, revision_content_digest),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_publisher_attribution_claims (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    publisher_name_utf8 BLOB NOT NULL
        CHECK(length(publisher_name_utf8) > 0),
    publisher_namespace_present INTEGER NOT NULL
        CHECK(publisher_namespace_present IN (0, 1)),
    publisher_namespace_utf8 BLOB NOT NULL,
    source_uri_present INTEGER NOT NULL CHECK(source_uri_present IN (0, 1)),
    source_uri_ascii BLOB NOT NULL,

    CHECK (
        (publisher_namespace_present = 0
            AND length(publisher_namespace_utf8) = 0)
        OR
        (publisher_namespace_present = 1
            AND length(publisher_namespace_utf8) > 0)
    ),
    CHECK (
        (source_uri_present = 0 AND length(source_uri_ascii) = 0)
        OR
        (source_uri_present = 1 AND length(source_uri_ascii) > 0)
    ),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        publisher_name_utf8,
        publisher_namespace_present,
        publisher_namespace_utf8,
        source_uri_present,
        source_uri_ascii
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_reference_label_bindings (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    label_utf8 BLOB NOT NULL CHECK(length(label_utf8) > 0),
    source_rank INTEGER NOT NULL CHECK(source_rank BETWEEN 0 AND 3),
    publisher_name_utf8 BLOB NOT NULL,
    publisher_namespace_present INTEGER NOT NULL
        CHECK(publisher_namespace_present IN (0, 1)),
    publisher_namespace_utf8 BLOB NOT NULL,
    source_uri_ascii BLOB NOT NULL,

    CHECK (
        (source_rank = 0
            AND length(publisher_name_utf8) = 0
            AND publisher_namespace_present = 0
            AND length(publisher_namespace_utf8) = 0
            AND length(source_uri_ascii) = 0)
        OR
        (source_rank = 1
            AND length(publisher_name_utf8) = 0
            AND publisher_namespace_present = 0
            AND length(publisher_namespace_utf8) = 0
            AND length(source_uri_ascii) > 0)
        OR
        (source_rank = 2
            AND length(publisher_name_utf8) > 0
            AND (
                (publisher_namespace_present = 0
                    AND length(publisher_namespace_utf8) = 0)
                OR
                (publisher_namespace_present = 1
                    AND length(publisher_namespace_utf8) > 0)
            )
            AND length(source_uri_ascii) = 0)
        OR
        (source_rank = 3
            AND length(publisher_name_utf8) > 0
            AND (
                (publisher_namespace_present = 0
                    AND length(publisher_namespace_utf8) = 0)
                OR
                (publisher_namespace_present = 1
                    AND length(publisher_namespace_utf8) > 0)
            )
            AND length(source_uri_ascii) > 0)
    ),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        label_utf8,
        source_rank,
        publisher_name_utf8,
        publisher_namespace_present,
        publisher_namespace_utf8,
        source_uri_ascii
    ),

    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_runtime_content_refs (
    package_id BLOB NOT NULL
        CHECK(length(package_id) = 16),

    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),

    content_id TEXT NOT NULL COLLATE BINARY,

    blob_digest BLOB NOT NULL
        CHECK(length(blob_digest) = 32),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        content_id
    ),

    FOREIGN KEY (
        package_id,
        revision_content_digest
    )
    REFERENCES revisions(
        package_id,
        revision_content_digest
    )
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_snapshot_operation_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    operation_rank INTEGER NOT NULL CHECK(operation_rank IN (0, 1)),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (package_id, revision_content_digest, operation_rank),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_snapshot_parameter_presentation_current (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    operation_rank INTEGER NOT NULL CHECK(operation_rank IN (0, 1)),
    parameter_id_utf8 BLOB NOT NULL CHECK(length(parameter_id_utf8) > 0),
    display_name_utf8 BLOB,
    summary_utf8 BLOB,
    description_utf8 BLOB,
    help_utf8 BLOB,

    CHECK(display_name_utf8 IS NULL OR length(display_name_utf8) > 0),
    CHECK(summary_utf8 IS NULL OR length(summary_utf8) > 0),
    CHECK(description_utf8 IS NULL OR length(description_utf8) > 0),
    CHECK(help_utf8 IS NULL OR length(help_utf8) > 0),
    CHECK(display_name_utf8 IS NOT NULL OR summary_utf8 IS NOT NULL
        OR description_utf8 IS NOT NULL OR help_utf8 IS NOT NULL),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        operation_rank,
        parameter_id_utf8
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_source_uri_claims (
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),
    source_uri_ascii BLOB NOT NULL CHECK(length(source_uri_ascii) > 0),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        source_uri_ascii
    ),
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE revisions (
    package_id BLOB NOT NULL
        CHECK(length(package_id) = 16),

    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),

    core_jcs BLOB NOT NULL,
    runtime_content_jcs BLOB NOT NULL,

    PRIMARY KEY (
        package_id,
        revision_content_digest
    ),

    FOREIGN KEY (package_id)
        REFERENCES packages(package_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE run_action_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    action_identity BLOB NOT NULL CHECK(length(action_identity) > 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_artifact_chunks (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    output_identity BLOB NOT NULL CHECK(length(output_identity) > 0),
    chunk_index INTEGER NOT NULL CHECK(chunk_index >= 0),
    chunk_bytes BLOB NOT NULL CHECK(length(chunk_bytes) BETWEEN 1 AND 1048576),
    PRIMARY KEY (run_id, output_identity, chunk_index),
    FOREIGN KEY (run_id, output_identity)
        REFERENCES run_artifacts(run_id, output_identity)
        ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_artifacts (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    output_identity BLOB NOT NULL CHECK(length(output_identity) > 0),
    byte_length INTEGER NOT NULL CHECK(byte_length BETWEEN 0 AND 536870912),
    PRIMARY KEY (run_id, output_identity),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE "run_capture_invocations" (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES "run_operation_kinds"(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_capture_results (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_deletion_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    deletion_mode INTEGER NOT NULL CHECK(deletion_mode IN (0, 1)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

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

CREATE TABLE run_executions (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    owner_session BLOB NOT NULL CHECK(length(owner_session) BETWEEN 1 AND 128),
    risk_state INTEGER NOT NULL CHECK(risk_state IN (0, 1)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_hook_completions (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    status_rank INTEGER NOT NULL CHECK(status_rank IN (0, 1)),
    code_present INTEGER NOT NULL CHECK(code_present IN (0, 1)),
    code_utf8 BLOB NOT NULL
        CHECK((code_present = 0 AND length(code_utf8) = 0)
            OR (code_present = 1 AND length(code_utf8) > 0)),
    message_present INTEGER NOT NULL CHECK(message_present IN (0, 1)),
    message_utf8 BLOB NOT NULL CHECK(message_present = 1 OR length(message_utf8) = 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_migration_boundaries (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    instance_state_version BLOB NOT NULL CHECK(length(instance_state_version) = 16),
    PRIMARY KEY (run_id, edge_index),
    FOREIGN KEY (run_id, edge_index) REFERENCES run_migration_edges(run_id, edge_index) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_migration_checkpoint_bindings (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    input_identity BLOB NOT NULL CHECK(length(input_identity) > 0),
    payload_id BLOB NOT NULL CHECK(length(payload_id) = 16),
    PRIMARY KEY (run_id, input_identity),
    FOREIGN KEY (run_id) REFERENCES run_migration_progress(run_id) ON DELETE CASCADE,
    FOREIGN KEY (instance_id, payload_id) REFERENCES managed_input_payloads(instance_id, payload_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_migration_edges (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    source_revision_digest BLOB NOT NULL CHECK(length(source_revision_digest) = 32),
    target_revision_digest BLOB NOT NULL CHECK(length(target_revision_digest) = 32),
    PRIMARY KEY (run_id, edge_index),
    CHECK(source_revision_digest <> target_revision_digest),
    FOREIGN KEY (run_id) REFERENCES run_migration_invocations(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_migration_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    source_revision_digest BLOB NOT NULL CHECK(length(source_revision_digest) = 32),
    target_revision_digest BLOB NOT NULL CHECK(length(target_revision_digest) = 32),
    authorize_declassification INTEGER NOT NULL CHECK(authorize_declassification IN (0, 1)),
    PRIMARY KEY (run_id),
    CHECK(source_revision_digest <> target_revision_digest),
    FOREIGN KEY (run_id) REFERENCES run_operation_kinds(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_migration_payload_pins (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    payload_id BLOB NOT NULL CHECK(length(payload_id) = 16),
    PRIMARY KEY (run_id, instance_id, payload_id),
    FOREIGN KEY (run_id) REFERENCES run_migration_invocations(run_id) ON DELETE CASCADE,
    FOREIGN KEY (instance_id, payload_id) REFERENCES managed_input_payloads(instance_id, payload_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_migration_progress (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    committed_edge_count INTEGER NOT NULL CHECK(committed_edge_count >= 0),
    step_rank INTEGER NOT NULL CHECK(step_rank BETWEEN 0 AND 4),
    boundary_revision_digest BLOB NOT NULL CHECK(length(boundary_revision_digest) = 32),
    boundary_state_version BLOB NOT NULL CHECK(length(boundary_state_version) = 16),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_migration_invocations(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_migration_revision_pins (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    PRIMARY KEY (run_id, package_id, revision_content_digest),
    FOREIGN KEY (run_id) REFERENCES run_migration_invocations(run_id) ON DELETE CASCADE,
    FOREIGN KEY (package_id, revision_content_digest) REFERENCES revisions(package_id, revision_content_digest) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE "run_operation_kinds" (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    operation_kind INTEGER NOT NULL CHECK(operation_kind IN (0, 1, 2, 3, 4)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_outcomes (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    outcome_rank INTEGER NOT NULL CHECK(outcome_rank IN (0, 1, 2, 3, 4)),
    admitted_rank INTEGER NOT NULL CHECK(admitted_rank IN (0, 1)),
    risk_state INTEGER NOT NULL CHECK(risk_state IN (0, 1)),
    finished_at_unix_ms INTEGER NOT NULL CHECK(finished_at_unix_ms >= 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_payload_pins (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    input_identity BLOB NOT NULL CHECK(length(input_identity) > 0),
    payload_id BLOB NOT NULL CHECK(length(payload_id) = 16),
    PRIMARY KEY (run_id, instance_id, input_identity),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE,
    FOREIGN KEY (instance_id, payload_id)
        REFERENCES managed_input_payloads(instance_id, payload_id)
        ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_primary_failures (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    error_owner BLOB NOT NULL CHECK(length(error_owner) BETWEEN 1 AND 128),
    error_code BLOB NOT NULL CHECK(length(error_code) BETWEEN 1 AND 128),
    failed_step INTEGER NOT NULL CHECK(failed_step BETWEEN 0 AND 5),
    message_utf8 BLOB NOT NULL,
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_restore_admissions (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    admitted_state_version BLOB NOT NULL CHECK(length(admitted_state_version) = 16),
    admitted_consequence_version INTEGER NOT NULL CHECK(admitted_consequence_version >= 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES run_executions(run_id) ON DELETE CASCADE,
    FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE "run_restore_invocations" (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES "run_operation_kinds"(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_revision_pins (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE,
    FOREIGN KEY (package_id, revision_content_digest)
        REFERENCES revisions(package_id, revision_content_digest)
        ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_secondary_failures (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    error_owner BLOB NOT NULL CHECK(length(error_owner) BETWEEN 1 AND 128),
    error_code BLOB NOT NULL CHECK(length(error_code) BETWEEN 1 AND 128),
    message_utf8 BLOB NOT NULL,
    PRIMARY KEY (run_id, ordinal),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_service_edge_commits (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    edge_index INTEGER NOT NULL CHECK(edge_index >= 0),
    PRIMARY KEY (run_id, edge_index),
    FOREIGN KEY (run_id, edge_index) REFERENCES run_migration_boundaries(run_id, edge_index) ON DELETE CASCADE
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

CREATE TABLE "runs" (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    accepted_state_version BLOB NOT NULL CHECK(length(accepted_state_version) = 16),
    accepted_at_unix_ms INTEGER NOT NULL CHECK(accepted_at_unix_ms >= 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (instance_id) REFERENCES instance_history_identities(instance_id) ON DELETE RESTRICT
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

CREATE TABLE snapshot_blobs (
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    blob_digest BLOB NOT NULL CHECK(length(blob_digest) = 32),
    byte_length INTEGER NOT NULL CHECK(byte_length >= 0),
    PRIMARY KEY (snapshot_id, blob_digest),
    FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE snapshots (
    snapshot_id BLOB NOT NULL CHECK(length(snapshot_id) = 16),
    integrity_format TEXT NOT NULL CHECK(length(integrity_format) BETWEEN 1 AND 128),
    integrity_digest BLOB NOT NULL CHECK(length(integrity_digest) = 32),
    canonical_manifest BLOB NOT NULL CHECK(length(canonical_manifest) > 0),
    PRIMARY KEY (snapshot_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE writable_admissions (
    owner_session BLOB NOT NULL CHECK(length(owner_session) = 40),
    admitted_schema_version INTEGER NOT NULL CHECK(admitted_schema_version = 0),
    PRIMARY KEY(owner_session)
) STRICT, WITHOUT ROWID;

CREATE TABLE pactrun_metadata (
    singleton INTEGER NOT NULL PRIMARY KEY CHECK(singleton = 1),
    format_version TEXT NOT NULL
) STRICT, WITHOUT ROWID;

INSERT INTO pactrun_metadata(singleton, format_version) VALUES (1, '1.0-alpha.1');

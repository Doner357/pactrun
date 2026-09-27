---
title: Persistence Baseline 1.0-alpha.1
---

# Persistence baseline 1.0-alpha.1

**Status: Approved E normative baseline, 1.0-alpha.1; integration verification in progress.**

<!-- spec-navigation:start -->
This page owns the complete fresh schema and the surviving persistence obligations
under [PR-REQ-0078](../foundations/resources-and-versioning.md#pr-req-0078---persistence-migrations).
Development-era schemas are not supported or upgraded, and real data is not deleted.
<!-- spec-navigation:end -->

The public identifier is the singleton pactrun_metadata format_version string.
The application marker remains 0x50414354; SQLite user_version and the admission
integer are private bootstrap marker 0, not a second public format version.
Only pristine storage or the exact supported metadata and schema may be admitted.
Existing populated files without supported metadata are refused before Pactrun
staging, admission/content-coordination creation, conversion or cleanup. Ordinary
read-only SQLite inspection MAY create or update its own read-coordination
sidecars. It MUST NOT change existing database or committed WAL content, ignore
committed WAL frames, rewrite identities or remove data. WAL files may contain
committed data; they are not disposable merely because they are sidecars.
Admission repeats the checks
under its serialized write transaction. No product update relabels data or
rewrites identities, bindings, non-terminal Runs or recovery obligations.

The 72-table baseline preserves functional constraints and contains no ALTER
ladder. Snapshot payloads are immutable files; the obsolete inline Snapshot
representation is absent. Managed Input chunks remain an active representation.

## Complete fresh schema

```sql
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
```

## Functional persistence obligations

Stable requirement IDs and historical heading anchors are retained below. Their
headings do not advertise numeric-schema support. Only the complete current DDL
above defines layout; functional transaction, ordering and ownership rules remain.

### PR-REQ-0255 - Typed metadata mutation and repository contract

The crate-private repository MUST accept only typed metadata values and a
`RevisionMetadataMutationBatch` addressed to one already-persisted exact
Revision. A batch MAY contain idempotent reference-label and provenance add or
remove operations and semantic compare-and-set operations for presentation,
local aliases, the local current note, and the local current trust assessment.
It MUST NOT accept a generic metadata key, arbitrary JSON, a persistence row,
or a serialized Rust object as a Domain value.

Batch meaning MUST be independent of caller order. Completely identical
operations MAY coalesce. Adding and removing the same set-like tuple in one
batch is invalid, as are different compare-and-set mutations for the same typed
semantic key. Identical compare-and-set operations MAY coalesce. Pactrun MUST
reject a contradictory batch before publishing any part of it; it MUST NOT
interpret caller order as a sequence that resolves the contradiction.

Every current-state mutation MUST carry `expected` and `desired` typed states,
each exactly `Absent | Present(value)`. While holding the database write
transaction, Pactrun MUST compare the current typed semantic state as follows:

1. if current equals desired, the operation is an idempotent success;
2. otherwise, if current equals expected, Pactrun applies desired;
3. otherwise, the complete batch conflicts and MUST roll back.

This comparison intentionally has no history-sensitive or ABA guarantee. M1-D
has no metadata generation, version token, event history, or audit order. If a
value changes from A to B and back to A, a later operation expecting A cannot
distinguish that history from no intervening mutation. Stronger concurrency is
a future design gate.

The repository MUST use `BEGIN IMMEDIATE`. Validation failure, a missing target
Revision, a malformed or absent presentation target, alias conflict, CAS
conflict, constraint failure, or process loss before commit MUST publish no
part of the batch. A successful commit is the durable metadata boundary. Loss
after commit but before the response MAY be retried: set-like operations and
current-equals-desired handling MUST converge on the committed semantic state.
The batch is not required to share a transaction with initial Revision
publication; future installation or import orchestration may coordinate those
boundaries without changing this contract.

Logical load MUST validate every stored value against its typed Domain syntax,
validate every presentation target against the strict-decoded canonical
`RevisionCoreV1`, reject non-canonical optional encodings, and return the same
typed semantic state that was committed. Any impossible enum rank, invalid
UTF-8, malformed URI, empty present value, invalid target, duplicate semantic
tuple, or derived-state mismatch is corruption rather than another tolerated
representation.

Every deterministic lookup or enumeration MUST use the comparators in
PR-REQ-0249 through PR-REQ-0253. SQL queries MUST state every correctness-
relevant `ORDER BY` component explicitly. Cross-table presentation and metadata
enumeration MUST merge results with the same Domain comparator. SQL and Domain
ordering MUST be parity-equivalent and MUST NOT use rowid, insertion order,
timestamps, query-plan order, locale collation, or SQLite's NULL ordering.

The Revision-wide metadata-kind rank is, in order: reference-label binding,
presentation, source-URI claim, publisher-attribution claim, attribution claim,
local alias, local current note, and local current trust. Within each kind, the
complete type comparator is:

- reference label: label bytes, Revision identity, source rank, then the source
  tuple in declaration order;
- presentation: Revision identity, target rank, target components in declaration
  order, then field rank;
- each provenance relation: Revision identity, claim rank, then its complete
  typed claim tuple in declaration order;
- local alias: alias bytes, then target Revision identity;
- local current note and trust: their singleton Revision identity only.

Optional tuple components compare presence rank before present payload. Exact
text uses BLOB byte order, and Revision identity compares `package_id` before
`revision_content_digest`. A label lookup fixes the exact label, deduplicates
source rows by Revision identity, and orders only the resulting distinct
Revision identities. These ranks and components are semantic constants rather
than implementation enumeration layouts.

An authorized exact Revision deletion MUST remove all of that Revision's M1-D
subordinate rows in the same database transaction. Metadata MUST NOT pin a
Revision or require a separate purge. Cascading one target's label bindings
MUST NOT remove bindings that target another Revision.

**Verification: PR-TEST-0062, PR-TEST-0063, PR-TEST-0064, PR-TEST-0065,
PR-TEST-0066, PR-TEST-0067, PR-TEST-0530, PR-TEST-0536.**

### PR-REQ-0256 - Exact PersistenceSchemaV2

The complete baseline contains immutable Package/Revision/reference tables and
typed metadata relations. Metadata MUST NOT mutate canonical components or derived
runtime references. The complete DDL above is authoritative; it is not a delta
or a reader for the original development schema.

The complete baseline DDL above owns this representation; there is no historical SQL upgrade step.

For a pristine database, the complete block above is the exact implemented
schema. For an exact V1 database, migration executes only the additions after
the marked boundary. The presentation operation ranks are Capture `0` and Restore `1`; the trust
ranks are Trusted `0` and Distrusted `1`. Table identity supplies the remaining
presentation target ranks from PR-REQ-0251. A presentation row represents the
four current fields for one target. SQL NULL represents an absent field only
inside that unique target row. The all-NULL check ensures the row itself cannot
be a second representation of total absence; when all four fields are absent,
the row MUST not exist. Local note, local trust, and alias absence are likewise
represented only by row absence.

The source discriminators, presence flags, non-null payloads, and checks above
ensure that SQL's NULL-distinct uniqueness behavior cannot create duplicate
semantic tuples. A zero-length payload is a storage sentinel only in a branch
whose discriminator says `Absent`; it is never a valid present Domain value.

## Required repository ordering

Every ordered SQL read MUST include an explicit clause equivalent to the
applicable clause below. A query that fixes a leading component MAY omit only
that constant component; it MUST retain every remaining component. No textual
column uses a locale collation.

```sql
-- Complete reference-label binding enumeration
ORDER BY label_utf8, package_id, revision_content_digest, source_rank,
         publisher_name_utf8, publisher_namespace_present,
         publisher_namespace_utf8, source_uri_ascii

-- Distinct-target lookup for one exact label
ORDER BY package_id, revision_content_digest

-- Presentation rows, before Domain expansion by field rank
ORDER BY package_id, revision_content_digest                         -- Revision
ORDER BY package_id, revision_content_digest, input_id_utf8          -- Input
ORDER BY package_id, revision_content_digest, action_id_utf8         -- Action
ORDER BY package_id, revision_content_digest, action_id_utf8,
         parameter_id_utf8                                           -- Action parameter
ORDER BY package_id, revision_content_digest, action_id_utf8,
         output_id_utf8                                              -- Managed output
ORDER BY package_id, revision_content_digest, operation_rank         -- Snapshot operation
ORDER BY package_id, revision_content_digest, operation_rank,
         parameter_id_utf8                                           -- Snapshot parameter
ORDER BY package_id, revision_content_digest,
         source_revision_content_digest                              -- Migration edge
ORDER BY package_id, revision_content_digest                         -- Cleanup

-- Provenance relations; cross-table Domain merge adds claim rank
ORDER BY package_id, revision_content_digest, source_uri_ascii
ORDER BY package_id, revision_content_digest, publisher_name_utf8,
         publisher_namespace_present, publisher_namespace_utf8,
         source_uri_present, source_uri_ascii
ORDER BY package_id, revision_content_digest, attribution_text_utf8,
         source_uri_present, source_uri_ascii

-- Local metadata
ORDER BY alias_utf8, package_id, revision_content_digest
ORDER BY package_id, revision_content_digest                         -- Note or trust
```

For presentation, the repository MUST merge table families by the target rank
in PR-REQ-0251, compare their typed target components, and expand non-NULL
columns by field rank. For provenance, it MUST merge table families by the
claim rank in PR-REQ-0252. A Revision-wide read MUST then merge kinds by the
metadata-kind rank in PR-REQ-0255. Repository load MUST independently verify
that the final Domain sequence is ordered by the same comparator; physical
primary-key iteration is not a substitute for these clauses.

**Verification: PR-TEST-0059, PR-TEST-0063, PR-TEST-0067.**

### PR-REQ-0257 - V1-to-V2 migration and validation

Retired by the approved E one-time baseline reset on 2026-09-26.
Development-era persistence upgrades are unsupported. Shared exact-schema,
serialized admission, bootstrap crash safety and preservation obligations now
belong to PR-REQ-0078 and PR-REQ-0299; this historical upgrade is not a
compatibility promise for the new baseline.

**Verification: Not applicable — retired requirement, not pending runtime coverage.**

## Planned crate-private typed interface

The implementation slice must derive, rather than redefine, an interface
equivalent to:

```text
CurrentState<T> = Absent | Present(T)

RevisionMetadataMutationBatch
|- AddReferenceLabel(ReferenceLabelBinding)
|- RemoveReferenceLabel(ReferenceLabelBinding)
|- CompareAndSetPresentation(target, field, expected, desired)
|- AddProvenance(ProvenanceClaim)
|- RemoveProvenance(ProvenanceClaim)
|- CompareAndSetLocalAlias(alias, expected_state, desired_state)
|- CompareAndSetLocalNote(expected, desired)
`- CompareAndSetLocalTrust(expected, desired)

apply_revision_metadata_batch(RevisionIdentity, RevisionMetadataMutationBatch)
load_revision_metadata(RevisionIdentity) -> RevisionMetadataView
lookup_reference_label(ReferenceLabel) -> ordered distinct RevisionIdentity[]
lookup_local_alias(LocalAlias) -> RevisionIdentity?
```

The concrete Rust names remain crate-private implementation detail. The typed
operations, states, equality, conflict, validation, atomicity, and ordering are
the required interface semantics. Alias states are
`CurrentState<RevisionIdentity>`; every `Present` identity involved in an
accepted replacement MUST already exist, and the batch's target Revision is the
desired target for a present binding or the current target for a clear. No
stable public Rust API, CLI spelling, or wire contract is created here.

## Deferred work

This internal schema does not define Export Bundle serialization or merge,
Package or Instance metadata, `LocalInstall`, generic timestamps, localization,
fuzzy lookup, additional provenance claims, trust policy or history, note history,
history-sensitive CAS or ABA detection, stable new error codes, cross-domain
Revision-plus-metadata publication, CLI, or ServiceStorage persistence.


### PR-REQ-0269 - Exact PersistenceSchemaV3

The complete baseline contains live Instance identities, bindings, payload
headers and active chunk storage. application_id is 0x50414354 and user_version
is private marker 0. All tables are STRICT and WITHOUT ROWID; correctness does
not depend on rowid, insertion order, timestamps or query-plan iteration.

The complete baseline DDL above owns this representation; there is no historical SQL upgrade step.

`instance_name` and `input_identity` store exact UTF-8 bytes as BLOBs. The
repository MUST validate their Domain syntax when writing and loading.
`protection_rank` is the closed semantic rank `Normal = 0`, `Secret = 1` and is
the persisted sticky protection floor for bindings that reference the payload.
`instance_id`, `payload_id`, and `instance_state_version` are independently
generated opaque 128-bit values. A payload identifier MUST be instance-scoped,
MUST NOT be derived from bytes, and MUST NOT be reused for a replacement.

One payload header plus its chunks is the only persistent payload
representation. An empty payload has `byte_length = 0` and no chunk rows. A
non-empty payload has chunk indices contiguous from zero; every non-final chunk
has exactly 1,048,576 bytes, the final chunk has 1 through 1,048,576 bytes, and
their concatenated length equals the header length. The maximum is
536,870,912 bytes. The row-local constraints above reject duplicates,
negative indices, empty chunks, oversize chunks, and oversize headers. Because
SQLite row checks cannot prove cross-row contiguity and aggregate length, the
typed repository MUST validate those invariants before commit and on every
logical load. A gap, non-final short chunk, length mismatch, impossible
protection rank, or invalid Domain string is corruption, not another
representation.

Logical load MUST also compare a binding with the strict-decoded active
Revision. If the Input is active and declared Secret, a referenced Normal
payload is corruption because the required promotion was not durably
published. Active Normal with a Secret payload is valid sticky state, not a
mismatch. An Input identity absent from the active Revision is retained; its
payload rank alone defines effective protection and MUST NOT be compared with a
nonexistent declaration. This is the complete V3 binding/payload protection
validation rule.

Payload headers and chunks are immutable after publication. Replacement MUST
insert a fresh payload and atomically change the binding pointer; it MUST NOT
rewrite an existing payload, deduplicate equal bytes, or persist a content
digest. `Active`, `retained`, `required`, and readiness are derived from the
strict-decoded active Revision and MUST NOT appear as columns. Binding absence
is represented only by no binding row.

Initial publication stores the active declaration's protection. Replacement
stores the stronger of the previous payload protection and the active
declaration. Any operation that promotes an existing binding MUST publish a
fresh Secret payload because it cannot mutate an existing payload header.
Future Migration coordination must establish the same committed invariant
before target publication, without this V3 contract defining M5 execution.

Every deterministic Instance enumeration MUST order by exact
`instance_name`, then `instance_id`, using BLOB byte order. Binding enumeration
MUST order by `input_identity`; chunk reads MUST state `ORDER BY chunk_index`.
Domain code MUST verify SQL/Domain ordering parity after load. Payload lookup
is by the complete `(instance_id, payload_id)` key and never by physical row
order.

The active Revision foreign key prevents deletion while a live Instance refers
to it. Instance deletion remains M7 behavior; the cascades define relational
cleanup only after a separately authorized Instance deletion transaction.
Current bindings, in-progress ExportInput observations, and future durable pin
or recovery references govern payload reachability. V3 does not add those
future reference kinds or a generic GC table.

**Verification: PR-TEST-0075.**

### PR-REQ-0270 - Persistence migration to V3

Retired by the approved E one-time baseline reset on 2026-09-26.
Development-era persistence upgrades are unsupported. Shared exact-schema,
serialized admission, bootstrap crash safety and preservation obligations now
belong to PR-REQ-0078 and PR-REQ-0299; this historical upgrade is not a
compatibility promise for the new baseline.

**Verification: Not applicable — retired requirement, not pending runtime coverage.**

## Candidate crate-private repository contract

The M2 implementation derives typed operations equivalent to:

```text
create_instance(name, revision, initial_payloads)
list_instances() -> ordered InstanceSummary[]
load_instance(instance) -> InstanceView
list_inputs(instance) -> ordered ManagedInputView[]
set_input(instance, input, expected_version, staged_payload)
delete_input(instance, input, expected_version)
acquire_export_observation(instance, input, authorization) -> staged payload
```

Creation and mutation use `BEGIN IMMEDIATE`. The repository revalidates the
exact active Revision, Input declaration, role, stored/effective protection
invariant, and expected state version inside the transaction. It streams one-
megabyte chunks from transient staging and does not require one giant SQLite
bind buffer. A transaction failure publishes no header, chunk, binding,
Instance, or new state version. The concrete Rust names remain crate-private
and are not a stable API.

## Deferred work

V3 does not define Action execution, durable pins, Snapshot persistence,
Migration execution, recovery state, Instance deletion UX, ServiceStorage,
stable public APIs, payload encryption, secure erasure, generic garbage
collection, or a backup format.


### PR-REQ-0275 - Exact PersistenceSchemaV4

The complete baseline includes managed Run identities, invocations, execution
owners, pins, outcomes, failures, completion, artifacts and recovery guards.
It retains application_id 0x50414354 and private user_version marker 0. All
relations use the exact STRICT/WITHOUT ROWID definitions above, never a schema
extension performed while opening an older development store.

The complete baseline DDL above owns this representation; there is no historical SQL upgrade step.

`run_id` is an independently generated opaque 128-bit value. It MUST NOT be
derived from the Plan, the Instance, the Action, or a timestamp, and MUST NOT
be reused. `action_identity`, `input_identity`, and `output_identity` store
exact UTF-8 semantic identifiers; `error_owner` and `error_code` store exact
UTF-8 Pactrun error names; `code_utf8` stores an exact UTF-8 `HookCodeV1`
semantic identifier; `message_utf8` stores exact UTF-8 text; `owner_session`
stores the exact staging session directory name selected by PR-REQ-0277. The
repository MUST validate each Domain syntax when writing and loading.

Closed ranks are the only representation of their vocabularies. `outcome_rank`
is `Succeeded = 0`, `Failed = 1`, `Cancelled = 2`, `TimedOut = 3`,
`Interrupted = 4`; the row check proves that `ManualRecoveryRequired` is not a
Run outcome. `admitted_rank` is `Accepted = 0`, `Admitted = 1`. `risk_state` is
`Clear = 0`, `Open = 1`. `status_rank` is `success = 0`, `failure = 1`.
`failed_step` is `Admission = 0` followed by the Action Plan steps
`EstablishSession = 1`, `LaunchHook = 2`, `AcceptCompletion = 3`,
`PublishDeclaredOutputs = 4`, `Finalize = 5`. `trigger_rank` is
`OpenRiskFailure = 0`, `OpenRiskOwnerLoss = 1`, `SuccessWithOpenRisk = 2`.

Optional state is represented only by row presence. A Run is `Running` if and
only if its `run_executions` row exists and `Finished` if and only if its
`run_outcomes` row exists; exactly one of the two MUST exist. A Running Run is
`Admitted` if and only if its `run_revision_pins` row exists. A Finished Run's
boundary is the recorded `admitted_rank`; released pins MUST NOT be used to
classify a Finished Run. Every Run MUST have exactly one
`run_action_invocations` row. That row is the complete Action-specific
operation record and the operation-kind discriminator; a later schema adds
other managed-execution kinds as their own rows rather than a kind column.

`run_action_invocations` records the exact historical Revision and Action
identity without a foreign key. Only `run_revision_pins` references
`revisions`, with `ON DELETE RESTRICT`, so a durably pinned Revision cannot be
deleted while historical Run identity never prevents deletion. `run_payload_pins`
references `managed_input_payloads` with `ON DELETE RESTRICT`, so a pinned
payload cannot be reclaimed. Payload reachability is a current binding row or
an execution pin row; the repository MUST reclaim a payload only when neither
exists and MUST decide that by explicit lookup rather than by relying on a
foreign-key failure. `runs` references `instances` with `ON DELETE RESTRICT`;
Instance deletion remains M7 behavior and MUST address Run history explicitly.

Execution pins are established only for an accepted Run, atomically with the
token-first comparison of the current `InstanceStateVersion` against the
accepted state version, and only while the Run is Running and unpinned. The
pin set for an Action Run is exactly one Revision pin and one payload pin per
active binding referenced at admission. Pins are released in the same
transaction that publishes the terminal outcome. This Action-specific lifetime
is admissible because the recovery reference of an Action obligation is the
Run record itself, never the pinned bytes: no Pactrun-owned recovery action
reads pinned content, and Hook replay is forbidden. Pins are internal lifetime
bookkeeping; they MUST NOT block `SetInput`, `DeleteInput`, or any other
management mutation, and MUST NOT be presented as a lease or guard.

`run_executions.risk_state` is the live protocol risk state of a Running Run.
It MUST change only `Clear` to `Open` on durable risk entry and `Open` to
`Clear` on durable risk resolution, and only while the Run is Admitted.
`run_outcomes.risk_state` is the terminal risk state copied from the live state
in the terminal transaction and retained as provenance. The Action recovery
directive is fixed by the operation kind: an Action changes no
Pactrun-authoritative Instance state, so the only Pactrun-owned recovery action
is applying the open-risk Instance consequence. No directive column exists.

The terminal consequence is derived in Domain code from the outcome, the live
risk state, and the Hook completion, in this order: `Clear` risk yields no
consequence; `Open` risk with `Succeeded` is an invalid transition that
publishes nothing; `Open` risk with a Hook completion whose status is success
yields `SuccessWithOpenRisk`; otherwise `Open` risk with `Interrupted` yields
`OpenRiskOwnerLoss`; every remaining `Open` non-success yields
`OpenRiskFailure`. A consequence publishes an `instance_recovery_guards` row
and a fresh `InstanceStateVersion` in the terminal transaction. The guard row
exists if and only if the Instance is in `ManualRecoveryRequired`; its
`run_id` references `run_outcomes` with `ON DELETE RESTRICT` so the triggering
Run remains a strong recovery reference while the obligation is unresolved. If
a guard already exists when another open-risk Run finishes, the existing row
MUST remain unchanged and no new state version is published; the newer Run's
terminal `risk_state` records the fact. Run creation, pin establishment, risk
transitions, and a terminal publication without a consequence MUST NOT publish
a new `InstanceStateVersion`. `ResolveManualRecovery` is a token-first
management mutation that deletes the guard row and publishes a fresh state
version.

Hook completion `code` and `message` use the presence representation. An
absent message is `message_present = 0` with empty bytes; a present empty
message is `message_present = 1` with empty bytes; a present message stores its
exact bytes. An absent code is `code_present = 0` with empty bytes; a present
code is `code_present = 1` with a non-empty valid `HookCodeV1`. No byte-length
bound applies to completion or failure messages. Failure records are Pactrun
error references; a Hook-reported failure keeps its Hook-owned completion and
does not require a Pactrun primary failure.

Run Artifacts use the exact payload physical representation: a header plus
contiguous chunks from index zero, every non-final chunk of exactly 1,048,576
bytes, a final chunk of 1 through 1,048,576 bytes, concatenated length equal to
the header length, and a 536,870,912-byte maximum. An empty Artifact has
`byte_length = 0` and no chunk rows. The typed repository MUST validate
cross-row contiguity and aggregate length before commit and on every logical
load. Artifacts reference `run_outcomes`, so an Artifact can exist only for a
Finished Run; a terminal publication MAY store any subset of the declared
outputs, a failed Run MAY retain submitted files as diagnostic Artifacts, and an
Artifact MAY be deleted independently of the retained Run record.

Every deterministic Run enumeration MUST order by `instance_id`, then `run_id`,
using BLOB byte order. Secondary failures MUST order by `ordinal`; chunk reads
MUST state `ORDER BY chunk_index`. Domain code MUST verify SQL/Domain ordering
parity after load. `accepted_at_unix_ms`, `finished_at_unix_ms`, and
`entered_at_unix_ms` are informational timing values; they MUST NOT carry
identity, ordering, or correctness.

Logical load MUST treat the following as corruption rather than another
representation: a Run with both or neither of its execution and outcome rows;
a Run without exactly one `run_action_invocations` row; a payload pin whose
`instance_id` differs from the Run's Instance; a `Succeeded` outcome with a
primary failure, with a Hook completion whose status is failure, or with
terminal risk `Open`; a guard whose Run has terminal risk `Clear`; a chunk
gap, non-final short chunk, or length mismatch; an invalid Domain string; or an
`owner_session` that is not a valid staging session name.

A Running Action Run MUST be in exactly one of these boundary/risk states:
`Accepted + Clear`, `Admitted + Clear`, or `Admitted + Open`. Recovery risk
MUST NOT become `Open` before Admission. A persisted `Accepted + Open` Running
Run is impossible and corrupt. Logical loading, recovery, and terminal
transitions MUST reject it without publishing an outcome, changing the Instance
recovery guard or state version, releasing references, or normalizing it into a
valid state.

V4 has no dedicated sensitive-value fields: no table stores invocation
parameter values, Secret payload bytes, or value-derived digests for a Run.
This is a schema fact only. Runtime redaction of Hook-authored and
Pactrun-authored diagnostic text remains the obligation of the slices that
produce Run text.

**Verification: PR-TEST-0084, PR-TEST-0087, PR-TEST-0104, PR-TEST-0105, PR-TEST-0112, PR-TEST-0452.**

### PR-REQ-0276 - Persistence migration to V4

Retired by the approved E one-time baseline reset on 2026-09-26.
Development-era persistence upgrades are unsupported. Shared exact-schema,
serialized admission, bootstrap crash safety and preservation obligations now
belong to PR-REQ-0078 and PR-REQ-0299; this historical upgrade is not a
compatibility promise for the new baseline.

**Verification: Not applicable — retired requirement, not pending runtime coverage.**

## Candidate crate-private repository contract

The M3 Slice 2 implementation derives typed operations equivalent to:

```text
create_accepted_run(instance, accepted_state_version, action_identity, owner_session) -> RunId
admit_run(run, admission_facts, launcher_check, override_guard) -> admitted | refusal
open_recovery_risk(run)
clear_recovery_risk(run)
finish_run(run, outcome, failures, hook_completion, staged_artifacts) -> published state version?
finish_run_owned(owner, run, outcome, hook_completion, submitted_outputs, staged_artifacts)
    -> published state version?
advance_owner_continuation(run) -> durable terminal published | owner retry retained
resolve_manual_recovery(instance, expected_state_version) -> InstanceStateVersion
list_runs(instance) -> ordered RunSummary[]
load_run(run) -> RunView
load_instance_recovery_guard(instance) -> RecoveryGuardView?
load_run_inspection(run) -> RunInspectionData
reconcile_action_runs() -> reconciled RunId[]
open_run_artifact(run, output) -> streamed bytes
delete_run_artifact(run, output)
stream_admitted_payload(run, input) -> streamed pinned bytes
open_admitted_runtime_blob(run, runtime_file) -> verified pinned bytes
```

Every mutation uses `BEGIN IMMEDIATE`. Run creation records the accepted state
version without comparing it. Admission evaluates, inside one transaction and in
the order of PR-REQ-0279, the recovery guard, the current Instance state
version against the expected and accepted versions, the active Revision, every
referenced current binding, readiness derived from the persisted bindings and
Revision declarations, runtime-content availability, interpreter launcher
re-selection, and the Mutate-conflict predicate. Both access modes the predicate
compares are read from the persisted `run_action_invocations` identity and the
decoded Revision core, and the predicate itself is derived from existing
`run_executions`, `run_revision_pins`, and `run_action_invocations` rows, so no
schema change is needed. A refusal publishes the terminal `Failed` outcome in
that same transaction; success inserts the pins. Terminal publication computes the
consequence before writing, deletes the execution row, writes the outcome and
its records, releases pins, reclaims unreferenced payloads, and publishes the
guard and state version only when a consequence exists. Artifact bytes stream
from transient staging in one-megabyte chunks. The two pinned-view readers
added by M3 Slice 4 resolve an admitted Run's payload and runtime-content pins
from the existing `run_payload_pins`, `run_revision_pins`, and
`revision_runtime_content_refs` rows and refuse a Run that is not Running and
Admitted; they add no schema. A transaction failure publishes no Run, pin,
risk transition, outcome, Artifact, guard, or state version. The concrete Rust
names remain crate-private and are not a stable API.

M3 Slice 5 composes the crate-private owner continuation and finalization
substrate over V4: eligible Action output bytes are independently staged,
execution cleanup is attempted before the terminal transaction, and the
Run outcome, failures, Hook structural result, and Artifacts are published
atomically. The production finalizer retains Hook completion status while
omitting Hook-authored code and message text; generic historical V4 reads keep
their exact optional-value semantics. The explicit owner-loss reconciler reads
the durable owner and risk state, confirms the staging lease, and finishes only
confirmed-lost valid Running Action records. `load_run_inspection` reads a Run
and its Instance's current recovery guard from one SQLite read snapshot. These
additions do not alter the V4 table manifest or expose a public inspection or
CLI surface.

## Deferred work

V4 does not define non-sensitive parameter recording, Run retention policy,
Snapshot, Migration, Restore, or Cleanup execution records, Instance deletion,
generic garbage collection, Artifact export, ServiceStorage, a stable public
API, or human spelling.


### PR-REQ-0298 - Exact PersistenceSchemaV5

The baseline contains durable writable admission, exact operation kinds,
Capture/Restore invocations, recovery-consequence versions, Snapshot manifests
and immutable blob references, Restore admissions and Capture results. The old
inline Snapshot chunk relation is retired. The public format is the metadata
string; user_version and admitted_schema_version remain private marker 0.
All tables are STRICT and WITHOUT ROWID. Rowid, clocks and query plans do not
establish correctness.

The complete baseline DDL above owns this representation; there is no historical SQL upgrade step.

### Logical loading and mutation invariants

- operation_kind is Action=0, Capture=1, Restore=2. Every Run MUST have exactly
  one discriminator and exactly one matching invocation row; extra or missing
  variants are corruption. Existing Action ranks and optional-value encodings
  remain unchanged. Failed-step ranks 0..5 retain their Action meanings and map
  to Admission, establish Session, launch Hook, accept completion, publish
  managed result, and finalize for Snapshot operations.
- Every Instance MUST have one nonnegative signed-64-bit consequence counter.
  Increment is checked; exhaustion fails closed, never wraps. Only a newly
  committed open-risk terminal consequence advances it. Risk requests alone,
  successful Restore, and migration do not. Counter advancement, outcome, and
  guard consequence are one transaction even when an existing guard remains.
- A Running Run remains Accepted or Admitted as established by its exact
  Revision pin. Accepted+Open is corrupt. Access comes from its exact/pinned
  authoritative capability, not operation_kind or a caller. Action pins remain
  active-only; Capture pins the complete admitted registry. Roles/absence are
  derived from that set and the immutable pinned Revision, not current rows.
- An Admitted Running Restore MUST have exactly one run_restore_admissions row;
  other operations and Accepted-only Restore MUST have none. Its SnapshotId
  equals its invocation, state token equals the accepted/revalidated token,
  producer matches the pinned Revision, and its Snapshot foreign key is the
  execution strong pin. No old-target payload pin is required for rollback:
  Restore changes no target bindings before terminal commit and never rolls
  back concurrent management changes.
- Restore's terminal transaction MUST compare both retained tokens before any
  successful target publication. Snapshot content may exceed target Managed
  Input capability; check before launch, never truncate or silently promote.
  Restore creates fresh target-instance-scoped payload IDs under the existing
  Managed Input contract. Terminal execution-row deletion releases the
  Snapshot admission pin in the same transaction.
- A successful Capture MUST publish exactly one result reference together with
  its Snapshot and outcome. Unsuccessful Capture MUST publish none. The result
  and Restore invocation SnapshotIds are historical references, not lifecycle
  ownership: no foreign key may permanently retain the Snapshot through them.
- Snapshot root identity, format, and digest MUST agree with its exact canonical
  manifest. The manifest is authority for producer, origin, capture time,
  bindings, and service descriptors. Do not add a divergent authoritative
  registry. Snapshot roots have no lifecycle foreign key to Instance, Run,
  or installed Revision.
- The blob keys MUST equal the exact manifest closure, with one header per
  digest in that Snapshot. Chunks are contiguous from zero; each non-final
  chunk is 1,048,576 bytes, the final nonempty chunk is 1..1,048,576 bytes, and
  aggregate length equals the header. Empty blobs have no chunks. Check digest
  and exact closure before publication; full verification streams all bytes.
  All chunk reads specify ORDER BY chunk_index. Snapshot enumeration orders by
  SnapshotId BLOB bytes. Cross-Snapshot/Instance/Artifact CAS is not introduced.
- Capability limits are checked independently of these storage invariants.
  Do not add the former 8 GiB product ceiling to the schema or call an otherwise-valid
  over-capability object corrupt. Managed Input and Artifact V4 limits remain.
- Canonical manifests and digests may contain sensitive authoritative material.
  Run records MUST NOT copy this content. Inspection projects safe structural
  fields rather than dumping rows or library errors.
- Durable staging is not a result or a reconciler-committable PreparedCommit.
  Owner loss before terminal commit finishes Interrupted, never success.
  Orphan recovery depends on operation, boundary, risk, and references, not a
  persisted Plan. Corrupt states are not normalized into valid ones.

**Verification: PR-TEST-0182, PR-TEST-0183, PR-TEST-0195, PR-TEST-0202, PR-TEST-0203, PR-TEST-0204, PR-TEST-0208, PR-TEST-0209, PR-TEST-0210, PR-TEST-0212, PR-TEST-0213, PR-TEST-0215, PR-TEST-0222, PR-TEST-0223, PR-TEST-0224, PR-TEST-0226, PR-TEST-0228, PR-TEST-0229, PR-TEST-0230, PR-TEST-0232, PR-TEST-0233, PR-TEST-0234, PR-TEST-0236, PR-TEST-0239, PR-TEST-0240, PR-TEST-0246, PR-TEST-0247, PR-TEST-0252, PR-TEST-0255, PR-TEST-0258, PR-TEST-0259, PR-TEST-0260, PR-TEST-0263, PR-TEST-0264, PR-TEST-0267, PR-TEST-0275, PR-TEST-0480.**

Coverage includes exact DDL, initialization, operation discriminators,
consequence counters, legacy preservation, and Snapshot-owned byte loading,
verification, atomic import, and export. Partial S4 coverage additionally
checks competing operation/invocation consistency, exact Revision pins, and
owner-checked Snapshot interruption with atomic consequence advancement and
pin release. Further S4 tests exercise typed Snapshot acceptance, operation-aware
failed-step ranks, complete Capture registry pins, Restore admission-link
corruption, unknown/live/lost owners, and real-process Capture/Action admission
arbitration. Capture/Restore successful result publication and Hook execution
remain later-slice work. S4 additionally proves real Restore admission
qualification, before/after-commit crash boundaries, one selected immutable
Snapshot strong pin, exact state/consequence tokens (including nonzero counters),
and owner-held retry/cancellation without creating another Run. No DDL or rank
mapping changed to implement those paths.

### PR-REQ-0299 - Durable writable admission and serialized migration

An operation-scoped session directory and its lease are durable pre-admission
ownership evidence, not permission to write any schema version. Only a
committed writable_admissions row grants writable qualification. Session
preparation MUST first refuse unsupported existing stores, but MUST NOT require
an admission row or block ordinary admission-aware
coordination merely because its general lease is held.

Pre-admission preparation is limited to the current operation's resources.
Cross-session housekeeping and publisher maintenance MUST wait until the
supported exact schema is established and writable admission is committed.
An opener that refuses an incompatible schema does not gain authority to clean
that schema's abandoned operation data.

After acquiring the same SQLite serialized write boundary used for admission,
a writer MUST revalidate the exact supported schema before committing its
admission or performing schema-dependent writes. A pre-lock check is advisory.
The row MUST identify the exact generated session and schema version. The lease
MUST remain held for the entire writable qualification lifetime. Ending the
qualification MUST first prevent all further schema-dependent writes, then
revoke its admission and release connection/lease resources. If orderly
revocation fails, a stale record remains owner-lost rather than silently live.

Migration MUST inspect committed admissions under that serialized boundary:
live or unknown admitted owners block; confirmed owner loss does not. Missing
admission in admission-aware storage MUST NOT trigger a fallback scan of all leases.
An orphan Run does not require a fabricated writer admission. Removing stale
writer qualification MUST NOT reconcile or infer any Run outcome.

**Verification: PR-TEST-0195, PR-TEST-0196, PR-TEST-0197, PR-TEST-0198,
PR-TEST-0201, PR-TEST-0622, PR-TEST-0623, PR-TEST-0639.**

### PR-REQ-0300 - Exact V4 to V5 legacy bootstrap

Retired by the approved E one-time baseline reset on 2026-09-26. Development
schema upgrades are no longer supported. Stable IDs PR-TEST-0199 and PR-TEST-0200
are retired, not reused. Shared writer admission, crash-boundary and recovery
invariants remain covered under PR-REQ-0299 and the new baseline.

**Verification: Not applicable — retired requirement, not pending runtime coverage.**


### PR-REQ-0308 - V6 Migration persistence and upgrade boundary

The baseline preserves operation ranks and stored meaning, including the
distinct Migration discriminator. Durable Migration state MUST identify the
exact invocation, ordered edges, current edge/step, committed-edge evidence,
binding references, and the last committed recovery boundary. The final edge
and Run success share the atomic boundary of PR-REQ-0306. Recovery references
MUST remain strong roots while reconciliation or an unresolved obligation needs
them; temporary file presence is not durable publication evidence.

Only pristine initialization and exact supported-baseline admission are
implemented. Development schemas, foreign ownership, unmarked populated files,
partial schemas and drift are refused before coordination or writes. Admission
revalidates under SQLite serialization; a stale prepared writer cannot continue
once its contract no longer matches. Live or unknown owners are not stolen or
force-unlocked. Reopening preserves identities, bytes, bindings, metadata, Runs,
pins, risk, guards, completions, Snapshots and Artifacts; it does not reconcile,
replay, invent an operation kind or infer terminal disposition.

**Verification: PR-TEST-0304, PR-TEST-0327.**

Development upgrade-only tests are retired with their readers. The retained
Migration commit/recovery, pin-lifetime and discriminator tests are rebased onto
the current baseline; their earlier historical pass results are not reused as
proof of the new implementation.

### PR-REQ-0311 - Exact PersistenceSchemaV6 representation

The baseline preserves the closed operation ranks: 0 Action, 1 Capture,
2 Restore, 3 Migration, and the separately specified Deletion rank. Its seven
Migration relations record exact invocation, edges, progress, boundaries,
revision/payload pins and checkpoint bindings. Foreign keys remain enabled.
There are no temporary rebuild tables, table-copy steps or development upgrade
gates. First publication atomically creates the complete baseline DDL.

The complete baseline DDL above owns this representation; there is no historical SQL upgrade step.

Migration invocation stores an exact same-lineage source/target and the
declassification authorization, never a CLI path ID or host acquisition path.
Edges are zero-based and ordered; their source/target pairs describe the exact
accepted sequence. No FK to installed Revisions is inferred at acceptance;
execution pins belong to successful Admission.

The progress/checkpoint representation is established at Admission, not merely
acceptance. committed_edge_count counts published edge boundaries; step ranks
0..4 mean establish Session, launch Hook, accept completion, publish managed
result, and finalize. Boundary rows record the target and state version of each
successful edge. The progress boundary is the initial admitted source at count
zero or the latest committed edge afterward. It is historical boundary evidence,
not a claim that the Instance cannot subsequently change. The checkpoint holds
the complete present-binding registry at that boundary; absence and roles are
derived from the associated exact Revision and the single registry. Payload
protection remains owned by immutable managed_input_payloads, not duplicated
or downgraded in checkpoint metadata.

Revision, payload, and checkpoint references MUST prevent premature collection.
The payload reclaimer MUST check Migration pins and checkpoint bindings as well
as ordinary bindings and existing execution pins, instead of failing a whole
mutation on an avoidable FK violation. Necessary recovery references survive
terminalization while an unresolved obligation still needs them; logical
admission, per-edge commit, and lifetime transitions remain runtime work under
PR-REQ-0306, not claims established solely by these SQL constraints.

Matching rank-3 invocations MUST be decoded as Migration rather than inferred
as Action. Mixed invocation kinds and inconsistent path/boundary/reference data
MUST be rejected. Execution capability is separate from structural inspection;
Hook-backed execution remains unsupported by this slice.

**Verification: PR-TEST-0293, PR-TEST-0299.**

### PR-REQ-0312 - Exact V5-to-V6 admission-aware upgrade

Retired by the approved E one-time baseline reset on 2026-09-26.
Development-era schema upgrades are unsupported. Shared writer admission,
owner-loss, transactional bootstrap and identity/recovery preservation remain
requirements of the new Persistence baseline; no historical upgrade is promised.

**Verification: Not applicable — retired requirement, not pending runtime coverage.**


### PR-REQ-0323 - Exact V7 additions

The baseline contains the typed service allocation, preparation, protection,
origin, association, pin, target and committed-edge relations below. The public
contract is the exact Persistence string and complete DDL; no upgrade wrapper
publishes another integer schema version. Allocation and association publication
remain owned by their existing transactional and filesystem boundaries.

The complete baseline DDL above owns this representation; there is no historical SQL upgrade step.

All identifiers stored as BLOB are strict-decoded Domain IDs, not arbitrary
bytes that merely pass SQL lengths. Associations do not duplicate locator,
kind, storage identity (for resources), exposure or mutable presence: their
exact declaration Revision plus typed identity supplies that contract. This is
a dedicated typed service-resource relation, not ManagedInputBindings or EAV.

### Logical invariants and readers

- Every allocation has exactly one preparation or protection row, never both
  and never neither at a committed boundary. Instance publication, target
  association publication or any external path grant requires protection.
  A preparation's Revision identity equals its allocation's origin identity
  and pins that exact declaration until protection/publication; after promotion,
  Instance associations or the Running Migration's path pins own needed contracts.
- The immutable allocation owner InstanceId is intentionally not an Instance FK:
  pre-Instance allocation and future non-destructive abandonment must not erase
  physical custody. At creation its exact origin Core must declare the storage
  in that Package. Origin Revision identity is thereafter audit provenance,
  not an installation-lifetime FK. Physical paths and service bytes are absent
  from SQL. Removing an origin Revision cannot remove allocation custody.
  In V7 a protected allocation still requires an extant owner Instance; only
  an unexposed creation preparation may precede that Instance. A missing owner
  is corruption, not permission to delete bytes. The later M7 schema must add
  explicit detached-custody evidence before making owner removal a valid state.
- Each published association belongs to an extant Instance with the same Package
  and to an allocation with that same immutable owner. Its declaration Revision
  must contain that typed declaration. Current declared identities have exactly
  one association referring to the current Core; unmapped undeclared identities
  retain the last published contract. Mapped sources are consumed/replaced
  atomically under PR-REQ-0319. Never infer an association from a path.
- An active resource's allocation equals its active storage association's
  allocation. A retained resource directly retains its historical allocation,
  even if a later storage association changes. Distinct active storages cannot
  alias one allocation; a Storage identity rename consumes the source association
  rather than preserving a second Storage alias. Resource references can still
  keep an allocation alive independently of its current Storage association.
- run_service_storage_pins attach only to Admitted Runs. They include every
  granted/read prerequisite/source dependency allocation and all prepared targets
  of the currently prepared Migration edge. Admitted target rows belong only to
  that current uncommitted edge and exactly cover its target declarations.
  Each resource target matches the allocation selected for its declared storage.
- Preparing a later edge happens after the preceding edge commit. Initial
  whole-chain symbolic preflight does not expose or allocate all future roots.
  Existing whole-path Revision pins remain; retained resource declaration
  Revisions are strongly held by their published associations throughout the
  continuing Mutate Run. Instance management cannot change them behind the Run.
- On edge commit, remove consumed source associations, apply target associations and mark all target allocations
  protected before removing that edge's target rows. Retain execution pins until
  terminal publication; their release never removes protection/custody rows.
- A committed transform edge has one run_service_edge_commits row, inserted in
  the same transaction as run_migration_boundaries. This is historical committed
  evidence, not an uncommitted target-ready/replay flag. It cannot authorize
  completion or clear risk when loaded by a new owner. Non-transform edges have
  no row. While Running, exact pinned Core validates this relationship; historical
  validation after Revision removal is structural unless that Core is available.
- No synthetic V1 successful Hook completion is stored for a target proposal.
  V7's transform evidence plus the committed boundary explains it. Ordinary
  completion records retain their historical meaning; terminal risk remains
  Clear after committed successful transformation. Any pre-existing guard remains.
- origin_run_id is audit identity only and intentionally has no Run FK; allocation
  custody must survive later Run retention policy. It is set only at creation
  by that accepted Migration, never reassigned. Unresolved recovery guards retain
  their existing strong Run roots independently. Published association Revision
  FKs intentionally pin the exact contracts needed for access/reattachment;
  origin Revision/Run audit IDs alone do not pin obsolete installations/history.

Strict readers reject drift, wrong owners, mixed associations, invalid typed
IDs, missing protection and impossible operation/boundary matrices. They must
not repair them by scanning files or normalizing into Interrupted. Presence is
always a fresh observation outside the authoritative database; changing live
bytes or protection bookkeeping alone does not advance InstanceStateVersion.

**Verification: PR-TEST-0340, PR-TEST-0346, PR-TEST-0347, PR-TEST-0349, PR-TEST-0350, PR-TEST-0353, PR-TEST-0368, PR-TEST-0374, PR-TEST-0375, PR-TEST-0376, PR-TEST-0377, PR-TEST-0378, PR-TEST-0380, PR-TEST-0382, PR-TEST-0383.**

PR-TEST-0340 covers exact SQL and structural FK behavior. The allocation,
association and real operation tests separately cover Domain invariants;
SQL shape alone is not proof of those invariants.

### PR-REQ-0324 - Allocation and protection protocol

Generate a cryptographically random, non-reused 128-bit allocation ID. In this
initial implementation profile its physical root is
`<Pactrun storage root>/service-storage/alloc-<32-lowercase-hex>`; no user path is
accepted as an allocation source. This mapping is a local persistence/runtime
layout, not Revision identity. Keep the path for the allocation lifetime;
moving the Pactrun storage root or attaching existing directories is unsupported
without a separately designed operation.

1. Under serialized writer admission, persist allocation plus preparation owner
   before filesystem creation. For a Migration, persist its audit Run origin and
   the target selection under the accepted, admitted owner.
2. Create only the private allocation root and complete the supported platform's
   directory persistence barriers. Never overwrite or adopt a pre-existing entry
   at a newly generated allocation path; fail safely and retain diagnosis.
3. For Instance creation, publish the Instance, all storage/resource associations
   and protection rows atomically, removing their preparation rows. The physical
   roots must already exist. For a Migration, protect newly exposed targets before
   Session disclosure; published source/retained allocations are already protected.
4. Any protection promotion is monotonic in M6.5. Deleting an execution, stage
   row, association or stale writer cannot remove the protection row or allocation.
   No service contents are moved into or mirrored by managed_input_payloads.

No SQLite transaction is held across service execution. The protocol is ordered
filesystem/SQLite publication, not a cross-system transaction. A missing or unsafe
root for a protected allocation is an operational failure; never silently recreate
it and thereby claim original live state was restored. Whole-storage write grants
authorize contents, not renaming/deleting the allocation root itself.

Directory creation success alone is not the durability barrier. If the supported
Windows fixed-NTFS or existing POSIX storage backend cannot complete its required
namespace persistence primitives, allocation must fail before publication/path
exposure; no weaker filesystem fallback is allowed. S2 must demonstrate the
actual adapter barriers with filesystem crash tests rather than infer them from
the in-memory DDL check or a successful mkdir call.

After confirmed preparation-owner loss, recheck under the allocation's internal
maintenance exclusion that it has no protection, published association, Running
pin or target reference. Remove only a known empty directory via non-traversing
rmdir and required barriers; then remove preparation/origin/allocation records
atomically. A missing directory is an idempotent empty-preparation case.
Unexpected contents, linked/unsafe paths or inconclusive ownership preserve the
records. Unknown/unrecorded directories are not automatically adopted or deleted.
No startup cleanup traverses protected storage. M7 finalization is the only future
destructive lifecycle, under its separately approved receipt protocol.

### Crash matrix

| Last durable event | Recovery |
| --- | --- |
| allocation intent; no directory yet | owner-live/inconclusive: leave; confirmed loss and no references: remove empty preparation records |
| directory created; no Instance publication or protection | remove only proven empty preparation after confirmed loss; otherwise preserve |
| Instance publication | all declared associations and protection survive; never expose a partial Instance |
| target protection/path exposure; no target commit | preserve allocation and origin evidence; no candidate adoption, target commit or service-byte cleanup |
| target_ready received; owner lost | no durable proposal authority; apply Open-risk manual recovery using last committed boundary |
| before edge transaction commit | all source/current associations remain; preserve exposed target allocation, do not replay |
| after edge transaction commit | target associations/boundary/risk clear agree; final edge is already Succeeded, otherwise later loss interrupts at the new boundary |
| empty directory removed; preparation record still exists | repeat absent-directory reconciliation; never promote to protected or infer a Run outcome |

**Verification: PR-TEST-0345, PR-TEST-0346, PR-TEST-0347, PR-TEST-0348, PR-TEST-0351, PR-TEST-0352, PR-TEST-0376.**

This is partial runtime evidence: no-replace, root-relative creation with adapter
barriers, SQL intent/protection, eager isolated Instance allocation and atomic
association publication under process interruption, and rejection of missing or
replaced prepared namespaces. Preparation maintenance tests cover confirmed-loss
retirement of absent unreferenced intents and conservative preservation for live
or inconclusive owners and existing objects. An intent alone does not distinguish
a created root from a failed no-replace collision, so these tests do not claim
post-crash ownership proof or automatic removal of existing empty directories.
They do not establish Hook access, Migration publication or power-loss safety.

### PR-REQ-0325 - Explicit V6-to-V7 upgrade

Retired by the approved E one-time baseline reset on 2026-09-26.
Development-era schema upgrades are unsupported. Shared writer admission,
owner-loss, transactional bootstrap and identity/recovery preservation remain
requirements of the new Persistence baseline; no historical upgrade is promised.

**Verification: Not applicable — retired requirement, not pending runtime coverage.**


### PR-REQ-0338 - Exact V8 lifecycle records

The baseline owns the complete lifecycle relations above. Run
identity, outcome, child records and Artifacts MUST survive Instance removal.
The historical identity relation retains the last managed name as provenance,
not as a unique lookup key or permanent Revision pin. Creating a new Instance
MUST atomically create its history identity; reusing a name does not reuse an ID.

Deletion mode ranks are 0 ManagedCleanup and 1 AbandonManagement. Managed
execution kind 4 is DeleteInstance; ranks 0 through 3 keep their meanings.
Obligation phases are 0 launch authorized, 1 result unresolved and 2 finalization
authorized. Authority sources are 0 Hook completed, 1 operator confirmed and
2 no Cleanup declared. These are private persistence discriminants, not public
Instance states. Unknown ranks are corruption, not a fallback.

An obligation's attempt belongs to its Instance and has deletion mode 0. Phase 2
has exactly one authorization; phases 0/1 have none. A frozen work set belongs
to that authority and every allocation has the same immutable Instance owner.
An allocation may not be in two finalizations. No service bytes or native path
becomes authoritative database state. Receipt references preserve sufficient
provenance without permanently pinning obsolete Revision installation.

Retirement mode 0 requires all storage-lifetime obligations finished. Mode 1
requires explicit Abandon history and detached custody for surviving protected
allocations. Detached ownership must agree with allocation provenance. A
missing live owner without valid retirement/detached or completed-finalization
evidence remains corruption. Existing protected custody is never silently
converted into discard authority. Discard authority requires detached custody
and no active or unresolved resource/execution references.

`root_identity` records the physical incarnation before destructive I/O, not a
native path or content digest. Its 40 bytes contain five little-endian u64 words.
Tag 1 records the NTFS volume serial, file index, creation FILETIME and reserved
zero. Tag 2 records the Linux device major/minor pair (major in the high 32 bits),
inode, birth-time seconds and birth-time nanoseconds. Tags 1 and 2 are legacy
incarnation representations. Tag 3 has tag 2's native fields and requires the
Linux claim-isolated retirement backend; tag 4 has tag 1's native fields and
requires Windows namespace-pinned retirement. Tags 1/4 require the reserved
word to be zero; tags 2/3 require nanoseconds below one billion.

A legacy marker may be promoted only after qualifying the same native
incarnation, under the existing serialized operation. Commit the new marker
before relocation or removal. This makes an older reader reject an unsupported
backend requirement rather than mistake a relocated root for an absent one.
Changing the marker never changes the native identity or authorizes another
object. Cross-platform evidence is not an absence result. A platform without
reliable incarnation evidence MUST fail closed. Unknown tags are corruption. Initial
finalization work rows may have no identity before inspection; an observed absent
root is completed atomically instead of leaving unqualified authority that could
adopt later bytes. A pending discard requires a recorded physical identity.
An incarnation qualified during partial finalization remains binding after
Abandon and cannot be replaced by a later discard's newly observed identity.

The Linux `retirement-v1` area is durable physical progress for the same
AllocationId, not Workspace, a Snapshot, or a new source of deletion authority.
Every journal is bound to the V8-qualified root incarnation. Its versioned,
closed records retain native component names, parent provenance, qualified
object identity, original/claimed position and completion/move receipts.
Publish complete initial records atomically; publish claim intent before rename,
qualify what was actually captured, and establish durability before deletion.
Recovery must not fall back from a captured/completed position to a replacement
at its former name. Missing, malformed, foreign or contradictory progress must
fail closed; startup and ordinary maintenance must not execute it.

Allocation-scoped exclusive ownership covers physical progress. Explicit
read-only handoff cannot expose current processing positions while that owner
is active. Every resumed attempt reclaims survivors into fresh parents before
destructive use, because an earlier handoff may have exposed their old positions.
The control records remain Pactrun-authoritative metadata, separate from the
service-owned objects they account for. They are not an authoring format, and
no protection against deliberate concurrent alteration of Pactrun's own control
metadata is added beyond the existing trusted-local-store model.

Unexposed preparations are not proof that an existing directory belongs to
Pactrun. Freeze only protected allocations for physical finalization. Retirement
must not promote preparations into protection or detached discard authority.
Their intent/origin records retain the preceding conservative maintenance
rules, including preservation of existing or uncertain paths. An unresolved
preparation reference is not a permanent pin solely for historical provenance.

**Verification: PR-TEST-0392, PR-TEST-0400, PR-TEST-0408, PR-TEST-0416, PR-TEST-0419, PR-TEST-0420, PR-TEST-0421, PR-TEST-0422, PR-TEST-0427, PR-TEST-0436, PR-TEST-0438, PR-TEST-0441, PR-TEST-0445.**

These tests prove exact DDL, referential preservation and rejection of corrupt
legacy references. Logical lifecycle writers/readers and finalization runtime
require their own coverage; schema tests do not certify their implementation.

## Exact changes

Data-bearing table replacement uses a dedicated maintenance connection with
foreign-key enforcement disabled before BEGIN; never disable it on an admitted
runtime connection. Validate the full graph before committing and restore
foreign-key enforcement before further connection use. The table replacements
must not cascade through existing Run children.

The complete baseline DDL above owns this representation; there is no historical SQL upgrade step.

### PR-REQ-0339 - Explicit exact-V7 upgrade

Retired by the approved E one-time baseline reset on 2026-09-26.
Development-era schema upgrades are unsupported. Shared writer admission,
owner-loss, transactional bootstrap and identity/recovery preservation remain
requirements of the new Persistence baseline; no historical upgrade is promised.

**Verification: Not applicable — retired requirement, not pending runtime coverage.**


### PR-REQ-0345 - Lifecycle coordination activation

The supported [Persistence baseline](./persistence-baseline.md) incorporates the
[lifecycle coordination contract](../behavior/managed-object-lifecycle.md).
It adds no tombstone, GC queue or compact historical-evidence format. The public
version is the metadata string; the private admission marker is 0. Development
schemas are unsupported and no storage upgrade command remains.

The private `.collection.lock` in runtime-content coordinates ordinary shared
store openings against exclusive collection. It is non-authoritative OS
coordination, not an object, retention pin, saved Plan or GC work queue. Writable
bootstrap durably establishes it before use; read-only opening never
creates it. A read-only schema preflight rejects unsupported stores before
Pactrun content-coordination creation; SQLite's own read-coordination exception
is defined above. SQL admission revalidates under the content guard. Bootstrap
establishes the durable guard before publishing the baseline. Content coordination always
precedes SQL transactions, including writer admission.

Ordinary commands and read-only preview MUST NOT upgrade storage. Writable
admissions are bound to the exact supported contract and revalidated inside
each write transaction; a prepared session cannot write after the contract
changes. Unsupported or drifted sources fail without conversion or Pactrun
coordination creation. Reopening preserves objects, bytes, references, receipts and service
resources; it does not fabricate missing evidence or reconcile Runs.

**Verification: PR-TEST-0300, PR-TEST-0456, PR-TEST-0457, PR-TEST-0468, PR-TEST-0473.**


### PR-REQ-0347 - Immutable Snapshot and restored Input data references

New Snapshot payload bytes MUST be durably verified in the existing private
opaque blob store before the single Snapshot metadata transaction commits.
The historical `runtime-content` physical namespace MAY hold these immutable
values; this does not grant runtime membership, service authority, or a Revision
ownership relation. Physical addressing remains SHA-256; Frozen Snapshot,
Revision, Hook and error wire formats remain unchanged.

The complete relational definition is owned by the
[Persistence baseline](./persistence-baseline.md), not an ALTER ladder.


Every Snapshot blob reference names an immutable verified file by its digest.
No inline Snapshot representation, storage-kind fallback or conversion reader
remains. Missing/corrupt files fail; recorded lengths and delivered bytes are
checked independently.

A null Managed Input `content_digest` selects active inline chunk storage. A
non-null digest selects immutable bytes and MUST have no inline chunks. Restore
MUST publish fresh Instance-scoped payload IDs, complete binding replacement,
protection, both guarded conflict-token consequences and its terminal outcome
in the existing atomic boundary; it MUST NOT copy service-sized data into WAL.
Snapshot bound values already refer to immutable files; no legacy staging conversion precedes that boundary. Managed Input limits and sticky protection still apply.
Migration copying a payload with a changed protection floor MUST preserve its
representation and value while retaining the existing authorization rules.

The content-coordination guard MUST span blob publication through reference
commit, as it already does for runtime publication. A pre-commit crash MAY leave
unreferenced immutable files, never an authoritative partial Snapshot or Input.
GC MUST include Snapshot file references and all live/pinned Managed Input
payload references as strong roots, independently of producer/origin lifetime.
Deletion releases the logical reference; physical reclamation remains explicit
foreground GC. An unreliable reference catalog stops collection. Missing,
corrupt, conflicting or ambiguous data references MUST NOT be silently repaired.
Ordinary data-facing errors MUST NOT expose secret-derived digests or paths.
The trusted dedicated storage-root and host-access assumptions remain unchanged;
this adds neither encryption, secure erasure nor a secret vault.
A SQLite-only copy does not include referenced physical content. Snapshot export
still includes its exact payload closure; no whole-store backup facility is added.

Development-era stores are unsupported and are not upgraded. Supported baseline
reopening MUST preserve exact Snapshot identities, canonical bytes, payload
representations and service resources without a bulk rewrite. Read-only opening
MUST NOT publish new immutable files or infer missing evidence. Product updates
alone do not authorize rewriting retained data or bindings.

**Verification: PR-TEST-0483, PR-TEST-0484, PR-TEST-0485, PR-TEST-0479, PR-TEST-0212, PR-TEST-0301, PR-TEST-0480.**


### PR-REQ-0352 - Run evidence persistence and compatibility

New stores MUST use the supported [Persistence baseline](./persistence-baseline.md).
Development-era stores are refused without upgrade or inferred diagnostic
collections. Unsupported, foreign and drifted stores are not repaired. Reopening
supported storage preserves Run data and existing evidence exactly; absence of a
collection row means not-collected evidence, never permission to invent it.

New Run acceptance MUST record retention policy with the Run. Diagnostic writes
MUST revalidate writable admission and never recreate a missing collection. They
are separate from outcome/managed-output commit success and MUST NOT change those
boundaries. Evidence rows and state MUST be read in one snapshot; managed Run
inspection includes evidence in the same snapshot as Run facts and current guard.
Run deletion cascades to its evidence, without a new retention blocker or GC root.

Discriminators are exact: kind 0 is diagnostic, 1 completion, and 2 protocol-error.
Severity is null outside diagnostics; 0 is info, 1 warning and 2 error. Completion
status is null outside completion; 0 is success and 1 failure. Boolean flags use
0/1. A null message is absent and an empty string is present-empty. Truncated
messages concatenate retained prefix and suffix; truncated_prefix_bytes records
the UTF-8-safe split, and is zero for untruncated text. The stage description
contains Pactrun-owned operation/revision context and a Run-local Hook ordinal.
It is evidence for inspection, never an authority input.

A null receipt timestamp means the host wall clock was unavailable; it MUST NOT
be fabricated as the Unix epoch. Sequence remains the ordering authority.

Started means that collection initialization committed, not that a Hook launched.
Closed means that collection drained, not that the operation succeeded. An
uninitialized collection cannot prove that no Hook text was emitted.

Each batch MUST atomically update retained events, observed sequence and collection
status. A crash preserves committed evidence, not a claim of complete coverage.
A recording failure that cannot itself be persisted leaves an unclosed collection.
Missing events are not evidence that nothing was emitted. Disabled text retention
MUST NOT persist Hook code/message bytes, including through debug/error paths.
This is not an encrypted vault, backup facility or secure-erasure promise.

The complete baseline DDL above owns this representation; there is no historical SQL upgrade step.

**Verification: PR-TEST-0521, PR-TEST-0522, PR-TEST-0523, PR-TEST-0524, PR-TEST-0526, PR-TEST-0527.**

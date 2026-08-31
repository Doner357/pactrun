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

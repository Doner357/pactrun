CREATE TABLE runs (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    accepted_state_version BLOB NOT NULL CHECK(length(accepted_state_version) = 16),
    accepted_at_unix_ms INTEGER NOT NULL CHECK(accepted_at_unix_ms >= 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE run_action_invocations (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    package_id BLOB NOT NULL CHECK(length(package_id) = 16),
    revision_content_digest BLOB NOT NULL CHECK(length(revision_content_digest) = 32),
    action_identity BLOB NOT NULL CHECK(length(action_identity) > 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE run_executions (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    owner_session BLOB NOT NULL CHECK(length(owner_session) BETWEEN 1 AND 128),
    risk_state INTEGER NOT NULL CHECK(risk_state IN (0, 1)),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
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

CREATE TABLE run_outcomes (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    outcome_rank INTEGER NOT NULL CHECK(outcome_rank IN (0, 1, 2, 3, 4)),
    admitted_rank INTEGER NOT NULL CHECK(admitted_rank IN (0, 1)),
    risk_state INTEGER NOT NULL CHECK(risk_state IN (0, 1)),
    finished_at_unix_ms INTEGER NOT NULL CHECK(finished_at_unix_ms >= 0),
    PRIMARY KEY (run_id),
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
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

CREATE TABLE run_secondary_failures (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    error_owner BLOB NOT NULL CHECK(length(error_owner) BETWEEN 1 AND 128),
    error_code BLOB NOT NULL CHECK(length(error_code) BETWEEN 1 AND 128),
    message_utf8 BLOB NOT NULL,
    PRIMARY KEY (run_id, ordinal),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
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

CREATE TABLE run_artifacts (
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    output_identity BLOB NOT NULL CHECK(length(output_identity) > 0),
    byte_length INTEGER NOT NULL CHECK(byte_length BETWEEN 0 AND 536870912),
    PRIMARY KEY (run_id, output_identity),
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE CASCADE
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

CREATE TABLE instance_recovery_guards (
    instance_id BLOB NOT NULL CHECK(length(instance_id) = 16),
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    trigger_rank INTEGER NOT NULL CHECK(trigger_rank IN (0, 1, 2)),
    entered_at_unix_ms INTEGER NOT NULL CHECK(entered_at_unix_ms >= 0),
    PRIMARY KEY (instance_id),
    FOREIGN KEY (instance_id) REFERENCES instances(instance_id) ON DELETE CASCADE,
    FOREIGN KEY (run_id) REFERENCES run_outcomes(run_id) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

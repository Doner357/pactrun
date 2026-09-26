//! Functional SQL invariants retained across the one-time development baseline reset.
use super::PersistenceError;
use super::sqlite_revision_store::{BASELINE_SQL, configure_connection};
use crate::domain::*;
use rusqlite::{Connection, params};
use std::collections::BTreeSet;

fn baseline() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    configure_connection(&db).unwrap();
    db.execute_batch(BASELINE_SQL).unwrap();
    db
}

// Test-ID: PR-TEST-0183
// Verifies: PR-REQ-0077, PR-REQ-0298
#[test]
fn snapshot_schema_ownership_is_independent_and_capacity_is_not_a_sql_validity_rule() {
    let connection = baseline();
    // This intentionally tests SQL representation only, not logical manifest
    // validity or a complete payload. No Snapshot verifier is bypassed in production.
    let snapshot_id = [4_u8; 16];
    connection
        .execute(
            "INSERT INTO snapshots VALUES (?1, '1.0-alpha.1', ?2, ?3)",
            params![
                snapshot_id.as_slice(),
                [5_u8; 32].as_slice(),
                b"{}".as_slice()
            ],
        )
        .unwrap();
    let length_above_build_capability: i64 = 8 * 1024 * 1024 * 1024 + 1;
    connection
        .execute(
            "INSERT INTO snapshot_blobs(snapshot_id,blob_digest,byte_length) VALUES (?1, ?2, ?3)",
            params![
                snapshot_id.as_slice(),
                [6_u8; 32].as_slice(),
                length_above_build_capability
            ],
        )
        .unwrap();
    assert!(
        connection
            .execute(
                "INSERT INTO snapshot_blobs(snapshot_id,blob_digest,byte_length) VALUES (?1, ?2, -1)",
                params![snapshot_id.as_slice(), [7_u8; 32].as_slice()],
            )
            .is_err()
    );
    // No origin Instance, creator Run, or producer installation was inserted.
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM instances", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM runs", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM snapshots", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM sqlite_schema WHERE name='snapshot_blob_chunks'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        0,
        "the obsolete inline Snapshot representation must not exist"
    );
    connection.execute("DELETE FROM snapshots", []).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM snapshot_blobs", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

// Test-ID: PR-TEST-0182
// Verifies: PR-REQ-0298
#[test]
fn snapshot_and_admission_tables_are_strict_with_required_references() {
    let connection = baseline();
    let expected = BTreeSet::from([
        "writable_admissions",
        "run_operation_kinds",
        "run_capture_invocations",
        "run_restore_invocations",
        "instance_recovery_consequence_versions",
        "snapshots",
        "snapshot_blobs",
        "run_restore_admissions",
        "run_capture_results",
    ]);
    for table in expected {
        let flags: (i64, i64) = connection
            .query_row(
                "SELECT wr, strict FROM pragma_table_list WHERE schema='main' AND name=?1",
                [table],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(flags, (1, 1), "{table} must be WITHOUT ROWID and STRICT");
    }
    assert!(
        !connection
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_some()
    );
}

// Test-ID: PR-TEST-0299
// Verifies: PR-REQ-0311
#[test]
fn migration_references_retain_payloads_and_operation_kinds_are_not_inferred() {
    let mut db = baseline();
    let instance = InstanceId::from_bytes([2; 16]);
    let payload = ManagedInputPayloadId::from_bytes([3; 16]);
    let run = RunId::from_bytes([4; 16]);
    // SQL representation and reference-lifetime fixture only, not an executable Plan.
    db.execute("INSERT INTO packages VALUES (?1)", [[1u8; 16].as_slice()])
        .unwrap();
    db.execute(
        "INSERT INTO revisions VALUES (?1,?2,?3,?3)",
        params![[1u8; 16].as_slice(), [1u8; 32].as_slice(), b"{}".as_slice()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO instances VALUES (?1,?2,?3,?4,?5)",
        params![
            instance.as_bytes().as_slice(),
            b"fixture".as_slice(),
            [1u8; 16].as_slice(),
            [1u8; 32].as_slice(),
            [2u8; 16].as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO managed_input_payloads(instance_id,payload_id,protection_rank,byte_length) VALUES (?1,?2,1,1)",
        params![
            instance.as_bytes().as_slice(),
            payload.as_bytes().as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO managed_input_payload_chunks VALUES (?1,?2,0,?3)",
        params![
            instance.as_bytes().as_slice(),
            payload.as_bytes().as_slice(),
            b"x".as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO instance_history_identities VALUES (?1, x'66697874757265')",
        [instance.as_bytes().as_slice()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO runs VALUES (?1,?2,?3,0)",
        params![
            run.as_bytes().as_slice(),
            instance.as_bytes().as_slice(),
            [2u8; 16].as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO run_operation_kinds VALUES (?1,3)",
        [run.as_bytes().as_slice()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO run_migration_invocations VALUES (?1,?2,?3,?4,0)",
        params![
            run.as_bytes().as_slice(),
            [1u8; 16].as_slice(),
            [1u8; 32].as_slice(),
            [2u8; 32].as_slice()
        ],
    )
    .unwrap();
    assert!(matches!(
        super::writer_admission::validate_run_operation(&db, run),
        Ok(ManagedExecutionKind::Migration)
    ));
    db.execute(
        "INSERT INTO run_migration_progress VALUES (?1,0,0,?2,?3)",
        params![
            run.as_bytes().as_slice(),
            [1u8; 32].as_slice(),
            [2u8; 16].as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO run_migration_payload_pins VALUES (?1,?2,?3)",
        params![
            run.as_bytes().as_slice(),
            instance.as_bytes().as_slice(),
            payload.as_bytes().as_slice()
        ],
    )
    .unwrap();
    let tx = db.transaction().unwrap();
    assert!(
        !super::sqlite_instances::reclaim_payload_if_unreferenced(&tx, instance, payload).unwrap()
    );
    tx.execute(
        "INSERT INTO run_migration_checkpoint_bindings VALUES (?1,?2,?3,?4)",
        params![
            run.as_bytes().as_slice(),
            instance.as_bytes().as_slice(),
            b"config".as_slice(),
            payload.as_bytes().as_slice()
        ],
    )
    .unwrap();
    tx.execute("DELETE FROM run_migration_payload_pins", [])
        .unwrap();
    assert!(
        !super::sqlite_instances::reclaim_payload_if_unreferenced(&tx, instance, payload).unwrap()
    );
    tx.execute("DELETE FROM run_migration_checkpoint_bindings", [])
        .unwrap();
    assert!(
        super::sqlite_instances::reclaim_payload_if_unreferenced(&tx, instance, payload).unwrap()
    );
    tx.execute(
        "INSERT INTO run_action_invocations VALUES (?1,?2,?3,?4)",
        params![
            run.as_bytes().as_slice(),
            [1u8; 16].as_slice(),
            [1u8; 32].as_slice(),
            b"invalid-mixed-invocation".as_slice()
        ],
    )
    .unwrap();
    assert!(matches!(
        super::writer_admission::validate_run_operation(&tx, run),
        Err(PersistenceError::CorruptRun(_))
    ));
    tx.commit().unwrap();
}

const MIGRATION_TABLES: [&str; 7] = [
    "run_migration_checkpoint_bindings",
    "run_migration_payload_pins",
    "run_migration_revision_pins",
    "run_migration_boundaries",
    "run_migration_progress",
    "run_migration_edges",
    "run_migration_invocations",
];

// Test-ID: PR-TEST-0293
// Verifies: PR-REQ-0311
#[test]
fn migration_tables_preserve_shape_and_reject_unowned_invocations() {
    let db = baseline();
    for table in MIGRATION_TABLES {
        let flags: (i64, i64) = db
            .query_row(
                "SELECT wr,strict FROM pragma_table_list WHERE schema='main' AND name=?1",
                [table],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(flags, (1, 1));
    }
    assert!(
        db.execute(
            "INSERT INTO writable_admissions VALUES (?1,5)",
            [b"session-00000000000000000000000000000000".as_slice()]
        )
        .is_err()
    );
    assert!(
        db.execute(
            "INSERT INTO run_migration_invocations VALUES (?1,?2,?3,?3,0)",
            params![
                [1u8; 16].as_slice(),
                [2u8; 16].as_slice(),
                [3u8; 32].as_slice()
            ]
        )
        .is_err()
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name LIKE 'v6_%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

const TABLES: [&str; 10] = [
    "service_storage_allocations",
    "service_storage_preparations",
    "service_storage_protections",
    "service_storage_run_origins",
    "instance_service_storages",
    "instance_service_resources",
    "run_service_storage_pins",
    "run_service_storage_targets",
    "run_service_resource_targets",
    "run_service_edge_commits",
];

// Test-ID: PR-TEST-0340
// Verifies: PR-REQ-0323
#[test]
fn service_allocation_tables_protect_custody() {
    let db = baseline();
    for t in TABLES {
        assert_eq!(
            db.query_row(
                "SELECT wr,strict FROM pragma_table_list WHERE name=?1",
                [t],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            )
            .unwrap(),
            (1, 1)
        );
    }
    // SQL edge probes only; Domain ownership/contract validation is separate.
    db.execute(
        "INSERT INTO service_storage_allocations VALUES(?1,?2,?3,?4,?5)",
        params![
            [1u8; 16].as_slice(),
            [2u8; 16].as_slice(),
            [3u8; 16].as_slice(),
            [4u8; 32].as_slice(),
            b"state".as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO service_storage_protections VALUES(?1)",
        [[1u8; 16].as_slice()],
    )
    .unwrap();
    assert!(
        db.execute("DELETE FROM service_storage_allocations", [])
            .is_err()
    );
    assert!(
        db.execute(
            "INSERT INTO run_service_storage_pins VALUES(?1,?2)",
            params![[9u8; 16].as_slice(), [1u8; 16].as_slice()]
        )
        .is_err()
    );
}

// Test-ID: PR-TEST-0400
// Verifies: PR-REQ-0338
#[test]
fn lifecycle_allocation_tables_protect_custody() {
    let db = baseline();
    for t in TABLES {
        assert_eq!(
            db.query_row(
                "SELECT wr,strict FROM pragma_table_list WHERE name=?1",
                [t],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            )
            .unwrap(),
            (1, 1)
        );
    }
    // SQL edge probes only; Domain ownership/contract validation is separate.
    db.execute(
        "INSERT INTO service_storage_allocations VALUES(?1,?2,?3,?4,?5)",
        params![
            [1u8; 16].as_slice(),
            [2u8; 16].as_slice(),
            [3u8; 16].as_slice(),
            [4u8; 32].as_slice(),
            b"state".as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO service_storage_protections VALUES(?1)",
        [[1u8; 16].as_slice()],
    )
    .unwrap();
    assert!(
        db.execute("DELETE FROM service_storage_allocations", [])
            .is_err()
    );
    assert!(
        db.execute(
            "INSERT INTO run_service_storage_pins VALUES(?1,?2)",
            params![[9u8; 16].as_slice(), [1u8; 16].as_slice()]
        )
        .is_err()
    );
}

fn lifecycle_fixture() -> Connection {
    let connection = baseline();
    connection
        .execute("INSERT INTO packages VALUES(?1)", [[1u8; 16].as_slice()])
        .unwrap();
    connection
        .execute(
            "INSERT INTO revisions VALUES(?1, ?2, ?3, ?3)",
            params![[1u8; 16].as_slice(), [2u8; 32].as_slice(), b"{}".as_slice()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO instances VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                [3u8; 16].as_slice(),
                b"sample".as_slice(),
                [1u8; 16].as_slice(),
                [2u8; 32].as_slice(),
                [4u8; 16].as_slice()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO instance_history_identities VALUES(?1, x'73616d706c65')",
            [[3u8; 16].as_slice()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO runs VALUES(?1, ?2, ?3, 0)",
            params![
                [5u8; 16].as_slice(),
                [3u8; 16].as_slice(),
                [4u8; 16].as_slice()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO run_operation_kinds VALUES(?1, 0)",
            [[5u8; 16].as_slice()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO run_action_invocations VALUES(?1, ?2, ?3, ?4)",
            params![
                [5u8; 16].as_slice(),
                [1u8; 16].as_slice(),
                [2u8; 32].as_slice(),
                b"action".as_slice()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO run_outcomes VALUES(?1, 0, 1, 0, 1)",
            [[5u8; 16].as_slice()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO run_artifacts VALUES(?1, ?2, 0)",
            params![[5u8; 16].as_slice(), b"output".as_slice()],
        )
        .unwrap();
    connection
}

fn count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

// Test-ID: PR-TEST-0392
// Verifies: PR-REQ-0338
#[test]
fn history_preserves_run_children_and_artifacts_after_retirement() {
    let connection = lifecycle_fixture();
    for table in [
        "runs",
        "run_action_invocations",
        "run_outcomes",
        "run_artifacts",
        "instance_history_identities",
    ] {
        assert_eq!(count(&connection, table), 1, "{table}");
    }
    connection.execute("DELETE FROM instances", []).unwrap();
    assert_eq!(count(&connection, "runs"), 1);
    assert_eq!(count(&connection, "run_artifacts"), 1);
    assert!(
        connection
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none()
    );
    // New Instances can reuse a human name, not the old immutable identity.
    connection
        .execute(
            "INSERT INTO instances VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                [6u8; 16].as_slice(),
                b"sample".as_slice(),
                [1u8; 16].as_slice(),
                [2u8; 32].as_slice(),
                [7u8; 16].as_slice()
            ],
        )
        .unwrap();
    assert_eq!(count(&connection, "instance_history_identities"), 1);
    assert!(
        connection
            .execute("DELETE FROM instance_history_identities", [])
            .is_err()
    );
}

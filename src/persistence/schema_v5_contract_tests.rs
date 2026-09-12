//! Execute the approved S0 DDL without enabling a V5 production open path.
//! These tests do not prove migration, writer admission, or Snapshot runtime.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, params};

use super::sqlite_revision_store::{
    SCHEMA_V1_SQL, SCHEMA_V2_ADDITIONS_SQL, SCHEMA_V3_ADDITIONS_SQL, SCHEMA_V4_ADDITIONS_SQL,
};

const SPECIFICATION: &str =
    include_str!("../../docs/pactrun-developers/architecture/persistence-schema-v5.md");

fn approved_additions() -> &'static str {
    let (_, code) = SPECIFICATION.split_once("```sql\n").expect("one SQL block");
    let sql = code.split_once("\n```").expect("closed SQL block").0;
    assert_eq!(
        sql.trim(),
        super::sqlite_revision_store::SCHEMA_V5_ADDITIONS_SQL.trim()
    );
    sql
}

fn v4() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    for sql in [
        SCHEMA_V1_SQL,
        SCHEMA_V2_ADDITIONS_SQL,
        SCHEMA_V3_ADDITIONS_SQL,
        SCHEMA_V4_ADDITIONS_SQL,
    ] {
        connection.execute_batch(sql).unwrap();
    }
    connection
}

fn table_manifest(connection: &Connection) -> BTreeMap<String, String> {
    connection
        .prepare("SELECT name, sql FROM sqlite_schema WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

// Test-ID: PR-TEST-0182
// Verifies: PR-REQ-0298
#[test]
fn approved_v5_ddl_adds_ten_strict_tables_without_changing_v4_tables() {
    let connection = v4();
    let before = table_manifest(&connection);
    connection.execute_batch(approved_additions()).unwrap();
    let after = table_manifest(&connection);
    for (name, definition) in &before {
        assert_eq!(after.get(name), Some(definition), "changed V4 table {name}");
    }
    let added: BTreeSet<_> = after
        .keys()
        .filter(|name| !before.contains_key(*name))
        .map(String::as_str)
        .collect();
    let expected = BTreeSet::from([
        "writable_admissions",
        "run_operation_kinds",
        "run_capture_invocations",
        "run_restore_invocations",
        "instance_recovery_consequence_versions",
        "snapshots",
        "snapshot_blobs",
        "snapshot_blob_chunks",
        "run_restore_admissions",
        "run_capture_results",
    ]);
    assert_eq!(added, expected);
    for table in added {
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

// Test-ID: PR-TEST-0183
// Verifies: PR-REQ-0298
#[test]
fn snapshot_schema_ownership_is_independent_and_capacity_is_not_a_sql_validity_rule() {
    let connection = v4();
    connection.execute_batch(approved_additions()).unwrap();
    // This intentionally tests SQL representation only, not logical manifest
    // validity or a complete payload. No Snapshot verifier is bypassed in production.
    let snapshot_id = [4_u8; 16];
    connection
        .execute(
            "INSERT INTO snapshots VALUES (?1, 2, ?2, ?3)",
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
            "INSERT INTO snapshot_blobs VALUES (?1, ?2, ?3)",
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
                "INSERT INTO snapshot_blobs VALUES (?1, ?2, -1)",
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
    assert!(
        connection
            .execute(
                "INSERT INTO snapshot_blob_chunks VALUES (?1, ?2, 0, ?3)",
                params![
                    snapshot_id.as_slice(),
                    [6_u8; 32].as_slice(),
                    b"".as_slice()
                ],
            )
            .is_err()
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

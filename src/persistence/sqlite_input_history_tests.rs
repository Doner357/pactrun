use super::*;
// Test-ID: PR-TEST-0776
// Verifies: PR-REQ-0402, PR-REQ-0403
#[test]
fn input_history_repeated_context_is_idempotent_not_replacement_and_corruption_is_refused() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE runs(run_id BLOB PRIMARY KEY); CREATE TABLE run_operation_kinds(run_id BLOB PRIMARY KEY,operation_kind INTEGER);").unwrap();
    db.execute_batch(super::super::sqlite_revision_store::INPUT_HISTORY_SQL)
        .unwrap();
    let run = RunId::from_bytes([9; 16]);
    db.execute("INSERT INTO runs VALUES(?1)", [run.as_bytes().as_slice()])
        .unwrap();
    db.execute(
        "INSERT INTO run_operation_kinds VALUES(?1,0)",
        [run.as_bytes().as_slice()],
    )
    .unwrap();
    let context = HistoricalInputContext {
        edge_index: None,
        scope: InputHistoryScope::Current,
        coverage: InputHistoryCoverage::AdmittedBindings,
        snapshot: None,
        snapshot_integrity: None,
        inputs: vec![HistoricalInput {
            input: InputIdentity::parse("config").unwrap(),
            role: InputHistoryRole::Active,
            protection: ManagedInputProtection::Secret,
            origin: InputHistoryOrigin::Managed(ManagedInputPayloadId::from_bytes([1; 16])),
        }],
    };
    record(&db, run, context.clone()).unwrap();
    record(&db, run, context.clone()).unwrap();
    assert_eq!(load(&db, run).unwrap(), vec![context.clone()]);
    let mut changed = context.clone();
    changed.inputs[0].origin =
        InputHistoryOrigin::Managed(ManagedInputPayloadId::from_bytes([2; 16]));
    assert!(record(&db, run, changed).is_err());
    assert_eq!(load(&db, run).unwrap(), vec![context]);
    db.execute_batch(
        "PRAGMA ignore_check_constraints=ON; UPDATE run_input_references SET origin_rank=0;",
    )
    .unwrap();
    assert!(
        load(&db, run).is_err(),
        "contradictory absent payload was accepted"
    );
    db.execute_batch("PRAGMA foreign_keys=OFF; DELETE FROM run_input_contexts;")
        .unwrap();
    assert!(
        load(&db, run).is_err(),
        "orphan history was hidden as not recorded"
    );
}

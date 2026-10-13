use super::*;
use crate::domain::*;

// Test-ID: PR-TEST-0771
// Verifies: PR-REQ-0402, PR-REQ-0403
#[test]
fn input_history_remembers_exact_action_payload_without_retaining_it() {
    let f = RuntimeFixture::new();
    let p = persistence(&f.storage);
    let db = rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap();
    let before: i64 = db
        .query_row("SELECT count(*) FROM managed_input_payloads", [], |r| {
            r.get(0)
        })
        .unwrap();
    let admitted = f.admit("direct", "success", &f.marker("history"));
    let run = admitted.run();
    f.application.execute_admitted_action(
        admitted,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    assert!(f.application.advance_owner_continuation(run).unwrap());
    let history = p.input_history(run).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].scope, InputHistoryScope::Current);
    assert_eq!(history[0].coverage, InputHistoryCoverage::AdmittedBindings);
    let input = history[0]
        .inputs
        .iter()
        .find(|r| r.input.as_str() == "secret_config")
        .unwrap();
    assert_eq!(input.protection, ManagedInputProtection::Secret);
    let InputHistoryOrigin::Managed(payload) = input.origin else {
        panic!("no exact historical payload")
    };
    assert_eq!(
        db.query_row("SELECT count(*) FROM managed_input_payloads", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        before,
        "history copied payloads"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM run_payload_pins WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    f.replace_binding();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM managed_input_payloads WHERE payload_id=?1",
            [payload.as_bytes().as_slice()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0,
        "history pinned replaced bytes"
    );
    assert_eq!(p.input_history(run).unwrap(), history);
    assert!(!format!("{history:?}").contains(&payload.to_string()));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(
        crate::cli::run(
            vec![
                "--format".into(),
                "json".into(),
                "run".into(),
                "show".into(),
                run.to_string().into()
            ],
            Some(f.storage.clone().into_os_string()),
            &mut io::empty(),
            &mut out,
            &mut err
        ),
        0
    );
    let shown: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let metadata = &shown["result"]["input_history"];
    assert_eq!(metadata["recorded"], true);
    assert_eq!(
        metadata["contexts"][0]["inputs"][0]["origin"],
        "managed_reference"
    );
    let text = metadata.to_string();
    assert!(
        !text.contains(&payload.to_string())
            && !text.contains("payload_id")
            && !text.contains("digest")
            && !text.contains("byte_length")
    );
    assert_eq!(
        f.application
            .delete_object(&ObjectDeletion::Run {
                run,
                delete_artifacts: false
            })
            .unwrap(),
        ObjectDeletionResult::Deleted
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM run_input_contexts WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

// Test-ID: PR-TEST-0772
// Verifies: PR-REQ-0402, PR-REQ-0403
#[test]
fn input_history_absence_is_not_backfilled_and_schema_has_no_payload_root() {
    let f = RuntimeFixture::new();
    let p = persistence(&f.storage);
    let admitted = f.admit("direct", "success", &f.marker("old-history"));
    let run = admitted.run();
    f.application.execute_admitted_action(
        admitted,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    assert!(f.application.advance_owner_continuation(run).unwrap());
    let db = rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    db.execute(
        "DELETE FROM run_input_contexts WHERE run_id=?1",
        [run.as_bytes().as_slice()],
    )
    .unwrap();
    assert!(p.input_history(run).unwrap().is_empty());
    assert!(p.input_history(run).unwrap().is_empty());
    for (table, parent) in [
        ("run_input_contexts", "runs"),
        ("run_input_references", "run_input_contexts"),
    ] {
        let mut query = db
            .prepare(&format!("PRAGMA foreign_key_list({table})"))
            .unwrap();
        let targets = query
            .query_map([], |r| r.get::<_, String>(2))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(!targets.is_empty() && targets.iter().all(|target| target == parent));
    }
}

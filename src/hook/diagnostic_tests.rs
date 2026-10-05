use super::*;
use crate::domain::{DiagnosticKind, DiagnosticSeverity, HookEvidence, HookText, DiagnosticWindow};
fn text(kind: DiagnosticKind, message: &str) -> HookText {
    HookText { kind, severity: (kind==DiagnosticKind::Diagnostic).then_some(DiagnosticSeverity::Info), code:Some("evidence".into()), message:Some(message.into()), completion_status:(kind==DiagnosticKind::Completion).then_some(crate::domain::HookCompletionStatus::Failure),truncated:false, truncated_prefix_bytes:0 }
}
// Test-ID: PR-TEST-0521
// Verifies: PR-REQ-0352
#[test]
fn evidence_windows_commit_atomically_and_cannot_resurrect_deleted_runs() {
    let fixture=RuntimeFixture::new();
    let admitted=fixture.admit("direct","success",&fixture.marker("not-launched"));
    let run=admitted.run();
    let store=persistence(&fixture.storage);
    let mut window=DiagnosticWindow::default();
    for sequence in 1..=6000 { window.push(HookEvidence {sequence,received_at_unix_ms:Some(sequence),stage:"action hook_ordinal=1".into(),text:text(DiagnosticKind::Diagnostic,&sequence.to_string())}); }
    window.push(HookEvidence {sequence:6001,received_at_unix_ms:None,stage:"action hook_ordinal=1".into(),text:text(DiagnosticKind::Completion,"failure-tail")});
    store.save_diagnostics(run,&window,true,false,false).unwrap();
    let reopened=PactrunPersistence::open_read_only(&fixture.storage).unwrap();
    let view=reopened.inspect_diagnostics(run).unwrap().unwrap();
    assert!(!view.closed && view.started);
    assert_eq!(view.observed,6001);
    assert_eq!(view.events.first().unwrap().sequence,1);
    assert_eq!(view.events.last().unwrap().text.message.as_deref(),Some("failure-tail"));
    assert_eq!(view.events.len(),4097);
    assert_eq!(view.events.last().unwrap().received_at_unix_ms,None);
    store.save_diagnostics(run,&window,true,true,false).unwrap();
    assert!(reopened.inspect_diagnostics(run).unwrap().unwrap().closed);
    let db=rusqlite::Connection::open(fixture.storage.join("database/pactrun.sqlite3")).unwrap();
    db.execute("DELETE FROM run_diagnostic_events WHERE run_id=?1",[run.as_bytes().as_slice()]).unwrap();
    db.execute("DELETE FROM run_diagnostic_collections WHERE run_id=?1",[run.as_bytes().as_slice()]).unwrap();
    assert!(store.save_diagnostics(run,&window,true,true,false).is_err());
    assert!(reopened.inspect_diagnostics(run).unwrap().is_none());
    drop(admitted);
}

// Test-ID: PR-TEST-0528
// Verifies: PR-REQ-0351
#[test]
fn continuous_diagnostics_do_not_starve_deadlines_or_replay_the_hook() {
    let fixture=RuntimeFixture::new();
    let marker=fixture.marker("flood");
    let admitted=fixture.admit("direct","diagnostic_flood",&marker);
    let run=admitted.run();
    let cancellation=ActionCancellation::default();
    fixture.application.execute_admitted_action(admitted,policy(Some(10000),Some(1000),Some(50)),cancellation.clone());
    assert!(fixture.application.advance_owner_continuation(run).unwrap());
    cancellation.diagnostics.finish();
    let store=PactrunPersistence::open_read_only(&fixture.storage).unwrap();
    assert!(matches!(store.load_run(run).unwrap().unwrap().state,RunState::Finished(outcome) if outcome.outcome==RunOutcome::TimedOut));
    let evidence=store.inspect_diagnostics(run).unwrap().unwrap();
    assert!(evidence.observed>0);
    assert!(evidence.events.len()<=4352);
    assert_eq!(fs::read_to_string(marker_variant(&marker,"launches")).unwrap().lines().count(),1);
}

// Test-ID: PR-TEST-0526
// Verifies: PR-REQ-0351, PR-REQ-0352
#[test]
fn diagnostic_write_failure_does_not_replay_or_change_the_run_outcome() {
    let fixture=RuntimeFixture::new();
    let marker=fixture.marker("recording-failure");
    let admitted=fixture.admit("direct","success",&marker);
    let run=admitted.run();
    let cancellation=ActionCancellation::default();
    cancellation.diagnostics.fail_persistence.store(true,Ordering::Release);
    fixture.application.execute_admitted_action(admitted,policy(None,None,None),cancellation.clone());
    assert!(fixture.application.advance_owner_continuation(run).unwrap());
    cancellation.diagnostics.finish();
    let store=PactrunPersistence::open_read_only(&fixture.storage).unwrap();
    assert!(matches!(store.load_run(run).unwrap().unwrap().state,RunState::Finished(outcome) if outcome.outcome==RunOutcome::Succeeded));
    let evidence=store.inspect_diagnostics(run).unwrap().unwrap();
    assert!(evidence.failed && !evidence.closed);
    assert_eq!(fs::read_to_string(marker_variant(&marker,"launches")).unwrap().lines().count(),1);
}


// Test-ID: PR-TEST-0525
// Verifies: PR-REQ-0031, PR-REQ-0087, PR-REQ-0115, PR-REQ-0263
#[test]
fn instance_completeness_and_guard_are_independent_of_historical_run_state() {
    let fixture=RuntimeFixture::new();
    let admitted=fixture.admit("direct","risk_open_failure",&fixture.marker("guard"));
    let run=admitted.run();
    fixture.application.execute_admitted_action(admitted,policy(None,None,None),ActionCancellation::default());
    assert!(fixture.application.advance_owner_continuation(run).unwrap());
    let guarded=fixture.application.load_instance(fixture.instance.id).unwrap().unwrap();
    assert!(guarded.required_inputs_satisfied);
    assert_eq!(guarded.recovery_guard.as_ref().unwrap().run,run);
    let summary=fixture.application.list_instances().unwrap().remove(0);
    assert_eq!(summary.recovery_guard,guarded.recovery_guard);
    assert!(fixture.application.delete_input(guarded.id,&InputIdentity::parse("secret_config").unwrap(),guarded.state_version).is_err());
    // Seed a valid post-Migration projection; do not bypass required-Input
    // deletion policy. Migration publication itself has its own acceptance.
    let source = fixture.temporary.path().join("source");
    let yaml = fs::read_to_string(source.join("pactrun.yaml")).unwrap();
    let target_yaml = yaml.replace("  inputs:\n", "  inputs:\n    - {id: recovery_config, required: true, protection: normal}\n");
    assert_ne!(yaml,target_yaml);
    fs::write(source.join("pactrun.yaml"),target_yaml).unwrap();
    let target=fixture.application.install_pack_source(&source,&RevisionMetadataMutationBatch::new([]).unwrap()).unwrap().revision;
    let db=rusqlite::Connection::open(fixture.storage.join("database/pactrun.sqlite3")).unwrap();
    db.execute("UPDATE instances SET active_revision_content_digest=?2, instance_state_version=?3 WHERE instance_id=?1",rusqlite::params![guarded.id.as_bytes().as_slice(),target.content_digest.as_bytes().as_slice(),[119u8;16].as_slice()]).unwrap();
    let incomplete=fixture.application.load_instance(guarded.id).unwrap().unwrap();
    assert!(!incomplete.required_inputs_satisfied);
    assert_eq!(incomplete.recovery_guard,guarded.recovery_guard);
    let history=fixture.application.managed_run_inspection(run).unwrap().unwrap();
    assert_eq!(history.run.accepted_state_version,fixture.instance.state_version);
    assert_ne!(history.run.accepted_state_version,incomplete.state_version);
}

// Test-ID: PR-TEST-0522
// Verifies: PR-REQ-0283, PR-REQ-0352
#[test]
fn explicit_text_opt_out_leaves_no_durable_hook_explanations() {
    let fixture=RuntimeFixture::new();
    let marker=fixture.marker("opt-out");
    let admitted=fixture.admit("direct","finalization_text_marker",&marker);
    let run=admitted.run();
    let cancellation=ActionCancellation::default();
    cancellation.diagnostics.disabled.store(true,Ordering::Release);
    fixture.application.execute_admitted_action(admitted,policy(None,None,None),cancellation.clone());
    assert!(fixture.application.advance_owner_continuation(run).unwrap());
    cancellation.diagnostics.finish();
    let store=PactrunPersistence::open_read_only(&fixture.storage).unwrap();
    let view=store.inspect_diagnostics(run).unwrap().unwrap();
    assert!(!view.retain_text && view.closed && view.events.is_empty());
    for marker in ["hook_completion_marker","private_hook_completion_marker","private_hook_diagnostic_marker"] { assert!(!contains(&database_bytes(&fixture.storage),marker.as_bytes())); }
}

#[test]
fn diagnostic_crash_worker() {
    let Some(root)=std::env::var_os("PACTRUN_TEST_EVIDENCE_ROOT") else { return; };
    let run:RunId=std::env::var("PACTRUN_TEST_EVIDENCE_RUN").unwrap().parse().unwrap();
    let root=PathBuf::from(root);
    let manager=crate::hook::diagnostics::Diagnostics::default();
    let scope=manager.scope(root.clone(),run,"action hook_ordinal=1".into(),false);
    scope.record(text(DiagnosticKind::Diagnostic,"committed-before-owner-exit"));
    let reader=PactrunPersistence::open_read_only(&root).unwrap();
    let deadline=Instant::now()+Duration::from_secs(10);
    loop {
        if reader.inspect_diagnostics(run).unwrap().unwrap().events.len()==1 { std::process::exit(23); }
        assert!(Instant::now()<deadline);
        thread::sleep(Duration::from_millis(10));
    }
}
// Test-ID: PR-TEST-0523
// Verifies: PR-REQ-0352
#[test]
fn committed_evidence_survives_process_loss_with_unknown_tail() {
    let fixture=RuntimeFixture::new();
    let admitted=fixture.admit("direct","success",&fixture.marker("not-launched"));
    let run=admitted.run();
    let result=Command::new(std::env::current_exe().unwrap()).args(["--exact","hook::tests::diagnostic_runtime::diagnostic_crash_worker","--nocapture"])
        .env("PACTRUN_TEST_EVIDENCE_ROOT",&fixture.storage).env("PACTRUN_TEST_EVIDENCE_RUN",run.to_string()).output().unwrap();
    assert_eq!(result.status.code(),Some(23),"{}",String::from_utf8_lossy(&result.stderr));
    let reopened=PactrunPersistence::open_read_only(&fixture.storage).unwrap();
    let view=reopened.inspect_diagnostics(run).unwrap().unwrap();
    assert!(view.started && !view.closed);
    assert_eq!(view.events[0].text.message.as_deref(),Some("committed-before-owner-exit"));
    assert!(!fixture.marker("not-launched").exists());
    drop(admitted);
}

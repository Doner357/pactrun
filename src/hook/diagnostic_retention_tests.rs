use super::*;
use crate::{
    diagnostic_retention::{EvidenceUpdate, StoreWriter},
    domain::*,
    trace_journal::{self, Error, Journal, Location, RetentionGuard},
};

fn document(id: u8, run: RunId, policy: TracePolicy, full: bool) -> TraceDocument {
    let mut window = TraceWindow::new(
        TraceId::from_bytes([id; 16]),
        TraceOperation::ActionInvoke,
        Some(1),
        policy,
    );
    window.associate(run);
    for i in 0..if full { 1500 } else { 1 } {
        let span = window.begin(TracePhase::Command, i * 20).unwrap();
        window.end(
            span,
            TracePhase::Command,
            i * 20 + 10,
            10,
            PhaseResult::Succeeded,
        );
    }
    window.close(0);
    window.snapshot()
}

// Test-ID: PR-TEST-0796
// Verifies: PR-REQ-0398, PR-REQ-0399
#[test]
fn diagnostic_retention_lock_is_bounded_and_foreign_locks_are_not_repaired() {
    let f = RuntimeFixture::new();
    let guard = RetentionGuard::acquire(&f.storage).unwrap();
    assert!(matches!(
        RetentionGuard::acquire(&f.storage),
        Err(Error::Busy)
    ));
    drop(guard);
    drop(RetentionGuard::acquire(&f.storage).unwrap());
    let path = f.storage.join("database/diagnostic-retention.lock");
    fs::write(&path, b"foreign-private-content").unwrap();
    assert!(matches!(
        RetentionGuard::acquire(&f.storage),
        Err(Error::ForeignJournal)
    ));
    assert_eq!(fs::read(path).unwrap(), b"foreign-private-content");
    let missing = f.temporary.path().join("missing-store");
    assert!(StoreWriter::open(&missing).is_err());
    assert!(!missing.exists());
}

// Test-ID: PR-TEST-0797
// Verifies: PR-REQ-0394, PR-REQ-0398
#[test]
fn store_diagnostic_budget_and_age_cover_hook_helper_and_phase_evidence_together() {
    let f = RuntimeFixture::new();
    let admitted = f.admit("direct", "success", &f.marker("not-launched"));
    let run = admitted.run();
    let p = persistence(&f.storage);
    let before = p.load_managed_run(run).unwrap();
    let instance = p.load_instance_by_id(f.instance.id).unwrap();
    let policy = TracePolicy {
        max_bytes: 1_048_576,
        retention_days: 1,
        detailed: true,
    };
    let mut writer = StoreWriter::open(&f.storage).unwrap();
    let mut hook = DiagnosticWindow::default();
    let text = "x".repeat(16 * 1024);
    for sequence in 1..=60 {
        hook.push(HookEvidence {
            sequence,
            received_at_unix_ms: Some(100),
            stage: "action".into(),
            text: HookText {
                kind: DiagnosticKind::Diagnostic,
                severity: Some(DiagnosticSeverity::Info),
                completion_status: None,
                code: None,
                message: Some(text.clone()),
                truncated: false,
                truncated_prefix_bytes: 0,
            },
        });
    }
    let failure = HelperFailure {
        command: HelperCommand::Diagnostic,
        stage: HelperStage::ReadJson,
        reason: HelperReason::NotFound,
    };
    let mut core = CoreDiagnosticWindow::default();
    for _ in 0..2 {
        core.push(CoreEvidence {
            sequence: 0,
            received_at_unix_ms: Some(100),
            stage: "action".into(),
            failure,
        });
    }
    writer
        .save_evidence(
            EvidenceUpdate {
                run,
                hook: &hook,
                core: Some(&core),
                retain_text: true,
                closed: true,
                failed: false,
            },
            policy,
            1000,
        )
        .unwrap();
    let original = p.diagnostic_retention_inventory(None, None).unwrap().bytes;
    assert!(original < policy.max_bytes);
    let trace = document(81, run, policy, true);
    assert!(serde_json::to_vec(&trace).unwrap().len() < policy.max_bytes as usize);
    assert!(original + serde_json::to_vec(&trace).unwrap().len() as u64 > policy.max_bytes);
    writer.save_trace(&trace, 1001).unwrap();
    let location = Location::store(&f.storage);
    assert_eq!(
        trace_journal::read(&location, trace.id)
            .unwrap()
            .unwrap()
            .retention,
        TraceRetention::Complete
    );
    let journal = Journal::open(&location).unwrap();
    let retained = p.diagnostic_retention_inventory(None, None).unwrap().bytes;
    assert!(
        retained < original,
        "per-class quotas did not enforce the combined budget"
    );
    assert!(retained + journal.retention_inventory(None, None).unwrap().0 <= policy.max_bytes);
    for sequence in 61..=70 {
        hook.push(HookEvidence {
            sequence,
            received_at_unix_ms: Some(2000),
            stage: "action".into(),
            text: HookText {
                kind: DiagnosticKind::Diagnostic,
                severity: Some(DiagnosticSeverity::Info),
                completion_status: None,
                code: None,
                message: Some(text.clone()),
                truncated: false,
                truncated_prefix_bytes: 0,
            },
        });
    }
    core.push(CoreEvidence {
        sequence: 0,
        received_at_unix_ms: Some(2000),
        stage: "action".into(),
        failure,
    });
    writer
        .save_evidence(
            EvidenceUpdate {
                run,
                hook: &hook,
                core: Some(&core),
                retain_text: true,
                closed: true,
                failed: false,
            },
            policy,
            2001,
        )
        .unwrap();
    assert!(
        trace_journal::read(&location, trace.id).unwrap().is_none(),
        "the current table-backed update did not prefer itself over old phase evidence"
    );
    assert!(
        p.diagnostic_retention_inventory(None, None).unwrap().bytes
            + journal.retention_inventory(None, None).unwrap().0
            <= policy.max_bytes
    );
    let next = document(82, run, policy, false);
    writer.save_trace(&next, 2 * 86_400_000).unwrap();
    assert_eq!(
        p.diagnostic_retention_inventory(None, None).unwrap().bytes,
        0
    );
    let evidence = p.inspect_diagnostics(run).unwrap().unwrap();
    assert_eq!(evidence.observed, 70);
    assert!(evidence.events.is_empty() && evidence.closed && evidence.started);
    let core = evidence.core.unwrap();
    assert_eq!(core.observed, 3);
    assert!(core.events.is_empty() && core.collection_closed);
    assert_eq!(p.load_managed_run(run).unwrap(), before);
    assert_eq!(p.load_instance_by_id(f.instance.id).unwrap(), instance);
    let mut input = Vec::new();
    p.export_input(
        f.instance.id,
        &InputIdentity::parse("secret_config").unwrap(),
        true,
        &mut input,
    )
    .unwrap();
    assert_eq!(input, SECRET_BINDING);
    // Reclamation is not a rewrite of the Run's owned history.
    assert!(!p.input_history(run).unwrap().is_empty());
    let saved = trace_journal::read(&location, next.id).unwrap().unwrap();
    assert_eq!(saved.retention, TraceRetention::Complete);
    assert!(journal.retention_inventory(None, None).unwrap().0 <= policy.max_bytes);
}

// Test-ID: PR-TEST-0799
// Verifies: PR-REQ-0398
#[test]
fn concurrent_processes_share_one_store_diagnostic_budget_without_run_mutation() {
    use std::process::Stdio;
    let f = RuntimeFixture::new();
    let admitted = f.admit("direct", "success", &f.marker("concurrent"));
    let run = admitted.run();
    let p = persistence(&f.storage);
    let before = p.load_managed_run(run).unwrap();
    let policy = TracePolicy {
        max_bytes: 1_048_576,
        retention_days: 1,
        detailed: true,
    };
    let mut writer = StoreWriter::open(&f.storage).unwrap();
    let mut hook = DiagnosticWindow::default();
    for sequence in 1..=40 {
        hook.push(HookEvidence {
            sequence,
            received_at_unix_ms: Some(1),
            stage: "action".into(),
            text: HookText {
                kind: DiagnosticKind::Diagnostic,
                severity: Some(DiagnosticSeverity::Info),
                completion_status: None,
                code: None,
                message: Some("x".repeat(8192)),
                truncated: false,
                truncated_prefix_bytes: 0,
            },
        });
    }
    writer
        .save_evidence(
            EvidenceUpdate {
                run,
                hook: &hook,
                core: None,
                retain_text: true,
                closed: true,
                failed: false,
            },
            policy,
            2,
        )
        .unwrap();
    let gate = f.marker("retention-go");
    let children: Vec<_> = (0..2)
        .map(|index| {
            Command::new(env::current_exe().unwrap())
                .args([
                    "--exact",
                    "hook::tests::retention_runtime::retention_writer_worker",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("PACTRUN_RETENTION_STORE", &f.storage)
                .env("PACTRUN_RETENTION_RUN", run.to_string())
                .env("PACTRUN_RETENTION_GATE", &gate)
                .env("PACTRUN_RETENTION_WORKER", index.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for index in 0..2 {
        wait_for(&gate.with_extension(format!("ready-{index}")));
    }
    fs::write(&gate, b"go").unwrap();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}; {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let journal = Journal::open(&Location::store(&f.storage)).unwrap();
    let (trace_bytes, candidates) = journal.retention_inventory(None, None).unwrap();
    assert!(trace_bytes > 0 && !candidates.is_empty());
    assert!(
        trace_bytes + p.diagnostic_retention_inventory(None, None).unwrap().bytes
            <= policy.max_bytes
    );
    assert_eq!(p.load_managed_run(run).unwrap(), before);
    assert!(!p.input_history(run).unwrap().is_empty());
}

#[test]
fn retention_writer_worker() {
    let Some(root) = env::var_os("PACTRUN_RETENTION_STORE") else {
        return;
    };
    let root = PathBuf::from(root);
    let run: RunId = env::var("PACTRUN_RETENTION_RUN").unwrap().parse().unwrap();
    let worker: u8 = env::var("PACTRUN_RETENTION_WORKER")
        .unwrap()
        .parse()
        .unwrap();
    let gate = PathBuf::from(env::var_os("PACTRUN_RETENTION_GATE").unwrap());
    fs::write(gate.with_extension(format!("ready-{worker}")), b"ready").unwrap();
    wait_for(&gate);
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut writer = loop {
        match StoreWriter::open(&root) {
            Ok(writer) => break writer,
            Err(Error::Busy) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(5))
            }
            Err(error) => panic!("retention worker could not open: {error}"),
        }
    };
    let policy = TracePolicy {
        max_bytes: 1_048_576,
        retention_days: 1,
        detailed: true,
    };
    for index in 0..6 {
        let trace = document(100 + worker * 8 + index, run, policy, true);
        loop {
            match writer.save_trace(&trace, 10_000 + u64::from(index)) {
                Ok(()) | Err(Error::Busy) => {}
                Err(error) => panic!("retention worker could not save: {error}"),
            }
            if let Ok(Some(stored)) = trace_journal::read(&Location::store(&root), trace.id)
                && stored.retention == TraceRetention::Complete
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "combined retention did not converge"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
}

// Test-ID: PR-TEST-0800
// Verifies: PR-REQ-0398, PR-REQ-0399
#[test]
fn retention_unavailability_keeps_phase_evidence_without_bootstrap_or_schema_upgrade() {
    let f = RuntimeFixture::new();
    let policy = TracePolicy::default();
    for old_store in [false, true] {
        let root = f.temporary.path().join(if old_store {
            "older-store"
        } else {
            "missing-database"
        });
        fs::create_dir_all(root.join("database")).unwrap();
        fs::create_dir(root.join("runtime-content")).unwrap();
        let path = root.join("database/pactrun.sqlite3");
        let before = if old_store {
            let db = rusqlite::Connection::open(&path).unwrap();
            db.execute_batch(include_str!("../persistence/persistence_alpha5.sql"))
                .unwrap();
            db.pragma_update(None, "application_id", 0x5041_4354_i64)
                .unwrap();
            db.pragma_update(None, "user_version", 4).unwrap();
            drop(db);
            Some(fs::read(&path).unwrap())
        } else {
            None
        };
        assert!(PactrunPersistence::open_for_diagnostics(&root).is_err());
        let mut writer = StoreWriter::open(&root).unwrap();
        let trace = document(91, RunId::from_bytes([91; 16]), policy, false);
        writer.save_trace(&trace, 10).unwrap();
        let saved = trace_journal::read(&Location::store(&root), trace.id)
            .unwrap()
            .unwrap();
        assert_eq!(saved.retention, TraceRetention::Unavailable);
        assert!(saved.closed);
        assert_eq!(saved.events, trace.events);
        if let Some(before) = before {
            assert_eq!(fs::read(path).unwrap(), before);
        } else {
            assert!(!path.exists());
        }
        assert!(!root.join("staging").exists());
        assert!(
            fs::read_dir(root.join("runtime-content"))
                .unwrap()
                .next()
                .is_none()
        );
    }
}

// Test-ID: PR-TEST-0801
// Verifies: PR-REQ-0398
#[test]
fn busy_primary_database_does_not_erase_trace_and_later_maintenance_can_complete() {
    let f = RuntimeFixture::new();
    let admitted = f.admit("direct", "success", &f.marker("busy"));
    let run = admitted.run();
    let p = persistence(&f.storage);
    let before = p.load_managed_run(run).unwrap();
    let mut blocker =
        rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap();
    let tx = blocker
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let mut writer = StoreWriter::open(&f.storage).unwrap();
    let trace = document(92, run, TracePolicy::default(), false);
    writer.save_trace(&trace, 100).unwrap();
    let saved = trace_journal::read(&Location::store(&f.storage), trace.id)
        .unwrap()
        .unwrap();
    assert_eq!(saved.retention, TraceRetention::Incomplete);
    assert_eq!(saved.events, trace.events);
    assert_eq!(p.load_managed_run(run).unwrap(), before);
    drop(tx);
    writer.save_trace(&trace, 101).unwrap();
    let saved = trace_journal::read(&Location::store(&f.storage), trace.id)
        .unwrap()
        .unwrap();
    assert_eq!(saved.retention, TraceRetention::Complete);
    assert_eq!(p.load_managed_run(run).unwrap(), before);
}

// Test-ID: PR-TEST-0802
// Verifies: PR-REQ-0352, PR-REQ-0398
#[test]
fn unavailable_phase_file_does_not_stop_hook_recording_or_change_the_outcome() {
    let f = RuntimeFixture::new();
    let path = f.storage.join("database/diagnostics.sqlite3");
    fs::write(&path, b"foreign-phase-file").unwrap();
    let admitted = f.admit(
        "direct",
        "sensitive_diagnostic",
        &f.marker("unavailable-phase"),
    );
    let run = admitted.run();
    let cancellation = ActionCancellation::default();
    f.application
        .execute_admitted_action(admitted, policy(None, None, None), cancellation.clone());
    assert!(f.application.advance_owner_continuation(run).unwrap());
    cancellation.diagnostics.finish();
    let p = persistence(&f.storage);
    assert!(
        matches!(p.load_managed_run(run).unwrap().unwrap().state, RunState::Finished(ref outcome) if outcome.outcome == RunOutcome::Succeeded)
    );
    let evidence = p.inspect_diagnostics(run).unwrap().unwrap();
    assert!(evidence.started && evidence.closed && !evidence.failed);
    assert!(evidence.events.iter().any(|e| {
        e.text
            .message
            .as_ref()
            .is_some_and(|message| contains(message.as_bytes(), SECRET_BINDING))
    }));
    assert_eq!(fs::read(path).unwrap(), b"foreign-phase-file");
    assert!(
        p.diagnostic_retention_inventory(None, None).unwrap().bytes
            <= TracePolicy::default().max_bytes
    );
}

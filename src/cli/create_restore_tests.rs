// Included in cli::tests to reuse the isolated CLI storage fixture.
fn create_restore_fixture() -> (
    TempDir,
    PathBuf,
    RevisionIdentity,
    crate::domain::SnapshotId,
) {
    use crate::domain::{InstanceId, SnapshotId, SnapshotIntegrityVersion};
    let (temp, root, source) = cli_roots();
    let worker = if cfg!(windows) {
        "worker.exe"
    } else {
        "worker"
    };
    fs::copy(env::current_exe().unwrap(), source.join(worker)).unwrap();
    fs::write(
        source.join("pactrun.yaml"),
        r#"source_format: 1.0-alpha.2
package_id: 00000000000000000000000000000094
revision:
  inputs: []
  actions: []
  snapshot:
    restore:
      parameters: []
      hook:
        protocol_version: 1.0-alpha.1
        launch: { kind: direct, executable: worker }
        args: ["--exact", "hook::tests::restore_runtime::restore_hook_worker", "--nocapture", "--test-threads=1"]
        io: { terminal: none }
  migrations: []
runtime_content:
  files:
    - { id: worker, source: worker, path: bin/worker, executable: true }
"#.replace("source: worker, path: bin/worker", &format!("source: {worker}, path: bin/{worker}")),
    )
    .unwrap();
    let app = PactrunApplication::open(&root).unwrap();
    let revision = app
        .install_pack_source(&source, &RevisionMetadataMutationBatch::new([]).unwrap())
        .unwrap()
        .revision;
    let snapshot = SnapshotId::generate().unwrap();
    let raw = serde_json::json!({"format_version":"1.0-alpha.1","snapshot_id":snapshot.to_string(),
        "producer":{"package_id":revision.package_id.to_string(),"revision_content_digest":revision.content_digest.to_string()},
        "origin_instance_id":InstanceId::generate().unwrap().to_string(),
        "captured_at":{"unix_seconds":0,"nanoseconds":0},"managed_bindings":[],"service_content":[]});
    let manifest = crate::snapshot_integrity::decode_snapshot_manifest(
        SnapshotIntegrityVersion::BASELINE,
        &serde_json::to_vec(&raw).unwrap(),
        None,
    )
    .unwrap();
    let p = crate::persistence::PactrunPersistence::open(&root).unwrap();
    let mut stage = p
        .staging_session()
        .unwrap()
        .create_snapshot_stage()
        .unwrap();
    crate::snapshot_bundle::start_bundle(&mut stage, &manifest)
        .unwrap()
        .finish()
        .unwrap();
    stage.finish_operation_file().unwrap();
    let mut bundle = crate::snapshot_bundle::ValidatedSnapshotBundle::read(stage).unwrap();
    p.import_snapshot_bundle(&mut bundle).unwrap();
    (temp, root, revision, snapshot)
}

// Test-ID: PR-TEST-0676
// Verifies: PR-REQ-0346, PR-REQ-0375
#[test]
fn create_restore_continues_after_human_output_closes_and_preserves_success() {
    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }
    for close_stderr in [false, true] {
        let (_temp, root, revision, snapshot) = create_restore_fixture();
        let mut closed = Closed;
        let mut captured = Vec::new();
        let (stdout, stderr): (&mut dyn Write, &mut (dyn Write + Send)) = if close_stderr {
            (&mut captured, &mut closed)
        } else {
            (&mut closed, &mut captured)
        };
        let args = [
            "instance".to_owned(),
            "create".into(),
            "created".into(),
            "--revision".into(),
            format_revision(&revision),
            "--restore-from".into(),
            snapshot.to_string(),
            "--startup-timeout-ms".into(),
            "10000".into(),
            "--execution-timeout-ms".into(),
            "10000".into(),
        ];
        assert_eq!(
            run(
                args.into_iter().map(OsString::from).collect(),
                Some(root.as_os_str().into()),
                &mut io::empty(),
                stdout,
                stderr
            ),
            1
        );
        let app = PactrunApplication::open_read_only(&root).unwrap();
        let instance = app
            .resolve_instance_name(&InstanceName::parse("created").unwrap())
            .unwrap()
            .unwrap();
        let runs = app.list_managed_runs(instance).unwrap();
        assert_eq!(
            runs.len(),
            1,
            "output failure must not skip or replay Restore"
        );
        assert!(
            matches!(&runs[0].state, RunState::Finished(outcome) if outcome.outcome==RunOutcome::Succeeded),
            "{:?}",
            runs[0].state
        );
        assert_eq!(
            app.load_instance(instance)
                .unwrap()
                .unwrap()
                .active_revision,
            revision
        );
    }
}

// Test-ID: PR-TEST-0478
// Verifies: PR-REQ-0346
#[test]
fn create_restore_revalidates_after_create_and_never_resolves_a_replacement_name() {
    for race in ["replace", "delete_snapshot", "cancel"] {
        let (_temp, root, revision, snapshot) = create_restore_fixture();
        let app = PactrunApplication::open(&root).unwrap();
        let name = InstanceName::parse("created").unwrap();
        let cancellation = ActionCancellation::default();
        let original = std::sync::Arc::new(std::sync::Mutex::new(None));
        let seen = original.clone();
        let path = root.clone();
        let target = revision.clone();
        let selected = name.clone();
        let cancel = cancellation.clone();
        let _observer = snapshots::observe_creation(move |id| {
            *seen.lock().unwrap() = Some(id);
            let app = PactrunApplication::open(&path).unwrap();
            match race {
                "replace" => {
                    let mut out = Vec::new();
                    let mut err = Vec::new();
                    assert_eq!(
                        run(
                            ["instance", "abandon", "created"]
                                .map(OsString::from)
                                .to_vec(),
                            Some(path.as_os_str().into()),
                            &mut io::empty(),
                            &mut out,
                            &mut err
                        ),
                        0,
                        "{err:?}"
                    );
                    let replacement = app.create_instance(selected, target, vec![]).unwrap();
                    assert_ne!(replacement.id, id);
                }
                "delete_snapshot" => {
                    app.delete_object(&crate::domain::ObjectDeletion::Snapshot(snapshot))
                        .unwrap();
                }
                "cancel" => cancel.request(),
                _ => unreachable!(),
            }
        });
        let capture =
            reply::Capture::new("instance create", false, reply::DisplayOptions::default());
        let error = snapshots::create_and_restore(
            snapshots::CreateRestoreCommand {
                name: name.clone(),
                revision: RevisionReference::Exact(revision),
                snapshot: snapshot.into(),
                options: ExecutionOptions::default(),
            },
            &root,
            &mut io::empty(),
            &mut io::sink(),
            &cancellation,
            reply::OutputContext(&capture),
        )
        .unwrap_err();
        assert!(!error.usage);
        let original = original.lock().unwrap().unwrap();
        let partial = serde_json::to_value(error.partial.as_ref().unwrap()).unwrap();
        assert_eq!(
            partial["created_instance"]["instance_id"],
            original.to_string()
        );
        assert!(partial["restore_run_id"].is_null());
        let current = app.resolve_instance_name(&name).unwrap().unwrap();
        if race == "replace" {
            assert_ne!(current, original);
        } else {
            assert_eq!(current, original);
        }
        assert!(app.list_managed_runs(current).unwrap().is_empty());
    }
}

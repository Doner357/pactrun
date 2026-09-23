// Included in cli::tests to reuse the isolated CLI storage fixture.
fn create_restore_fixture() -> (
    TempDir,
    PathBuf,
    RevisionIdentity,
    crate::domain::SnapshotId,
) {
    use crate::domain::{InstanceId, SnapshotId, SnapshotIntegrityVersion};
    let (temp, root, source) = cli_roots();
    fs::copy(env::current_exe().unwrap(), source.join("worker")).unwrap();
    fs::write(
        source.join("pactrun.yaml"),
        r#"source_format: 1
package_id: 00000000000000000000000000000094
revision:
  inputs: []
  actions: []
  snapshot:
    restore:
      parameters: []
      hook:
        protocol_version: 1
        launch: { kind: direct, executable: worker }
        args: []
        io: { terminal: none }
  migrations: []
runtime_content:
  files:
    - { id: worker, source: worker, path: bin/worker, executable: true }
"#,
    )
    .unwrap();
    let app = PactrunApplication::open(&root).unwrap();
    let revision = app
        .install_pack_source(&source, &RevisionMetadataMutationBatch::new([]).unwrap())
        .unwrap()
        .revision;
    let snapshot = SnapshotId::generate().unwrap();
    let raw = serde_json::json!({"format_version":2,"snapshot_id":snapshot.to_string(),
        "producer":{"package_id":revision.package_id.to_string(),"revision_content_digest":revision.content_digest.to_string()},
        "origin_instance_id":InstanceId::generate().unwrap().to_string(),
        "captured_at":{"unix_seconds":0,"nanoseconds":0},"managed_bindings":[],"service_content":[]});
    let manifest = crate::snapshot_integrity::decode_snapshot_manifest(
        SnapshotIntegrityVersion::V2,
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

struct CreateRestoreInterpose<F: FnOnce()> {
    action: Option<F>,
    fail: bool,
}
impl<F: FnOnce()> Write for CreateRestoreInterpose<F> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(action) = self.action.take() {
            action();
        }
        if self.fail {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// Test-ID: PR-TEST-0478
// Verifies: PR-REQ-0346
#[test]
fn create_restore_revalidates_after_create_and_never_resolves_a_replacement_name() {
    for race in ["replace", "delete_snapshot", "cancel", "output_failure"] {
        let (_temp, root, revision, snapshot) = create_restore_fixture();
        let app = PactrunApplication::open(&root).unwrap();
        let name = InstanceName::parse("created").unwrap();
        let cancellation = ActionCancellation::default();
        let mut original = None;
        let mut writer = CreateRestoreInterpose {
            action: Some(|| {
                original = app.resolve_instance_name(&name).unwrap();
                assert!(original.is_some());
                match race {
                    "replace" => {
                        let mut out = Vec::new();
                        let mut err = Vec::new();
                        assert_eq!(
                            run(
                                vec!["instance".into(), "abandon".into(), "created".into()],
                                Some(root.as_os_str().to_owned()),
                                &mut io::empty(),
                                &mut out,
                                &mut err
                            ),
                            0,
                            "{err:?}"
                        );
                        let replacement = app
                            .create_instance(name.clone(), revision.clone(), Vec::new())
                            .unwrap();
                        assert_ne!(Some(replacement.id), original);
                    }
                    "delete_snapshot" => {
                        app.delete_object(&crate::domain::ObjectDeletion::Snapshot(snapshot))
                            .unwrap();
                    }
                    "cancel" => cancellation.request(),
                    "output_failure" => (),
                    _ => unreachable!(),
                }
            }),
            fail: race == "output_failure",
        };
        let mut stderr = Vec::new();
        let result = snapshots::create_and_restore(
            snapshots::CreateRestoreCommand {
                name: name.clone(),
                revision: RevisionReference::Exact(revision.clone()),
                snapshot: snapshot.into(),
                options: ExecutionOptions::default(),
            },
            &root,
            &mut io::empty(),
            &mut writer,
            &mut stderr,
            &cancellation,
            presentation::Format::Human,
        );
        let error = result.unwrap_err();
        assert!(!error.usage);
        assert!(error.message.contains(&original.unwrap().to_string()));
        assert!(
            String::from_utf8(stderr)
                .unwrap()
                .contains("partial_completion:")
        );
        let current = app.resolve_instance_name(&name).unwrap().unwrap();
        if race == "replace" {
            assert_ne!(Some(current), original);
        } else {
            assert_eq!(Some(current), original);
        }
        assert!(app.list_managed_runs(current).unwrap().is_empty());
    }
}

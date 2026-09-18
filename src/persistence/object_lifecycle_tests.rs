use super::*;
#[path = "object_lifecycle_extra_tests.rs"]
mod extra;
use crate::domain::*;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> (tempfile::TempDir, PathBuf) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/object-lifecycle-tests");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let root = temp.path().join("store");
    for child in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(child)).unwrap();
    }
    (temp, root)
}

fn revision(p: &PactrunPersistence, package: u8, bytes: &[u8]) -> (RevisionIdentity, Sha256Digest) {
    let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
    let blob = p.put_runtime_content(&digest, &mut &bytes[..]).unwrap();
    let core = crate::revision_core_v1::project_revision_core_source_v1(br#"{
        "format_version":1,"inputs":[],"actions":[{"id":"inspect","access":"observe","parameters":[],"hook":{"protocol_version":1,"launch":{"kind":"direct","executable":"tool"},"args":[],"io":{"terminal":"none"}},"outputs":[]}],"migrations":[]
    }"#).unwrap();
    let runtime = project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
        files: vec![RuntimeFileV1 {
            id: ContentId::parse("tool").unwrap(),
            path: RuntimePath::parse("bin/tool").unwrap(),
            kind: RuntimeFileKindV1::RegularFile,
            blob_digest: digest.clone(),
            executable: true,
        }],
    })
    .unwrap();
    let content = validate_revision_content_v1(core, runtime).unwrap();
    (
        p.persist_revision(PackageId::from_bytes([package; 16]), &content, &[blob])
            .unwrap(),
        digest,
    )
}

fn instance(p: &PactrunPersistence, revision: &RevisionIdentity) -> InstanceView {
    p.create_instance(
        InstanceName::parse("sample").unwrap(),
        revision.clone(),
        &mut [],
    )
    .unwrap()
}

fn run(p: &PactrunPersistence, instance: &InstanceView) -> RunId {
    let id = RunId::generate().unwrap();
    p.create_accepted_run(
        id,
        instance.id,
        instance.state_version,
        &ActionRunIdentity {
            revision: instance.active_revision.clone(),
            action: ActionIdentity::parse("inspect").unwrap(),
        },
        &p.staging_session().unwrap().owner(),
        &UnconditionalAcceptance,
    )
    .unwrap();
    let content = p
        .load_revision(&instance.active_revision)
        .unwrap()
        .unwrap()
        .content;
    let launch = CompiledHookLaunch::Direct {
        executable: content.runtime_content.files()[0].clone(),
    };
    p.admit_run(
        id,
        &AdmissionFacts {
            service: None,
            expected_state_version: instance.state_version,
            active_bindings: &[],
            runtime_content: content.runtime_content.files(),
            launch: &launch,
        },
        &|_| Ok(()),
        false,
    )
    .unwrap()
    .unwrap();
    id
}

fn finish(p: &PactrunPersistence, id: RunId, outcome: RunOutcome) {
    p.finish_run(
        id,
        &RunFinish {
            outcome,
            primary_failure: None,
            secondary_failures: vec![],
            hook_completion: None,
        },
        &mut [RunArtifactWrite {
            output: ManagedOutputIdentity::parse("report").unwrap(),
            byte_len: 7,
            reader: &mut &b"payload"[..],
        }],
    )
    .unwrap();
}

fn snapshot(p: &PactrunPersistence, producer: &RevisionIdentity, origin: InstanceId) -> SnapshotId {
    let id = SnapshotId::generate().unwrap();
    let digest = Sha256Digest::from_bytes(Sha256::digest(b"payload").into());
    let raw = serde_json::to_vec(&serde_json::json!({"format_version":1,"snapshot_id":id.to_string(),"producer":{"package_id":producer.package_id.to_string(),"revision_content_digest":producer.content_digest.to_string()},"origin_instance_id":origin.to_string(),"captured_at":{"unix_seconds":0,"nanoseconds":0},"managed_bindings":[],"service_content":[{"role":"state","path":"state","blob_digest":digest.as_str()}]})).unwrap();
    let manifest = crate::snapshot_integrity::decode_snapshot_manifest(
        SnapshotIntegrityVersion::V1,
        &raw,
        None,
    )
    .unwrap();
    let mut stage = p
        .staging_session()
        .unwrap()
        .create_snapshot_stage()
        .unwrap();
    {
        let mut writer = crate::snapshot_bundle::start_bundle(&mut stage, &manifest).unwrap();
        writer.start_blob(&digest, 7).unwrap();
        writer.write_all(b"payload").unwrap();
        writer.finish().unwrap();
    }
    stage.finish_operation_file().unwrap();
    let mut bundle = crate::snapshot_bundle::ValidatedSnapshotBundle::read(stage).unwrap();
    p.import_snapshot_bundle(&mut bundle).unwrap();
    id
}

fn cli(root: &Path, args: &[&str]) -> (i32, String, String) {
    let (mut out, mut err) = (vec![], vec![]);
    let code = crate::cli::run(
        args.iter().map(std::ffi::OsString::from).collect(),
        Some(root.as_os_str().to_owned()),
        &mut io::empty(),
        &mut out,
        &mut err,
    );
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn worker(root: &Path, args: &[String]) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "persistence::object_lifecycle_tests::lifecycle_process_worker",
            "--nocapture",
        ])
        .env("PACTRUN_LIFECYCLE_ROOT", root)
        .env(
            "PACTRUN_LIFECYCLE_ARGS",
            serde_json::to_string(args).unwrap(),
        );
    command
}

#[test]
fn lifecycle_process_worker() {
    let Some(root) = std::env::var_os("PACTRUN_LIFECYCLE_ROOT") else {
        return;
    };
    let args: Vec<String> =
        serde_json::from_str(&std::env::var("PACTRUN_LIFECYCLE_ARGS").unwrap()).unwrap();
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    let result = cli(Path::new(&root), &refs);
    if result.0 != 0 {
        eprintln!("{}", result.2);
        std::process::exit(result.0);
    }
}

fn wait(path: &Path) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !path.exists() {
        assert!(std::time::Instant::now() < end, "worker barrier timed out");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

// Test-ID: PR-TEST-0462
// Verifies: PR-REQ-0341
#[test]
fn object_deletion_crashes_leave_atomic_retryable_commit_boundaries() {
    for kind in ["revision", "snapshot", "run"] {
        for (point, committed) in [
            ("before_object_deletion_commit", false),
            ("after_object_deletion_commit", true),
        ] {
            let (_temp, root) = root();
            let p = PactrunPersistence::open(&root).unwrap();
            let (rev, _) = revision(&p, 20, b"tool");
            let target = match kind {
                "revision" => ObjectDeletion::Revision(rev.clone()),
                "snapshot" => {
                    ObjectDeletion::Snapshot(snapshot(&p, &rev, InstanceId::from_bytes([3; 16])))
                }
                "run" => {
                    let i = instance(&p, &rev);
                    let r = run(&p, &i);
                    finish(&p, r, RunOutcome::Succeeded);
                    ObjectDeletion::Run {
                        run: r,
                        delete_artifacts: true,
                    }
                }
                _ => unreachable!(),
            };
            let mut args = vec![kind.to_owned(), "delete".to_owned()];
            args.push(match &target {
                ObjectDeletion::Revision(r) => {
                    format!("exact:{}/{}", r.package_id, r.content_digest)
                }
                ObjectDeletion::Snapshot(s) => s.to_string(),
                ObjectDeletion::Run { run, .. } => run.to_string(),
            });
            if kind == "run" {
                args.push("--delete-artifacts".to_owned());
            }
            drop(p);
            assert_eq!(
                worker(&root, &args)
                    .env("PACTRUN_LIFECYCLE_FAULT", point)
                    .output()
                    .unwrap()
                    .status
                    .code(),
                Some(87)
            );
            let p = PactrunPersistence::open(&root).unwrap();
            assert_eq!(
                p.delete_object(&target).unwrap(),
                if committed {
                    ObjectDeletionResult::AlreadyAbsent
                } else {
                    ObjectDeletionResult::Deleted
                }
            );
            assert_eq!(
                p.delete_object(&target).unwrap(),
                ObjectDeletionResult::AlreadyAbsent
            );
        }
    }
}

// Test-ID: PR-TEST-0463
// Verifies: PR-REQ-0076, PR-REQ-0343
#[test]
fn collection_process_loss_retries_from_current_references() {
    let mut points = vec!["before_collection_removal", "after_collection_removal"];
    if cfg!(target_os = "linux") {
        points.push("after_collection_claim");
    }
    for point in points {
        let (_temp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let (rev, _) = revision(&p, 21, b"orphan");
        p.delete_object(&ObjectDeletion::Revision(rev)).unwrap();
        drop(p);
        let args = vec!["storage".to_owned(), "gc".to_owned()];
        let output = worker(&root, &args)
            .env("PACTRUN_LIFECYCLE_FAULT", point)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(87),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let gc = PactrunPersistence::open_for_collection(&root).unwrap();
        let result = gc.collect_content(true).unwrap();
        assert_eq!(result.failed, 0);
        assert_eq!(
            result.removed,
            if point == "after_collection_removal" {
                0
            } else {
                1
            }
        );
        assert_eq!(gc.collect_content(true).unwrap().removed, 0);
    }
}

// Test-ID: PR-TEST-0464
// Verifies: PR-REQ-0076, PR-REQ-0343
#[test]
fn collection_retains_foreign_links_corruption_and_invalid_reference_graphs() {
    let (temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (revision, digest) = revision(&p, 22, b"linked");
    let path = root
        .join("runtime-content")
        .join(digest.as_str().strip_prefix("sha256:").unwrap());
    let outside = temp.path().join("outside");
    fs::hard_link(&path, &outside).unwrap();
    p.delete_object(&ObjectDeletion::Revision(revision))
        .unwrap();
    drop(p);
    {
        let gc = PactrunPersistence::open_for_collection(&root).unwrap();
        assert_eq!(gc.collect_content(true).unwrap().failed, 1);
        assert_eq!(fs::read(&outside).unwrap(), b"linked");
        assert_eq!(fs::read(&path).unwrap(), b"linked");
    }
    fs::remove_file(&outside).unwrap();
    fs::write(&path, b"corruption").unwrap();
    {
        let gc = PactrunPersistence::open_for_collection(&root).unwrap();
        assert_eq!(gc.collect_content(true).unwrap().failed, 1);
        assert_eq!(fs::read(&path).unwrap(), b"corruption");
    }
    // A broken strong edge prevents every removal, not just the affected digest.
    let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    db.pragma_update(None, "foreign_keys", false).unwrap();
    db.execute(
        "INSERT INTO run_revision_pins VALUES(?1,?2,?3)",
        params![
            [1u8; 16].as_slice(),
            [2u8; 16].as_slice(),
            [3u8; 32].as_slice()
        ],
    )
    .unwrap();
    drop(db);
    assert!(
        PactrunPersistence::open_for_collection(&root)
            .unwrap()
            .collect_content(true)
            .is_err()
    );
    assert!(path.exists());
}

// Test-ID: PR-TEST-0465
// Verifies: PR-REQ-0075, PR-REQ-0118, PR-REQ-0341, PR-REQ-0344
#[test]
fn revision_cli_resolves_exact_intent_and_removes_installation_metadata() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (first, _) = revision(&p, 23, b"first");
    let (second, _) = revision(&p, 24, b"second");
    let label = ReferenceLabel::parse("release").unwrap();
    let alias = LocalAlias::parse("chosen").unwrap();
    for rev in [&first, &second] {
        p.apply_revision_metadata_batch(
            rev,
            &RevisionMetadataMutationBatch::new([RevisionMetadataMutation::AddReferenceLabel(
                ReferenceLabelBinding {
                    label: label.clone(),
                    revision: rev.clone(),
                    source: ReferenceLabelSource::Unattributed,
                },
            )])
            .unwrap(),
        )
        .unwrap();
    }
    p.apply_revision_metadata_batch(
        &first,
        &RevisionMetadataMutationBatch::new([RevisionMetadataMutation::CompareAndSetLocalAlias {
            alias: alias.clone(),
            expected: CurrentState::Absent,
            desired: CurrentState::Present(first.clone()),
        }])
        .unwrap(),
    )
    .unwrap();
    assert_eq!(cli(&root, &["revision", "delete", "label:release"]).0, 1);
    assert_eq!(cli(&root, &["revision", "delete", "alias:chosen"]).0, 0);
    assert!(p.lookup_local_alias(&alias).unwrap().is_none());
    assert_eq!(p.lookup_reference_label(&label).unwrap(), vec![second]);
    assert_eq!(
        cli(
            &root,
            &[
                "revision",
                "delete",
                &format!("exact:{}/{}", first.package_id, first.content_digest)
            ]
        )
        .0,
        0
    );
    assert_eq!(cli(&root, &["revision", "delete", "alias:chosen"]).0, 1);
    let absent = root.join("absent");
    for args in [
        vec!["storage", "gc", "--plan", "--plan"],
        vec![
            "run",
            "delete",
            "00000000000000000000000000000001",
            "--force",
        ],
        vec![
            "snapshot",
            "delete",
            "00000000000000000000000000000001",
            "--plan",
        ],
    ] {
        assert_eq!(cli(&absent, &args).0, 2);
    }
    assert!(!absent.exists());
}

// Test-ID: PR-TEST-0466
// Verifies: PR-REQ-0341, PR-REQ-0342
#[test]
fn snapshot_export_keeps_its_consistent_read_across_another_process_deletion() {
    let (temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (rev, _) = revision(&p, 25, b"tool");
    let snap = snapshot(&p, &rev, InstanceId::from_bytes([1; 16]));
    let barrier = temp.path().join("barrier");
    fs::create_dir(&barrier).unwrap();
    let output = temp.path().join("snapshot.bundle");
    let args = vec![
        "snapshot".to_owned(),
        "export".to_owned(),
        snap.to_string(),
        "--output".to_owned(),
        output.to_str().unwrap().to_owned(),
        "--authorize-sensitive-export".to_owned(),
    ];
    let mut child = worker(&root, &args)
        .env("PACTRUN_LIFECYCLE_SYNC", "after_snapshot_read_established")
        .env("PACTRUN_LIFECYCLE_SYNC_DIR", &barrier)
        .spawn()
        .unwrap();
    wait(&barrier.join("ready"));
    assert_eq!(
        p.delete_object(&ObjectDeletion::Snapshot(snap)).unwrap(),
        ObjectDeletionResult::Deleted
    );
    assert!(p.inspect_snapshot(snap).is_err());
    fs::write(barrier.join("release"), b"").unwrap();
    assert!(child.wait().unwrap().success());
    let mut stage = p
        .staging_session()
        .unwrap()
        .create_snapshot_stage()
        .unwrap();
    io::copy(&mut fs::File::open(output).unwrap(), stage.writer()).unwrap();
    stage.finish_operation_file().unwrap();
    let mut bundle = crate::snapshot_bundle::ValidatedSnapshotBundle::read(stage).unwrap();
    assert_eq!(bundle.manifest().manifest().snapshot_id(), snap);
    p.import_snapshot_bundle(&mut bundle).unwrap();
    p.verify_snapshot(snap).unwrap();
}

// Test-ID: PR-TEST-0467
// Verifies: PR-REQ-0076, PR-REQ-0343
#[test]
fn collection_namespace_replacement_cannot_delete_an_unrelated_file() {
    let (temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (rev, digest) = revision(&p, 26, b"original");
    p.delete_object(&ObjectDeletion::Revision(rev)).unwrap();
    drop(p);
    let path = root
        .join("runtime-content")
        .join(digest.as_str().strip_prefix("sha256:").unwrap());
    let saved = temp.path().join("saved");
    let barrier = temp.path().join("barrier");
    fs::create_dir(&barrier).unwrap();
    let mut child = worker(&root, &["storage".to_owned(), "gc".to_owned()])
        .env("PACTRUN_LIFECYCLE_SYNC", "before_collection_removal")
        .env("PACTRUN_LIFECYCLE_SYNC_DIR", &barrier)
        .spawn()
        .unwrap();
    wait(&barrier.join("ready"));
    #[cfg(windows)]
    assert!(fs::rename(&path, &saved).is_err());
    #[cfg(target_os = "linux")]
    {
        fs::rename(&path, &saved).unwrap();
        fs::write(&path, b"unrelated replacement").unwrap();
    }
    fs::write(barrier.join("release"), b"").unwrap();
    let status = child.wait().unwrap();
    #[cfg(windows)]
    {
        assert!(status.success());
        assert!(!path.exists());
    }
    #[cfg(target_os = "linux")]
    {
        assert_eq!(status.code(), Some(1));
        assert_eq!(fs::read(path).unwrap(), b"unrelated replacement");
        assert_eq!(fs::read(saved).unwrap(), b"original");
    }
}

// Test-ID: PR-TEST-0468
// Verifies: PR-REQ-0345
#[test]
fn coordination_upgrade_fences_live_writers_and_crash_boundaries() {
    use super::sqlite_revision_store::*;
    for point in [
        "before_schema_migration_commit",
        "after_schema_migration_commit",
    ] {
        let (_temp, root) = root();
        let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
        for (sql, _) in &SCHEMA_LADDER[..8] {
            db.execute_batch(sql).unwrap();
        }
        db.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        db.pragma_update(None, "user_version", 8).unwrap();
        let session = crate::managed_data::StagingSession::prepare(&root).unwrap();
        db.execute(
            "INSERT INTO writable_admissions VALUES(?1,8)",
            [session.owner().as_str().as_bytes()],
        )
        .unwrap();
        assert!(matches!(
            PactrunPersistence::upgrade_storage(&root),
            Err(PersistenceError::ActiveWriters)
        ));
        drop(session);
        drop(db);
        let child = worker(&root, &["storage".to_owned(), "upgrade".to_owned()])
            .env("PACTRUN_LIFECYCLE_FAULT", point)
            .output()
            .unwrap();
        assert_eq!(child.status.code(), Some(87));
        let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
        let v = db
            .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(
            v,
            if point.starts_with("before") {
                8
            } else {
                super::SCHEMA_VERSION
            }
        );
        validate_schema(&db, v).unwrap();
        drop(db);
        assert_eq!(PactrunPersistence::upgrade_storage(&root).unwrap(), v == 8);
        let p = PactrunPersistence::open(&root).unwrap();
        let (rev, _) = revision(&p, 27, b"retained");
        p.database
            .lock()
            .unwrap()
            .execute("DELETE FROM writable_admissions", [])
            .unwrap();
        assert!(matches!(
            p.delete_object(&ObjectDeletion::Revision(rev.clone())),
            Err(PersistenceError::WriterAdmissionRequired)
        ));
        assert!(p.load_revision(&rev).unwrap().is_some());
    }
}

// Test-ID: PR-TEST-0456
// Verifies: PR-REQ-0345
#[test]
fn coordination_upgrade_accepts_only_exact_predecessor_and_preserves_data() {
    use super::sqlite_revision_store::*;
    let document =
        include_str!("../../docs/spec/persistence/persistence-schema-v9.md").replace("\r\n", "\n");
    assert_eq!(
        document
            .split("```sql\n")
            .nth(1)
            .unwrap()
            .split("```")
            .next()
            .unwrap()
            .trim(),
        include_str!("persistence_schema_v9_additions.sql").trim()
    );
    for version in 1..=SCHEMA_VERSION as usize {
        let (_temp, root) = root();
        let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
        for (sql, _) in &SCHEMA_LADDER[..version] {
            db.execute_batch(sql).unwrap();
        }
        db.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        db.pragma_update(None, "user_version", version as i64)
            .unwrap();
        db.execute("INSERT INTO packages VALUES(?1)", [[9u8; 16].as_slice()])
            .unwrap();
        if version == 8 {
            assert_eq!(cli(&root, &["storage", "gc", "--plan"]).0, 1);
            assert!(!root.join("runtime-content/.collection.lock").exists());
            assert_eq!(
                db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                    .unwrap(),
                8
            );
        }
        let result = PactrunPersistence::upgrade_storage(&root);
        match version {
            8 | 9 => assert!(result.unwrap()),
            10 => assert!(!result.unwrap()),
            _ => assert!(result.is_err()),
        }
        assert_eq!(
            db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            if matches!(version, 8 | 9) {
                SCHEMA_VERSION
            } else {
                version as i64
            }
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM packages", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        if version == 8 {
            assert!(PactrunPersistence::open_read_only(&root).is_ok());
        }
    }
}

// Test-ID: PR-TEST-0457
// Verifies: PR-REQ-0343, PR-REQ-0345
#[test]
fn coordination_covers_publication_witnesses_and_unpinned_readers() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let digest = Sha256Digest::from_bytes(Sha256::digest(b"uncommitted").into());
    let witness = p
        .put_runtime_content(&digest, &mut &b"uncommitted"[..])
        .unwrap();
    let reader = p.runtime_content.open_verified(&digest).unwrap();
    drop(p);
    assert!(PactrunPersistence::open_for_collection(&root).is_err());
    drop(witness);
    assert!(PactrunPersistence::open_for_collection(&root).is_err());
    drop(reader);
    let gc = PactrunPersistence::open_for_collection(&root).unwrap();
    assert!(PactrunPersistence::open(&root).is_err());
    assert_eq!(gc.collect_content(true).unwrap().removed, 1);
}

// Test-ID: PR-TEST-0458
// Verifies: PR-REQ-0341, PR-REQ-0344
#[test]
fn run_deletion_requires_explicit_artifact_intent_and_preserves_recovery_evidence() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (rev, _) = revision(&p, 1, b"tool");
    let instance = instance(&p, &rev);
    let id = run(&p, &instance);
    assert_eq!(
        p.delete_object(&ObjectDeletion::Run {
            run: id,
            delete_artifacts: true
        })
        .unwrap(),
        ObjectDeletionResult::Blocked(DeletionBlock::RunningRun)
    );
    finish(&p, id, RunOutcome::Succeeded);
    assert_eq!(
        p.delete_object(&ObjectDeletion::Run {
            run: id,
            delete_artifacts: false
        })
        .unwrap(),
        ObjectDeletionResult::Blocked(DeletionBlock::ArtifactsRemain)
    );
    let mut bytes = vec![];
    p.open_run_artifact(
        id,
        &ManagedOutputIdentity::parse("report").unwrap(),
        &mut bytes,
    )
    .unwrap();
    assert_eq!(bytes, b"payload");
    let snap = snapshot(&p, &rev, instance.id);
    assert_eq!(
        p.delete_object(&ObjectDeletion::Run {
            run: id,
            delete_artifacts: true
        })
        .unwrap(),
        ObjectDeletionResult::Deleted
    );
    assert_eq!(
        p.delete_object(&ObjectDeletion::Run {
            run: id,
            delete_artifacts: true
        })
        .unwrap(),
        ObjectDeletionResult::AlreadyAbsent
    );
    p.verify_snapshot(snap).unwrap();
    let id = run(&p, &instance);
    p.open_recovery_risk(id).unwrap();
    finish(&p, id, RunOutcome::Failed);
    assert_eq!(
        p.delete_object(&ObjectDeletion::Run {
            run: id,
            delete_artifacts: true
        })
        .unwrap(),
        ObjectDeletionResult::Blocked(DeletionBlock::RecoveryEvidence)
    );
    let state = p
        .load_instance_by_id(instance.id)
        .unwrap()
        .unwrap()
        .state_version;
    p.resolve_manual_recovery(instance.id, state).unwrap();
    assert_eq!(
        p.delete_object(&ObjectDeletion::Run {
            run: id,
            delete_artifacts: true
        })
        .unwrap(),
        ObjectDeletionResult::Deleted
    );
}

// Test-ID: PR-TEST-0459
// Verifies: PR-REQ-0075, PR-REQ-0341, PR-REQ-0344
#[test]
fn revision_removal_respects_active_use_but_not_historical_provenance() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (rev, digest) = revision(&p, 2, b"tool");
    let instance = instance(&p, &rev);
    let snap = snapshot(&p, &rev, instance.id);
    let id = run(&p, &instance);
    finish(&p, id, RunOutcome::Succeeded);
    assert_eq!(
        p.delete_object(&ObjectDeletion::Revision(rev.clone()))
            .unwrap(),
        ObjectDeletionResult::Blocked(DeletionBlock::ActiveInstance)
    );
    let result = cli(&root, &["instance", "delete", "sample"]);
    assert_eq!(result.0, 0, "{}", result.2);
    assert_eq!(
        p.delete_object(&ObjectDeletion::Revision(rev.clone()))
            .unwrap(),
        ObjectDeletionResult::Deleted
    );
    assert_eq!(
        p.delete_object(&ObjectDeletion::Revision(rev)).unwrap(),
        ObjectDeletionResult::AlreadyAbsent
    );
    assert!(p.load_run(id).unwrap().is_some());
    p.verify_snapshot(snap).unwrap();
    assert!(
        root.join("runtime-content")
            .join(digest.as_str().strip_prefix("sha256:").unwrap())
            .exists()
    );
    let db = p.database.lock().unwrap();
    let retired: Vec<u8> = db
        .query_row(
            "SELECT retirement_run_id FROM instance_retirement_receipts",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(db);
    assert_eq!(
        p.delete_object(&ObjectDeletion::Run {
            run: RunId::from_bytes(retired.try_into().unwrap()),
            delete_artifacts: true
        })
        .unwrap(),
        ObjectDeletionResult::Blocked(DeletionBlock::RetirementEvidence)
    );
}

// Test-ID: PR-TEST-0460
// Verifies: PR-REQ-0117, PR-REQ-0341, PR-REQ-0344
#[test]
fn snapshot_removal_is_atomic_and_rejects_an_accepted_restore() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (rev, _) = revision(&p, 3, b"tool");
    let instance = instance(&p, &rev);
    let snap = snapshot(&p, &rev, instance.id);
    let restore = RunId::generate().unwrap();
    p.create_accepted_managed_run(
        restore,
        instance.id,
        instance.state_version,
        &ManagedRunIdentity::Restore {
            revision: rev,
            snapshot: snap,
        },
        &p.staging_session().unwrap().owner(),
        &UnconditionalAcceptance,
    )
    .unwrap();
    assert_eq!(
        p.delete_object(&ObjectDeletion::Snapshot(snap)).unwrap(),
        ObjectDeletionResult::Blocked(DeletionBlock::SnapshotInUse)
    );
    p.finish_run(
        restore,
        &RunFinish {
            outcome: RunOutcome::Cancelled,
            primary_failure: None,
            secondary_failures: vec![],
            hook_completion: None,
        },
        &mut [],
    )
    .unwrap();
    let removed = cli(&root, &["snapshot", "delete", &snap.to_string()]);
    assert_eq!(removed.0, 0, "{}", removed.2);
    assert_eq!(
        p.delete_object(&ObjectDeletion::Snapshot(snap)).unwrap(),
        ObjectDeletionResult::AlreadyAbsent
    );
    let db = p.database.lock().unwrap();
    for table in ["snapshot_blobs", "snapshot_blob_chunks"] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert!(super::sqlite_runs::load_managed_run_from(&db, restore).is_ok());
}

// Test-ID: PR-TEST-0461
// Verifies: PR-REQ-0076, PR-REQ-0343, PR-REQ-0344
#[test]
fn collection_reclaims_only_verified_unreferenced_content_and_preview_is_read_only() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (first, shared) = revision(&p, 4, b"shared");
    let (second, _) = revision(&p, 5, b"shared");
    let (third, orphan) = revision(&p, 6, b"orphan");
    p.delete_object(&ObjectDeletion::Revision(first)).unwrap();
    p.delete_object(&ObjectDeletion::Revision(third)).unwrap();
    fs::write(root.join("runtime-content/foreign"), b"not garbage").unwrap();
    fs::create_dir(root.join("service-storage")).unwrap();
    fs::write(root.join("service-storage/service-owned"), b"protected").unwrap();
    let sessions = fs::read_dir(root.join("staging")).unwrap().count();
    let admissions = p
        .database
        .lock()
        .unwrap()
        .query_row("SELECT count(*) FROM writable_admissions", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap();
    let preview = cli(&root, &["storage", "gc", "--plan"]);
    assert_eq!(
        fs::read_dir(root.join("staging")).unwrap().count(),
        sessions
    );
    assert_eq!(
        p.database
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM writable_admissions", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        admissions
    );
    assert_eq!(
        p.database
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(preview.0, 0, "{}", preview.2);
    assert!(preview.1.contains("candidates=1"));
    assert_eq!(cli(&root, &["storage", "gc", "--dry-run"]).0, 2);
    assert!(PactrunPersistence::open_for_collection(&root).is_err());
    drop(p);
    let gc = PactrunPersistence::open_for_collection(&root).unwrap();
    let report = gc.collect_content(true).unwrap();
    assert_eq!(
        (
            report.candidates,
            report.removed,
            report.failed,
            report.retained,
            report.unsupported
        ),
        (1, 1, 0, 1, 1)
    );
    assert_eq!(gc.collect_content(true).unwrap().removed, 0);
    assert!(
        root.join("runtime-content")
            .join(shared.as_str().strip_prefix("sha256:").unwrap())
            .exists()
    );
    assert!(
        !root
            .join("runtime-content")
            .join(orphan.as_str().strip_prefix("sha256:").unwrap())
            .exists()
    );
    assert_eq!(
        fs::read(root.join("runtime-content/foreign")).unwrap(),
        b"not garbage"
    );
    assert_eq!(
        fs::read(root.join("service-storage/service-owned")).unwrap(),
        b"protected"
    );
    drop(gc);
    let p = PactrunPersistence::open(&root).unwrap();
    p.delete_object(&ObjectDeletion::Revision(second)).unwrap();
    drop(p);
    assert_eq!(cli(&root, &["storage", "gc"]).0, 0);
    assert!(
        !root
            .join("runtime-content")
            .join(shared.as_str().strip_prefix("sha256:").unwrap())
            .exists()
    );
}

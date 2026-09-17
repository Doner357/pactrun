use super::*;
use crate::{
    domain::*,
    persistence::{PactrunPersistence, RunArtifactWrite, UnconditionalAcceptance},
};
use sha2::{Digest, Sha256};

fn fixture(bytes: &[u8]) -> (tempfile::TempDir, PathBuf, RunId) {
    fixture_stream(&mut &bytes[..], bytes.len() as u64)
}

fn fixture_stream(reader: &mut dyn Read, byte_len: u64) -> (tempfile::TempDir, PathBuf, RunId) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/artifact-cli-tests");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let root = temp.path().join("storage");
    for child in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(child)).unwrap();
    }
    let p = PactrunPersistence::open(&root).unwrap();
    let tool = b"fixture-tool";
    let digest = Sha256Digest::from_bytes(Sha256::digest(tool).into());
    let publication = p.put_runtime_content(&digest, &mut &tool[..]).unwrap();
    let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
        inputs: vec![],
        actions: vec![ActionV1 {
            id: ActionIdentity::parse("inspect").unwrap(),
            access: OperationAccessV1::Observe,
            parameters: vec![],
            hook: HookV1 {
                protocol_version: PositiveVersion::new(1).unwrap(),
                launch: HookLaunchV1::Direct {
                    executable: ContentId::parse("tool").unwrap(),
                },
                args: vec![],
                io: IOContractV1 {
                    terminal: TerminalContractV1::None,
                },
            },
            outputs: vec![],
        }],
        snapshot: None,
        migrations: vec![],
        cleanup: None,
    })
    .unwrap();
    let runtime = project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
        files: vec![RuntimeFileV1 {
            id: ContentId::parse("tool").unwrap(),
            path: RuntimePath::parse("bin/tool").unwrap(),
            kind: RuntimeFileKindV1::RegularFile,
            blob_digest: digest,
            executable: true,
        }],
    })
    .unwrap();
    let content = validate_revision_content_v1(core, runtime).unwrap();
    let revision = p
        .persist_revision(PackageId::from_bytes([81; 16]), &content, &[publication])
        .unwrap();
    let instance = p
        .create_instance(
            InstanceName::parse("artifact-owner").unwrap(),
            revision.clone(),
            &mut [],
        )
        .unwrap();
    let run = RunId::generate().unwrap();
    p.create_accepted_run(
        run,
        instance.id,
        instance.state_version,
        &ActionRunIdentity {
            revision,
            action: ActionIdentity::parse("inspect").unwrap(),
        },
        &p.staging_session().unwrap().owner(),
        &UnconditionalAcceptance,
    )
    .unwrap();
    let launch = CompiledHookLaunch::Direct {
        executable: content.runtime_content.files()[0].clone(),
    };
    p.admit_run(
        run,
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
    let mut empty = &b""[..];
    p.finish_run(
        run,
        &RunFinish {
            outcome: RunOutcome::Succeeded,
            primary_failure: None,
            secondary_failures: vec![],
            hook_completion: None,
        },
        &mut [
            RunArtifactWrite {
                output: ManagedOutputIdentity::parse("report").unwrap(),
                byte_len,
                reader,
            },
            RunArtifactWrite {
                output: ManagedOutputIdentity::parse("empty").unwrap(),
                byte_len: 0,
                reader: &mut empty,
            },
        ],
    )
    .unwrap();
    (temp, root, run)
}

fn invoke(root: &Path, args: &[&str]) -> (i32, String, String) {
    let mut out = vec![];
    let mut err = vec![];
    let code = crate::cli::run(
        args.iter().map(OsString::from).collect(),
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

// Test-ID: PR-TEST-0449
// Verifies: PR-REQ-0342, PR-REQ-0344
#[test]
fn artifact_cli_authorization_and_closed_parsing_precede_storage_effects() {
    let (temp, _, run) = fixture(b"unused");
    let missing = temp.path().join("never-opened");
    let id = run.to_string();
    let destination = temp.path().join("output");
    let path = destination.to_str().unwrap();
    let (code, _, error) = invoke(
        &missing,
        &["run", "artifact", "export", &id, "report", "--output", path],
    );
    assert_eq!(code, 1);
    assert!(error.contains("--authorize-sensitive-export"));
    for args in [
        vec![
            "run",
            "artifact",
            "export",
            &id,
            "report",
            "--output",
            "-",
            "--authorize-sensitive-export",
        ],
        vec![
            "run", "artifact", "export", &id, "report", "--output", path, "--output", path,
        ],
        vec![
            "run",
            "artifact",
            "export",
            &id,
            "report",
            "--output",
            path,
            "--authorize-sensitive-export",
            "--authorize-sensitive-export",
        ],
        vec!["run", "artifact", "export", &id, "report", "--force"],
        vec!["run", "artifact", "export", &id, "report", "--resume"],
        vec!["run", "artifact", "delete", &id, "report", "--force"],
    ] {
        assert_eq!(invoke(&missing, &args).0, 2);
    }
    assert!(!missing.exists());
    assert!(!destination.exists());
}

// Test-ID: PR-TEST-0450
// Verifies: PR-REQ-0341, PR-REQ-0342, PR-REQ-0344
#[test]
fn artifact_cli_exports_exact_chunked_bytes_and_deletes_only_selected_output() {
    let bytes = vec![0xa5; 1024 * 1024 + 17];
    let (temp, root, run) = fixture(&bytes);
    let id = run.to_string();
    let destination = temp.path().join("delivered");
    let path = destination.to_str().unwrap();
    let args = [
        "run",
        "artifact",
        "export",
        &id,
        "report",
        "--output",
        path,
        "--authorize-sensitive-export",
    ];
    assert_eq!(invoke(&root, &args).0, 0);
    assert_eq!(fs::read(&destination).unwrap(), bytes);
    assert_eq!(invoke(&root, &args).0, 1);
    assert_eq!(fs::read(&destination).unwrap(), bytes);
    let empty = temp.path().join("empty");
    assert_eq!(
        invoke(
            &root,
            &[
                "run",
                "artifact",
                "export",
                &id,
                "empty",
                "--output",
                empty.to_str().unwrap(),
                "--authorize-sensitive-export"
            ]
        )
        .0,
        0
    );
    assert!(fs::read(empty).unwrap().is_empty());
    for expected in ["deleted", "already absent"] {
        let (code, out, _) = invoke(&root, &["run", "artifact", "delete", &id, "report"]);
        assert_eq!(code, 0);
        assert!(out.contains(expected));
    }
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    let view = p.load_run(run).unwrap().unwrap();
    assert!(matches!(view.state, RunState::Finished(_)));
    p.open_run_artifact(
        run,
        &ManagedOutputIdentity::parse("empty").unwrap(),
        &mut vec![],
    )
    .unwrap();
    assert!(
        p.open_run_artifact(
            run,
            &ManagedOutputIdentity::parse("report").unwrap(),
            &mut vec![]
        )
        .is_err()
    );
    let missing = RunId::generate().unwrap().to_string();
    let (code, _, error) = invoke(&root, &["run", "artifact", "delete", &missing, "report"]);
    assert_eq!(code, 1);
    assert!(error.contains("parent Run is not persisted"));
}

// Test-ID: PR-TEST-0451
// Verifies: PR-REQ-0341, PR-REQ-0342
#[test]
fn artifact_consistent_reader_survives_concurrent_deletion() {
    let bytes = vec![0x63; 1024 * 1024 + 9];
    let (_temp, root, run) = fixture(&bytes);
    let reader = PactrunPersistence::open_read_only(&root).unwrap();
    let output = ManagedOutputIdentity::parse("report").unwrap();
    struct DeleteOnWrite<'a> {
        root: &'a Path,
        run: RunId,
        bytes: Vec<u8>,
        deleted: bool,
    }
    impl Write for DeleteOnWrite<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.deleted {
                let child = std::process::Command::new(env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "cli::artifacts::tests::artifact_deletion_process_worker",
                        "--nocapture",
                    ])
                    .env("PACTRUN_ARTIFACT_DELETE_ROOT", self.root)
                    .env("PACTRUN_ARTIFACT_DELETE_RUN", self.run.to_string())
                    .output()
                    .unwrap();
                assert!(
                    child.status.success(),
                    "{}",
                    String::from_utf8_lossy(&child.stderr)
                );
                self.deleted = true;
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut target = DeleteOnWrite {
        root: &root,
        run,
        bytes: vec![],
        deleted: false,
    };
    reader.open_run_artifact(run, &output, &mut target).unwrap();
    assert!(target.deleted);
    assert_eq!(target.bytes, bytes);
    assert!(reader.open_run_artifact(run, &output, &mut vec![]).is_err());
}

#[test]
fn artifact_deletion_process_worker() {
    let Some(root) = env::var_os("PACTRUN_ARTIFACT_DELETE_ROOT") else {
        return;
    };
    let run = env::var("PACTRUN_ARTIFACT_DELETE_RUN").unwrap();
    let (code, output, error) = invoke(
        Path::new(&root),
        &["run", "artifact", "delete", &run, "report"],
    );
    assert_eq!(code, 0, "{error}");
    assert!(output.contains("deleted"));
}

// Test-ID: PR-TEST-0452
// Verifies: PR-REQ-0275, PR-REQ-0342
#[test]
fn artifact_export_streams_the_exact_artifact_capacity_without_an_extra_limit() {
    assert_eq!(RUN_ARTIFACT_MAX_BYTES_V1, 536_870_912);
    let length = RUN_ARTIFACT_MAX_BYTES_V1;
    let (temp, root, run) = fixture_stream(&mut io::repeat(0x4d).take(length), length);
    let output = temp.path().join("large-output");
    let id = run.to_string();
    let (code, out, error) = invoke(
        &root,
        &[
            "run",
            "artifact",
            "export",
            &id,
            "report",
            "--output",
            output.to_str().unwrap(),
            "--authorize-sensitive-export",
        ],
    );
    assert_eq!(code, 0, "{error}");
    assert_eq!(out, "Artifact exported\n");
    assert_eq!(fs::metadata(&output).unwrap().len(), length);
    let mut file = File::open(output).unwrap();
    let mut buffer = [0u8; 64 * 1024];
    let mut observed = 0;
    loop {
        let read = file.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        assert!(buffer[..read].iter().all(|byte| *byte == 0x4d));
        observed += read as u64;
    }
    assert_eq!(observed, length);
}

// Test-ID: PR-TEST-0453
// Verifies: PR-REQ-0342
#[test]
fn artifact_export_failure_leaves_no_completed_output_or_sensitive_diagnostic() {
    let secret = b"artifact-private-sentinel";
    let (temp, root, run) = fixture(secret);
    let id = run.to_string();
    let missing_parent = temp.path().join("missing-parent/output");
    assert_eq!(
        invoke(
            &root,
            &[
                "run",
                "artifact",
                "export",
                &id,
                "report",
                "--output",
                missing_parent.to_str().unwrap(),
                "--authorize-sensitive-export"
            ]
        )
        .0,
        1
    );
    assert!(!missing_parent.exists());
    let output = temp.path().join("output");
    for identity in ["absent", "report"] {
        if identity == "report" {
            let db = rusqlite::Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
            db.execute(
                "DELETE FROM run_artifact_chunks WHERE run_id=?1 AND output_identity=?2",
                rusqlite::params![run.as_bytes().as_slice(), b"report".as_slice()],
            )
            .unwrap();
        }
        let (code, out, error) = invoke(
            &root,
            &[
                "run",
                "artifact",
                "export",
                &id,
                identity,
                "--output",
                output.to_str().unwrap(),
                "--authorize-sensitive-export",
            ],
        );
        assert_eq!(code, 1);
        assert!(out.is_empty());
        assert!(!error.contains(std::str::from_utf8(secret).unwrap()));
        assert!(!output.exists());
    }
    assert_eq!(fs::read_dir(root.join("staging")).unwrap().count(), 0);
}

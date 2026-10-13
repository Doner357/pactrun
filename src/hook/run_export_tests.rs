use super::*;
use crate::domain::*;

// Test-ID: PR-TEST-0783
// Verifies: PR-REQ-0404, PR-REQ-0405
#[test]
fn run_export_normal_empty_and_unrecorded_inputs_are_not_confused() {
    let f = RuntimeFixture::with_source(|_, _, manifest| {
        *manifest = manifest.replace("protection: secret", "protection: normal")
    });
    let current = f.application.load_instance(f.instance.id).unwrap().unwrap();
    f.application
        .set_input(
            current.id,
            InputIdentity::parse("secret_config").unwrap(),
            current.state_version,
            Box::new(Cursor::new(Vec::<u8>::new())),
        )
        .unwrap();
    let run = finished(&f, "success");
    let (code, result) = export(&f, run, "empty.zip", &["--include-inputs"]);
    assert_eq!(code, 0, "{result}");
    let (report, files) = contents(&f.marker("empty.zip"));
    assert_eq!(report["inputs"][0]["state"], "copied");
    assert!(files[report["inputs"][0]["member"].as_str().unwrap()].is_empty());
    assert_eq!(result["result"]["copied_inputs"], 1);
    assert_eq!(
        export(
            &f,
            run,
            "only-secrets.zip",
            &["--include-secrets", "--authorize-secret-export"]
        )
        .0,
        0
    );
    assert_eq!(
        contents(&f.marker("only-secrets.zip")).0["inputs"],
        json!([])
    );
    let db = rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    db.execute(
        "DELETE FROM run_input_contexts WHERE run_id=?1",
        [run.as_bytes().as_slice()],
    )
    .unwrap();
    assert_eq!(export(&f, run, "old.zip", &["--include-inputs"]).0, 0);
    let (report, files) = contents(&f.marker("old.zip"));
    assert_eq!(report["inspection"]["input_history"]["recorded"], false);
    assert_eq!(files.len(), 1);
    assert_ne!(
        export(
            &f,
            run,
            "duplicate.zip",
            &["--include-inputs", "--include-inputs"]
        )
        .0,
        0
    );
    assert!(!f.marker("duplicate.zip").exists());
}

fn finished(f: &RuntimeFixture, mode: &str) -> RunId {
    let admitted = f.admit("direct", mode, &f.marker(mode));
    let run = admitted.run();
    f.application.execute_admitted_action(
        admitted,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    assert!(f.application.advance_owner_continuation(run).unwrap());
    run
}
fn args(run: RunId, path: &Path, extra: &[&str]) -> Vec<std::ffi::OsString> {
    let mut args: Vec<_> = [
        "--format",
        "json",
        "run",
        "export",
        &run.to_string(),
        "--output",
    ]
    .iter()
    .map(std::ffi::OsString::from)
    .collect();
    args.push(path.as_os_str().to_owned());
    args.extend(extra.iter().map(std::ffi::OsString::from));
    args
}
pub(super) fn export(f: &RuntimeFixture, run: RunId, name: &str, extra: &[&str]) -> (i32, Value) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = crate::cli::run(
        args(run, &f.marker(name), extra),
        Some(f.storage.clone().into_os_string()),
        &mut io::empty(),
        &mut out,
        &mut err,
    );
    let value: Value = serde_json::from_slice(&out)
        .unwrap_or_else(|_| panic!("non-JSON report result: {:?}; stderr: {:?}", out, err));
    static SCHEMA: std::sync::OnceLock<jsonschema::Validator> = std::sync::OnceLock::new();
    let schema = SCHEMA.get_or_init(|| {
        let schema: Value = serde_json::from_str(include_str!(
            "../../docs/spec/interfaces/cli-machine.schema.json"
        ))
        .unwrap();
        jsonschema::validator_for(&schema).unwrap()
    });
    schema
        .validate(&value)
        .unwrap_or_else(|error| panic!("invalid report response: {error}; {value}"));
    (code, value)
}
pub(super) fn contents(path: &Path) -> (Value, BTreeMap<String, Vec<u8>>) {
    let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let mut files = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        assert!(entry.enclosed_name().is_some());
        let name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        assert!(files.insert(name, bytes).is_none());
    }
    let report = serde_json::from_slice(files.get("report.json").unwrap()).unwrap();
    (report, files)
}

// Test-ID: PR-TEST-0780
// Verifies: PR-REQ-0404, PR-REQ-0405, PR-REQ-0406
#[test]
fn run_export_defaults_authorizations_and_exact_copy_are_real_cli_behavior() {
    let f = RuntimeFixture::new();
    let run = finished(&f, "output");
    let before = persistence(&f.storage)
        .managed_run_inspection(run)
        .unwrap()
        .unwrap();
    let (code, result) = export(&f, run, "default.zip", &[]);
    assert_eq!(code, 0, "{result}");
    let (report, files) = contents(&f.marker("default.zip"));
    assert_eq!(files.len(), 1);
    assert_eq!(report["format"], "pactrun.run-report");
    assert_eq!(report["run_id"], run.to_string());
    assert_eq!(report["core_trace"]["state"], "unavailable");
    assert_eq!(
        report["selection"],
        json!({"inputs":false,"secrets":false,"hook_text":false,"artifacts":false})
    );
    assert!(
        report["inspection"]["input_history"]["recorded"]
            .as_bool()
            .unwrap()
    );
    let text = report.to_string();
    assert!(!contains(text.as_bytes(), SECRET_BINDING));
    assert!(!text.contains(SENSITIVE_PARAMETER));
    assert!(!text.contains(&f.storage.to_string_lossy().to_string()));
    assert!(!text.contains("payload_id") && !text.contains("snapshot_integrity"));
    let mut window = TraceWindow::new(
        TraceId::from_bytes([88; 16]),
        TraceOperation::ActionInvoke,
        Some(1),
        TracePolicy::default(),
    );
    window.associate(run);
    window.close(0);
    let trace = window.snapshot();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    crate::trace_journal::Journal::open(&crate::trace_journal::Location::store(&f.storage))
        .unwrap()
        .save(&trace, now)
        .unwrap();
    assert_eq!(export(&f, run, "with-trace.zip", &[]).0, 0);
    let (with_trace, _) = contents(&f.marker("with-trace.zip"));
    assert_eq!(with_trace["core_trace"]["state"], "available");
    assert_eq!(
        with_trace["core_trace"]["record"],
        serde_json::to_value(trace).unwrap()
    );
    for flags in [
        vec!["--include-secrets"],
        vec!["--include-artifacts"],
        vec!["--include-hook-text"],
        vec!["--include-secrets", "--authorize-sensitive-export"],
    ] {
        let (code, _) = export(&f, run, "refused.zip", &flags);
        assert_ne!(code, 0);
        assert!(!f.marker("refused.zip").exists());
    }
    let (code, result) = export(
        &f,
        run,
        "permissions-only.zip",
        &["--authorize-sensitive-export", "--authorize-secret-export"],
    );
    assert_eq!(code, 0, "{result}");
    assert_eq!(contents(&f.marker("permissions-only.zip")).1.len(), 1);
    let flags = [
        "--include-secrets",
        "--authorize-secret-export",
        "--include-artifacts",
        "--authorize-sensitive-export",
    ];
    let (code, result) = export(&f, run, "selected.zip", &flags);
    assert_eq!(code, 0, "{result}");
    let (report, files) = contents(&f.marker("selected.zip"));
    assert_eq!(report["inputs"][0]["state"], "copied");
    assert_eq!(
        files[report["inputs"][0]["member"].as_str().unwrap()],
        SECRET_BINDING
    );
    assert_eq!(
        files[report["artifacts"][0]["member"].as_str().unwrap()],
        b"unpublished-output"
    );
    assert_eq!(result["result"]["copied_inputs"], 1);
    assert_eq!(result["result"]["copied_artifacts"], 1);
    assert_eq!(
        persistence(&f.storage)
            .managed_run_inspection(run)
            .unwrap()
            .unwrap(),
        before,
        "export mutated Run evidence"
    );
    let preserved = fs::read(f.marker("selected.zip")).unwrap();
    let (code, _) = export(&f, run, "selected.zip", &flags);
    assert_ne!(code, 0);
    assert_eq!(fs::read(f.marker("selected.zip")).unwrap(), preserved);
    let db = rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap();
    db.execute(
        "UPDATE run_artifacts SET byte_length=byte_length+1 WHERE run_id=?1",
        [run.as_bytes().as_slice()],
    )
    .unwrap();
    assert_ne!(
        export(
            &f,
            run,
            "corrupt-artifact.zip",
            &["--include-artifacts", "--authorize-sensitive-export"]
        )
        .0,
        0
    );
    assert!(!f.marker("corrupt-artifact.zip").exists());
    db.execute(
        "UPDATE run_artifacts SET byte_length=byte_length-1 WHERE run_id=?1",
        [run.as_bytes().as_slice()],
    )
    .unwrap();
    f.replace_binding();
    let (code, result) = export(&f, run, "reclaimed.zip", &flags);
    assert_eq!(code, 0, "{result}");
    let (report, files) = contents(&f.marker("reclaimed.zip"));
    assert_eq!(report["inputs"][0]["state"], "unavailable");
    assert!(report["inputs"][0]["member"].is_null());
    assert!(files.keys().all(|name| !name.starts_with("inputs/")));
    assert_eq!(result["result"]["unavailable_inputs"], 1);
}

// Test-ID: PR-TEST-0781
// Verifies: PR-REQ-0404, PR-REQ-0405, PR-REQ-0406
#[test]
fn run_export_hook_text_is_opt_in_and_copy_corruption_never_publishes() {
    let f = RuntimeFixture::new();
    let run = finished(&f, "sensitive_diagnostic");
    assert_eq!(export(&f, run, "without-text.zip", &[]).0, 0);
    let (report, _) = contents(&f.marker("without-text.zip"));
    assert!(!contains(report.to_string().as_bytes(), SECRET_BINDING));
    assert_eq!(report["inspection"]["diagnostics"]["events"], json!([]));
    let (code, result) = export(
        &f,
        run,
        "with-text.zip",
        &["--include-hook-text", "--authorize-sensitive-export"],
    );
    assert_eq!(code, 0, "{result}");
    let (report, _) = contents(&f.marker("with-text.zip"));
    assert!(contains(report.to_string().as_bytes(), SECRET_BINDING));
    let db = rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap();
    db.execute(
        "UPDATE managed_input_payloads SET byte_length=byte_length+1",
        [],
    )
    .unwrap();
    let (code, _) = export(
        &f,
        run,
        "corrupt.zip",
        &["--include-secrets", "--authorize-secret-export"],
    );
    assert_ne!(code, 0);
    assert!(!f.marker("corrupt.zip").exists());
    // An unselected corrupt payload is not read as report content.
    assert_eq!(export(&f, run, "metadata-after-corruption.zip", &[]).0, 0);
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let path = f.marker("response-failed.zip");
    let mut err = Vec::new();
    assert_ne!(
        crate::cli::run(
            args(run, &path, &[]),
            Some(f.storage.clone().into_os_string()),
            &mut io::empty(),
            &mut Broken,
            &mut err
        ),
        0
    );
    assert_eq!(
        contents(&path).0["run_id"],
        run.to_string(),
        "response failure undid completed publication"
    );
}

// Test-ID: PR-TEST-0782
// Verifies: PR-REQ-0405, PR-REQ-0406
#[test]
fn run_export_read_snapshot_survives_concurrent_binding_replacement_and_artifact_deletion() {
    use crate::persistence::{PersistenceError, RunExportSink};
    let f = RuntimeFixture::new();
    let run = finished(&f, "output");
    let p = persistence(&f.storage);
    struct Sink<'a> {
        f: &'a RuntimeFixture,
        run: RunId,
        inputs: Vec<u8>,
        artifacts: Vec<u8>,
    }
    impl RunExportSink for Sink<'_> {
        fn inspection(&mut self, _: &ManagedRunInspectionData) -> Result<(), PersistenceError> {
            self.f.replace_binding();
            assert!(
                persistence(&self.f.storage)
                    .delete_run_artifact(self.run, &ManagedOutputIdentity::parse("report").unwrap())
                    .unwrap()
            );
            Ok(())
        }
        fn input(
            &mut self,
            _: &HistoricalInputContext,
            _: &HistoricalInput,
            copy: impl FnOnce(&mut dyn Write) -> Result<HistoricalInputExport, PersistenceError>,
        ) -> Result<(), PersistenceError> {
            assert!(matches!(
                copy(&mut self.inputs)?,
                HistoricalInputExport::Copied { .. }
            ));
            Ok(())
        }
        fn artifact(
            &mut self,
            _: &RunArtifactSummary,
            copy: impl FnOnce(&mut dyn Write) -> Result<(), PersistenceError>,
        ) -> Result<(), PersistenceError> {
            copy(&mut self.artifacts)
        }
    }
    let mut sink = Sink {
        f: &f,
        run,
        inputs: vec![],
        artifacts: vec![],
    };
    p.write_run_export(
        run,
        RunExportOptions {
            secrets: true,
            artifacts: true,
            authorize_secret: true,
            authorize_sensitive: true,
            ..Default::default()
        },
        &mut sink,
    )
    .unwrap();
    assert_eq!(sink.inputs, SECRET_BINDING);
    assert_eq!(sink.artifacts, b"unpublished-output");
    assert_eq!(
        export(
            &f,
            run,
            "after-deletion.zip",
            &["--include-secrets", "--authorize-secret-export"]
        )
        .0,
        0
    );
    assert_eq!(
        contents(&f.marker("after-deletion.zip")).0["inputs"][0]["state"],
        "unavailable"
    );
}

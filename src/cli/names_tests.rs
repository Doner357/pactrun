// Included in cli::tests; all stores and Pack sources live under target/.
fn names_call(root: &Path, args: &[&str]) -> (i32, serde_json::Value) {
    let args = ["--format", "json"]
        .into_iter()
        .chain(args.iter().copied())
        .map(OsString::from)
        .collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run(
        args,
        Some(root.as_os_str().into()),
        &mut io::empty(),
        &mut out,
        &mut err,
    );
    let response = serde_json::from_slice(&out).unwrap_or_else(|e| {
        panic!(
            "{e}: {} / {}",
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(&err)
        )
    });
    (code, response)
}

// Test-ID: PR-TEST-0662
// Verifies: PR-REQ-0371, PR-REQ-0372
#[test]
fn local_names_install_rename_and_unname_preserve_instance_identity_and_time() {
    let (_temp, root, source) = cli_roots();
    let install = [
        "pack",
        "install",
        source.to_str().unwrap(),
        "--package-name",
        "web",
        "--revision-name",
        "stable",
    ];
    let (code, response) = names_call(&root, &install);
    assert_eq!(code, 0, "{response}");
    let id = RevisionIdentity::new(
        response["result"]["revision"]["package_id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        response["result"]["revision"]["content_digest"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
    );
    let app = PactrunApplication::open_read_only(&root).unwrap();
    let before = app.local_revision_facts(&id).unwrap();
    assert!(before.installed_at_unix_ms.is_some());
    assert_eq!(before.package_name.unwrap().as_str(), "web");
    assert_eq!(before.revision_name.unwrap().as_str(), "stable");
    assert_eq!(
        names_call(
            &root,
            &["instance", "create", "demo", "--revision", "web:stable"]
        )
        .0,
        0
    );
    assert_eq!(names_call(&root, &install).0, 0);
    assert_eq!(
        app.local_revision_facts(&id).unwrap().installed_at_unix_ms,
        before.installed_at_unix_ms
    );
    for args in [
        vec!["package", "rename", "web", "frontend"],
        vec!["revision", "rename", "frontend:stable", "testing"],
    ] {
        let (code, response) = names_call(&root, &args);
        assert_eq!(code, 0, "{response}");
    }
    assert_eq!(
        app.resolve_named_revision(
            &crate::domain::LocalRevisionReference::parse("frontend:testing").unwrap()
        )
        .unwrap(),
        id
    );
    let instance = app
        .resolve_instance_name(&InstanceName::parse("demo").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        app.load_instance(instance)
            .unwrap()
            .unwrap()
            .active_revision,
        id
    );
    assert_eq!(
        names_call(&root, &["revision", "unname", "frontend:testing"]).0,
        0
    );
    assert_eq!(names_call(&root, &["package", "unname", "frontend"]).0, 0);
    let exact = format!(
        "{}:{}",
        id.package_id,
        hex::encode(id.content_digest.as_bytes())
    );
    assert_eq!(
        app.resolve_named_revision(&crate::domain::LocalRevisionReference::parse(&exact).unwrap())
            .unwrap(),
        id
    );
    assert_eq!(
        app.local_revision_facts(&id).unwrap().installed_at_unix_ms,
        before.installed_at_unix_ms
    );
}

// Test-ID: PR-TEST-0663
// Verifies: PR-REQ-0372
#[test]
fn local_names_install_conflict_rolls_back_revision_and_name_changes() {
    let (_temp, root, source) = cli_roots();
    let args = [
        "pack",
        "install",
        source.to_str().unwrap(),
        "--package-name",
        "web",
        "--revision-name",
        "stable",
    ];
    let (code, response) = names_call(&root, &args);
    assert_eq!(code, 0, "{response}");
    let manifest = source.join("pactrun.yaml");
    let original = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, original.replace("id: config", "id: changed")).unwrap();
    let (code, response) = names_call(&root, &args);
    assert_eq!(code, 1, "{response}");
    let db = rusqlite::Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    for table in [
        "packages",
        "revisions",
        "revision_local_names",
        "revision_installations",
        "package_local_names",
    ] {
        let count: i64 = db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "{table}");
    }
    fs::write(
        &manifest,
        original.replace(
            "00000000000000000000000000000021",
            "00000000000000000000000000000022",
        ),
    )
    .unwrap();
    assert_eq!(names_call(&root, &args).0, 1);
    let count: i64 = db
        .query_row("SELECT count(*) FROM packages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let args = [
        "pack",
        "install",
        source.to_str().unwrap(),
        "--package-name",
        "database",
        "--revision-name",
        "stable",
    ];
    assert_eq!(names_call(&root, &args).0, 0);
    assert_eq!(
        names_call(&root, &["package", "rename", "database", "web"]).0,
        1
    );
}

// Test-ID: PR-TEST-0670
// Verifies: PR-REQ-0371
#[test]
fn local_names_reject_long_names_but_accept_complete_ids() {
    let (_temp, root, source) = cli_roots();
    let a = "00000000000000000000000000000021";
    for option in ["--package-name", "--revision-name"] {
        let args = ["pack", "install", source.to_str().unwrap(), option, a];
        let (code, response) = names_call(&root, &args);
        assert_eq!(code, 2, "{response}");
        assert!(
            response["error"]["message"]
                .as_str()
                .unwrap()
                .contains("1..31")
        );
        assert!(!root.join("database/pactrun.sqlite3").exists());
    }
    let boundary = "a".repeat(31);
    let args = [
        "pack",
        "install",
        source.to_str().unwrap(),
        "--package-name",
        &boundary,
        "--revision-name",
        "stable",
    ];
    let (code, response) = names_call(&root, &args);
    assert_eq!(code, 0, "{response}");
    let digest = response["result"]["revision"]["content_digest"]
        .as_str()
        .unwrap()
        .strip_prefix("sha256:")
        .unwrap();
    let reference = format!("{a}:{digest}");
    assert_eq!(
        names_call(&root, &["revision", "rename", &reference, &boundary]).0,
        0
    );
    let too_long = "b".repeat(32);
    assert_eq!(names_call(&root, &["package", "rename", a, &too_long]).0, 2);
    assert_eq!(
        names_call(&root, &["revision", "rename", &reference, &too_long]).0,
        2
    );
    assert_eq!(names_call(&root, &["revision", "unname", &reference]).0, 0);
    assert_eq!(names_call(&root, &["package", "unname", a]).0, 0);
}

// Test-ID: PR-TEST-0682
// Verifies: PR-REQ-0353, PR-REQ-0377
#[test]
fn human_revision_inspection_shows_each_author_explanation_once_without_empty_metadata() {
    let (_temp, root, source) = cli_roots();
    let path = source.join("pactrun.yaml");
    let mut yaml = fs::read_to_string(&path).unwrap();
    yaml.push_str("portable_metadata:\n  presentation:\n    - {target: {kind: revision}, field: summary, value: revision-summary-sentinel}\n    - {target: {kind: input, input_id: config}, field: help, value: input-help-sentinel}\n  provenance:\n    - {kind: source_uri, source_uri: 'https://example.invalid/pack'}\n");
    fs::write(&path, yaml).unwrap();
    let reference = json_install(&root, &source);
    let mut out = Vec::new();
    let mut err = Vec::new();
    assert_eq!(run(["revision", "show", &reference].map(Into::into).to_vec(),
        Some(root.as_os_str().into()), &mut io::empty(), &mut out, &mut err), 0, "{err:?}");
    let text = String::from_utf8(out).unwrap();
    for explanation in ["revision-summary-sentinel", "input-help-sentinel", "https://example.invalid/pack"] {
        assert_eq!(text.matches(explanation).count(), 1, "{text}");
    }
    assert!(!text.contains("Description: Not provided"));
    assert!(!text.contains("Display Name: Not provided"));
    assert!(text.contains("\nSummary\n  revision-summary-sentinel\n"));
    assert!(text.contains("Input ID: config"));
    assert!(text.contains("Protection: secret"));
}

// Test-ID: PR-TEST-0680
// Verifies: PR-REQ-0377
#[test]
fn all_output_formats_use_the_same_complete_query_and_refuse_invalid_metadata() {
    let (_temp, root, source) = cli_roots();
    let installed = json_ok(&root, &["pack", "install", source.to_str().unwrap()]);
    let reference = installed["reference"].as_str().unwrap();
    json_ok(
        &root,
        &["instance", "create", "demo", "--revision", reference],
    );
    let path = root.join("database/pactrun.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("INSERT INTO revision_action_presentation_current(package_id,revision_content_digest,action_id_utf8,summary_utf8) VALUES (?1,?2,?3,?4)",rusqlite::params![hex::decode(installed["revision"]["package_id"].as_str().unwrap()).unwrap(),hex::decode(installed["revision"]["content_digest"].as_str().unwrap().strip_prefix("sha256:").unwrap()).unwrap(),b"undeclared".as_slice(),b"invalid".as_slice()]).unwrap();
    drop(db);
    let before = fs::read(&path).unwrap();
    for format in ["human", "json", "jsonl"] {
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(
            run(
                ["--format", format, "instance", "list"]
                    .map(OsString::from)
                    .to_vec(),
                Some(root.as_os_str().into()),
                &mut io::empty(),
                &mut out,
                &mut err
            ),
            1,
            "{format}: {}",
            String::from_utf8_lossy(&out)
        );
    }
    assert_eq!(fs::read(&path).unwrap(), before);
}

// Test-ID: PR-TEST-0679
// Verifies: PR-REQ-0375
#[test]
fn export_warning_delivery_failure_cannot_cancel_publication() {
    let (temp, root, source) = cli_roots();
    let reference = json_install(&root, &source);
    let base = temp.path().join("exported");
    let output = base.with_extension("pack");
    struct Warning<'a> {
        path: &'a Path,
        seen: bool,
    }
    impl Write for Warning<'_> {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            assert!(self.path.is_file());
            self.seen = true;
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }
    let mut warning = Warning {
        path: &output,
        seen: false,
    };
    let mut stdout = Vec::new();
    let args = [
        OsString::from("--format"),
        "json".into(),
        "revision".into(),
        "export".into(),
        reference.into(),
        "--output".into(),
        base.into_os_string(),
    ];
    assert_eq!(
        run(
            args.into(),
            Some(root.as_os_str().into()),
            &mut io::empty(),
            &mut stdout,
            &mut warning
        ),
        1
    );
    assert!(warning.seen);
    assert!(output.is_file());
    let value: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(value["error"]["kind"], "output");
    assert_eq!(value["result"]["output"]["value"], output.to_str().unwrap());
    schema_tests::assert_response(&value);
}

// Test-ID: PR-TEST-0677
// Verifies: PR-REQ-0371, PR-REQ-0372, PR-REQ-0374
#[test]
fn first_install_creates_unicode_store_path_and_applies_local_names() {
    let (temp, _, source) = cli_roots();
    let root = temp.path().join("store-\u{4e2d}\u{6587}");
    let (code, response) = names_call(
        &root,
        &[
            "pack",
            "install",
            source.to_str().unwrap(),
            "--package-name",
            "demo",
            "--revision-name",
            "initial",
        ],
    );
    assert_eq!(code, 0, "{response}");
    assert_eq!(
        names_call(
            &root,
            &["instance", "create", "demo", "--revision", "demo:initial"]
        )
        .0,
        0
    );
    let db = rusqlite::Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    let package_name: String = db
        .query_row("SELECT name FROM package_local_names", [], |r| r.get(0))
        .unwrap();
    assert_eq!(package_name, "demo");
    for name in ["a".repeat(32), "\u{4e2d}\u{6587}".into(), "a\0b".into()] {
        assert!(
            db.execute("UPDATE package_local_names SET name=?1", [&name])
                .is_err()
        );
        assert!(
            db.execute("UPDATE revision_local_names SET name=?1", [&name])
                .is_err()
        );
    }
}

// Test-ID: PR-TEST-0674
// Verifies: PR-REQ-0374
#[test]
fn missing_store_global_queries_are_empty_without_creating_directories() {
    let (temp, _, _) = cli_roots();
    let root = temp.path().join("missing-store");
    for args in [
        vec!["instance", "list"],
        vec!["revision", "list"],
        vec!["run", "list"],
        vec!["snapshot", "list"],
        vec!["instance", "history", "list"],
        vec!["instance", "deletion", "list"],
        vec!["service-storage", "detached", "list"],
    ] {
        for format in ["human", "json", "jsonl"] {
            let arguments = ["--format", format]
                .into_iter()
                .chain(args.iter().copied())
                .map(OsString::from)
                .collect();
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                run(
                    arguments,
                    Some(root.as_os_str().into()),
                    &mut io::empty(),
                    &mut out,
                    &mut err
                ),
                0,
                "{args:?}/{format}: {}",
                String::from_utf8_lossy(&err)
            );
            if format != "human" {
                let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
                let response = if format == "jsonl" {
                    &value["response"]
                } else {
                    &value
                };
                schema_tests::assert_response(response);
                assert_eq!(response["result"]["items"].as_array().unwrap().len(), 0);
            }
            assert!(!root.exists());
        }
    }
    assert_eq!(names_call(&root, &["instance", "show", "missing"]).0, 1);
    assert_eq!(names_call(&root, &["run", "list", "missing"]).0, 1);
    assert!(!root.exists());
}

// Test-ID: PR-TEST-0664
// Verifies: PR-REQ-0371, PR-REQ-0372
#[test]
fn local_names_invalid_names_are_refused_before_store_access() {
    let (_temp, root, _source) = cli_roots();
    for args in [
        vec![
            "pack",
            "install",
            "missing",
            "--package-name",
            "\u{4e2d}\u{6587}",
        ],
        vec!["package", "rename", "web", "new name"],
        vec!["revision", "rename", "web:stable", ":bad"],
    ] {
        assert_eq!(names_call(&root, &args).0, 2);
        assert!(!root.join("database/pactrun.sqlite3").exists());
    }
}

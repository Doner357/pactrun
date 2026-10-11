use super::*;

fn query(root: Option<&Path>, args: &[&str]) -> (i32, String, String) {
    struct NoInput;
    impl Read for NoInput {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("help must not read stdin")
        }
    }
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = crate::cli::run(
        args.iter().map(OsString::from).collect(),
        root.map(|p| p.as_os_str().to_owned()),
        &mut NoInput,
        &mut out,
        &mut err,
    );
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

// Test-ID: PR-TEST-0649
// Verifies: PR-REQ-0271, PR-REQ-0359, PR-REQ-0360
#[test]
fn help_has_equal_scopes_and_definitions_without_a_store() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/help-tests");
    std::fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    let absent = temp.path().join("must-not-create");
    let mut topics = vec![Vec::new()];
    topics.extend(catalog().commands.iter().map(|c| c.path.clone()));
    for scope in topics {
        let mut args: Vec<&str> = scope.iter().map(String::as_str).collect();
        args.push("--help");
        let (code, human, error) = query(Some(&absent), &args);
        assert_eq!(code, 0, "{args:?}: {error}");
        assert!(error.is_empty());
        let expected = request(&args.iter().map(OsString::from).collect::<Vec<_>>())
            .unwrap()
            .result();
        let expected = serde_json::to_value(expected).unwrap();
        for format in ["json", "jsonl"] {
            let mut machine = vec!["--format", format];
            machine.extend(&args);
            let (code, out, err) = query(Some(&absent), &machine);
            assert_eq!(code, 0, "{machine:?}: {out}; {err}");
            assert!(err.is_empty());
            let value: serde_json::Value = serde_json::from_str(&out).unwrap();
            let response = if format == "jsonl" {
                &value["response"]
            } else {
                &value
            };
            crate::cli::schema_tests::assert_response(response);
            assert_eq!(response["result"], expected);
            assert_eq!(response["result"]["usage"], human);
            assert_eq!(response["result"]["scope"], serde_json::json!(scope));
        }
        assert!(!absent.exists());
    }
    let root = query(None, &["--help"]).1;
    assert!(
        root.lines().count() <= 40,
        "root help should be navigation, not a manual: {root}"
    );
    assert!(!root.contains("pactrun run artifact export"));
    let group = query(None, &["instance", "--help"]).1;
    assert!(group.contains("create") && group.contains("deletion"));
    assert!(!group.contains("--restore-from"));
    let leaf = query(None, &["instance", "create", "--help"]).1;
    assert!(leaf.contains("--restore-from") && leaf.contains("--input-file"));
    assert!(leaf.contains("mutually") || leaf.contains("cannot be combined"));
    assert!(
        query(None, &["instance", "abandon", "--help"])
            .1
            .contains("does not stop the service")
    );
    assert!(
        query(None, &["invoke", "--help"])
            .1
            .contains("does not repair or acknowledge recovery")
    );
}

// Supporting coverage for PR-TEST-0649.
#[test]
fn help_catalog_is_closed_complete_and_unambiguous() {
    let catalog = catalog();
    let mut paths = std::collections::BTreeSet::new();
    for entry in &catalog.commands {
        assert!(!entry.path.is_empty());
        assert!(paths.insert(entry.path.clone()));
        assert!(!entry.description.is_empty());
        for form in &entry.forms {
            assert!(
                form.usage
                    .starts_with(&format!("pactrun {}", entry.path.join(" ")))
            );
            assert!(!form.description.is_empty());
        }
        let mut names = std::collections::BTreeSet::new();
        for name in &entry.options {
            let option = catalog
                .options
                .get(name)
                .expect("all options must be declared");
            assert!(names.insert(&option.name));
            assert!(option.name.starts_with("--"));
            assert!(!option.description.is_empty());
        }
    }
    for entry in &catalog.commands {
        if entry.path.len() > 1 {
            assert!(paths.contains(&entry.path[..entry.path.len() - 1]));
        }
        if entry.forms.is_empty() {
            assert!(
                catalog
                    .commands
                    .iter()
                    .any(|c| c.path.starts_with(&entry.path) && c.path.len() > entry.path.len())
            );
        }
    }
    assert!(
        catalog
            .commands
            .iter()
            .map(|c| c.forms.len())
            .sum::<usize>()
            >= 60
    );
}

// Supporting coverage for PR-TEST-0649.
#[test]
fn help_does_not_swallow_operands_or_invalid_requests() {
    for args in [
        vec!["unknown", "--help"],
        vec!["instance", "unknown", "--help"],
        vec!["instance", "--help", "--bogus"],
        vec!["--help", "extra"],
    ] {
        for format in ["human", "json", "jsonl"] {
            let mut command = vec!["--format", format];
            command.extend(&args);
            assert_eq!(query(None, &command).0, 2, "{command:?}");
        }
    }
    let parsed = parse_command(
        ["pack", "install", "--", "--help"]
            .map(OsString::from)
            .to_vec(),
    )
    .unwrap();
    assert!(matches!(parsed,Command::Install{source_root,..} if source_root==Path::new("--help")));
    let reference = format!("{}:{}", "0".repeat(32), "1".repeat(64));
    assert!(matches!(
        parse_command(
            [
                "revision",
                "note",
                "set",
                &reference,
                "--expect-absent",
                "--value",
                "--help"
            ]
            .map(OsString::from)
            .to_vec()
        )
        .unwrap(),
        Command::Catalog(_)
    ));
    assert_eq!(query(None, &["hook", "-h"]).0, 0);
    assert_eq!(query(None, &["--format", "json", "hook", "-h"]).0, 0);
}

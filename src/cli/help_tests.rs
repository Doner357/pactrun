use super::*;

// Test-ID: PR-TEST-0649
// Verifies: PR-REQ-0271, PR-REQ-0359, PR-REQ-0360
#[test]
fn help_describes_commands_and_has_equal_machine_content_without_a_store() {
    let temp = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
    let missing = temp.path().join("must-not-create");
    let mut human = vec![];
    assert_eq!(
        run(
            vec!["--help".into()],
            Some(missing.as_os_str().to_owned()),
            &mut io::empty(),
            &mut human,
            &mut vec![]
        ),
        0
    );
    let text = String::from_utf8(human).unwrap();
    let lines: Vec<_> = text.lines().collect();
    let mut count = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.trim().starts_with("pactrun ") && !line.contains("[--format") {
            let description = lines.get(i + 1).expect("command description").trim();
            assert!(
                !description.is_empty() && !description.starts_with("pactrun "),
                "{line}"
            );
            count += 1;
        }
    }
    assert!(count >= 60);
    for format in ["json", "jsonl"] {
        let mut out = vec![];
        let mut err = vec![];
        assert_eq!(
            run(
                ["--format", format, "--help"]
                    .iter()
                    .map(OsString::from)
                    .collect(),
                Some(missing.as_os_str().to_owned()),
                &mut io::empty(),
                &mut out,
                &mut err
            ),
            0
        );
        assert!(err.is_empty());
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let response = if format == "jsonl" {
            &value["response"]
        } else {
            &value
        };
        assert_eq!(response["result"]["usage"], text);
        schema_tests::assert_response(response);
    }
    assert!(!missing.exists());
    assert!(text.contains("does not stop the service"));
    assert!(text.contains("does not repair or acknowledge recovery"));
}

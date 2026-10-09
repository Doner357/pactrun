use super::*;
use serde_json::json;

fn show(command: &str, value: &Value) -> String {
    let before = value.clone();
    let mut output = Vec::new();
    render(
        command,
        value,
        &reply::DisplayOptions::default(),
        &mut output,
    )
    .unwrap();
    assert_eq!(
        *value, before,
        "Human rendering must not rewrite public data"
    );
    String::from_utf8(output).unwrap()
}

// Test-ID: PR-TEST-0683
// Verifies: PR-REQ-0353, PR-REQ-0363, PR-REQ-0377
#[test]
fn human_summaries_use_their_own_field_in_lists_and_details() {
    let metadata = json!({"summary":"first\r\nsecond\u{1b}[31m", "display_name":"not-the-summary", "description":"also-not-the-summary"});
    assert_eq!(summary_cell(&metadata), "first second\\u{1b}[31m");
    assert_eq!(
        summary_cell(&json!({"summary":null,"display_name":"wrong","description":"wrong"})),
        ""
    );
    assert_eq!(summary_cell(&json!({"summary":""})), "");
    let action = show(
        "action list",
        &json!({"items":[
            {"action_id":"a","access":"observe","metadata":metadata},
            {"action_id":"b","access":"observe","metadata":{"display_name":"wrong","description":"wrong"}}
        ]}),
    );
    assert_eq!(
        action
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>(),
        ["ACTION", "ACCESS", "SUMMARY"]
    );
    assert!(action.contains("first second\\u{1b}[31m"));
    assert!(!action.contains("not-the-summary") && !action.contains("wrong"));
    assert_eq!(
        action
            .lines()
            .nth(2)
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>(),
        ["b", "observe"]
    );

    let revision = show(
        "revision list",
        &json!({"items":[{
        "reference":"demo:initial", "local":{"installed_at_unix_ms":null},
        "declarations":{"capabilities":{"metadata":{"summary":"Revision summary."}}}
    }],"next":null}),
    );
    assert!(revision.lines().next().unwrap().ends_with("SUMMARY"));
    assert!(revision.contains("Revision summary."));

    let detail = show(
        "action show",
        &json!({"action_id":"a","access":"observe","metadata":metadata}),
    );
    assert!(detail.contains("\nSummary\n  first\n  second\\u{1b}[31m\n"));
    assert!(!detail.contains('\u{1b}'));
}

// Supporting coverage for PR-TEST-0683.
#[test]
fn human_input_summaries_only_use_the_active_defining_revision() {
    let mut value = json!({
        "items":[
            {"input_id":"config","role":"active","required":false,"present":false,"protection":"normal"},
            {"input_id":"legacy","role":"retained","required":null,"present":true,"protection":"normal"},
            {"input_id":"unnamed","role":"active","required":false,"present":false,"protection":"normal"}
        ],
        "related_revisions":[{"declarations":{"capabilities":{"inputs":[
            {"input_id":"config","metadata":{"summary":"Configuration summary."}},
            {"input_id":"legacy","metadata":{"summary":"Do not borrow this declaration."}},
            {"input_id":"unnamed","metadata":{"display_name":"Not a summary"}}
        ]}}}]
    });
    let text = show("input list", &value);
    assert!(text.lines().next().unwrap().ends_with("SUMMARY"));
    assert!(text.contains("Configuration summary."));
    assert!(!text.contains("Do not borrow") && !text.contains("Not a summary"));
    value["related_revisions"] = json!([]);
    assert!(!show("input list", &value).contains("Configuration summary."));
}

// Supporting coverage for PR-TEST-0683.
#[test]
fn human_nested_capabilities_and_outputs_share_summary_blocks() {
    for kind in [
        "revision",
        "input",
        "action",
        "action_parameter",
        "managed_output",
        "snapshot_capture",
        "snapshot_capture_parameter",
        "snapshot_restore",
        "snapshot_restore_parameter",
        "migration_edge",
        "cleanup",
    ] {
        let metadata = json!({"summary":"first\nsecond","help":null});
        for value in [
            json!({"target":{"kind":kind},"metadata":metadata}),
            json!({"target":{"kind":kind},"summary":"first\nsecond","help":null}),
        ] {
            let mut out = Vec::new();
            tree(&value, &mut out, 2).unwrap();
            let text = String::from_utf8(out).unwrap();
            assert!(
                text.contains("\n  Summary\n    first\n    second\n"),
                "{kind}: {text}"
            );
            assert_eq!(text.matches("Summary").count(), 1);
            assert!(!text.contains("Help"));
        }
    }
    let action = show(
        "action show",
        &json!({"action_id":"a","access":"observe","metadata":{},
        "outputs":[{"output_id":"report","metadata":{"summary":"Report summary."}}]}),
    );
    assert!(action.contains("Outputs\n  Output: report\n\n    Summary\n      Report summary.\n"));
    let raw = show(
        "revision metadata show",
        &json!({"metadata":[{
            "kind":"presentation","target":{"kind":"revision"},"field":"summary","value":"Raw summary."
        }]}),
    );
    assert!(raw.contains("Field: summary") && raw.contains("Value: Raw summary."));
}

// Supporting coverage for PR-TEST-0683.
#[test]
fn human_migration_summaries_belong_to_exact_edges_not_whole_paths() {
    let a = json!({"package_id":"package","content_digest":"sha256:a"});
    let b = json!({"package_id":"package","content_digest":"sha256:b"});
    let c = json!({"package_id":"package","content_digest":"sha256:c"});
    let foreign = json!({"package_id":"other-package","content_digest":"sha256:b"});
    let mut value = json!({"candidates":[{"path_id":"mp1-example","unique_prefix_length":64,
        "edge_count":2,"revisions":[a,b,c]}],"next_after":null,"presentation":[
        {"revision":foreign,"entries":[{"target":{"kind":"migration_edge","source_digest":"sha256:a"},"summary":"Wrong package"}]},
        {"revision":b,"entries":[
            {"target":{"kind":"revision"},"summary":"Not a path summary"},
            {"target":{"kind":"migration_edge","source_digest":"sha256:wrong"},"summary":"Wrong edge"},
            {"target":{"kind":"migration_edge","source_digest":"sha256:a"},"summary":"First edge."}
        ]},
        {"revision":c,"entries":[{"target":{"kind":"migration_edge","source_digest":"sha256:b"},"summary":"Second edge."}]}
    ]});
    let text = show("instance migration-paths", &value);
    assert!(text.contains("STEP  SUMMARY"));
    assert!(text.contains("1     First edge.") && text.contains("2     Second edge."));
    assert!(!text.contains("Wrong") && !text.contains("Not a path"));
    value["presentation"][2]["entries"][0]["summary"] = Value::Null;
    value["presentation"][2]["entries"][0]["display_name"] = json!("Do not substitute");
    let text = show("instance migration-paths", &value);
    assert!(!text.contains("Do not substitute"));
    assert_eq!(text.lines().last(), Some("2"));
}

// Supporting coverage for PR-TEST-0683.
#[test]
fn human_object_lists_do_not_borrow_summaries_from_related_definitions() {
    for command in ["instance list", "run list", "snapshot list"] {
        let value = json!({"items":[{"name":"sample","run_id":"run","instance_id":"instance","snapshot_id":"snapshot"}],
            "related_revisions":[{"declarations":{"capabilities":{"metadata":{"summary":"Not this object's summary"}}}}]});
        let text = show(command, &value);
        assert!(!text.contains("SUMMARY") && !text.contains("Not this object's summary"));
    }
}

// Test-ID: PR-TEST-0688
// Verifies: PR-REQ-0364, PR-REQ-0095, PR-REQ-0377
#[test]
fn human_execution_views_show_targets_blockers_and_retained_input_roles() {
    let instance = json!({"name":"sample","instance_id":"instance","state_version":"state", "required_inputs_satisfied":false,
        "inputs":[{"input_id":"config","role":"active","required":true,"present":false,"protection":"secret","value":"DO_NOT_SHOW_0688"},
        {"input_id":"old","role":"retained","required":null,"present":true,"protection":"ordinary"}]});
    let rendered = show("instance create", &instance);
    assert!(rendered.contains("config") && rendered.contains("retained"));
    assert!(rendered.contains("pactrun input set sample config --file <path>"));
    assert!(!rendered.contains("DO_NOT_SHOW_0688"));
    let input = show(
        "input set",
        &json!({"instance_id":"instance","input_id":"config","state_version":"state"}),
    );
    assert!(input.contains("instance") && input.contains("config") && input.contains("state"));
    let mut plan = json!({"preview":"not_admitted","action_id":"inspect","instance_id":"instance","expected_state_version":"state","access":"observe",
        "revision":{"package_id":"package","content_digest":"sha256:revision"},"required_inputs_satisfied":false,"missing_input_ids":["config"],
        "launch":{"kind":"interpreter","resolved_path":{"encoding":"utf8","value":"/host/with space/launcher"}},"terminal":"none"});
    let rendered = show("invoke", &plan);
    for fact in [
        "config",
        "inspect",
        "instance",
        "package:revision",
        "/host/with space/launcher",
        "no Run created",
    ] {
        assert!(rendered.contains(fact), "{rendered}");
    }
    plan.as_object_mut().unwrap().remove("missing_input_ids");
    let rendered = show("invoke", &plan);
    assert!(!rendered.contains("Satisfied"));
    assert!(rendered.contains("IDs unavailable"));
}

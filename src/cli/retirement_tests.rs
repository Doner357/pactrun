use super::*;
use crate::{domain::*, persistence::PactrunPersistence};

fn fixture() -> (
    tempfile::TempDir,
    PathBuf,
    InstanceView,
    ServiceAllocationId,
    PathBuf,
) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/deletion-cli-tests");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let root = temp.path().join("storage");
    for part in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(part)).unwrap();
    }
    let p = PactrunPersistence::open(&root).unwrap();
    let core = crate::revision_canonical::project_service_revision_source(
        br#"{
        "format_version":"1.0-alpha.1","inputs":[],"actions":[],"migrations":[],
        "service_storages":[{"id":"data"}],"service_resources":[]
    }"#,
    )
    .unwrap();
    let runtime =
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
            .unwrap();
    let content =
        crate::revision_canonical::validate_service_revision_content(core, runtime).unwrap();
    let revision = p
        .persist_versioned_revision_with_metadata(
            PackageId::from_bytes([55; 16]),
            &content.into(),
            &[],
            &RevisionMetadataMutationBatch::new([]).unwrap(),
        )
        .unwrap();
    let instance = p
        .create_instance(InstanceName::parse("retire").unwrap(), revision, &mut [])
        .unwrap();
    let allocation = p
        .load_instance_service_state(instance.id)
        .unwrap()
        .unwrap()
        .storages[0]
        .allocation;
    let path = root
        .join("service-storage")
        .join(format!("alloc-{allocation}"));
    fs::write(path.join("secret"), b"service-owned-private-sentinel").unwrap();
    (temp, root, instance, allocation, path)
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

// Test-ID: PR-TEST-0645
// Verifies: PR-REQ-0359, PR-REQ-0336, PR-REQ-0337
#[cfg(target_os = "linux")]
#[test]
fn permission_diagnostics_survive_failed_run_inspection_without_revealing_service_paths() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, root, _, allocation, path) = fixture();
    fs::create_dir(path.join("private-directory-sentinel")).unwrap();
    let directory = fs::File::open(path.join("private-directory-sentinel")).unwrap();
    directory
        .set_permissions(fs::Permissions::from_mode(0o000))
        .unwrap();
    let (code, out, _) = invoke(&root, &["--format", "json", "instance", "delete", "retire"]);
    assert_eq!(code, 1, "{out}");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    schema_tests::assert_response(&value);
    let run = value["result"]["run"]["run_id"].as_str().unwrap();
    let failure = &value["result"]["run"]["state"]["primary_failure"];
    assert_eq!(failure["reference"]["code"], "allocation_unavailable");
    assert_eq!(failure["detail"], crate::retirement_fs::PERMISSION_DENIED);
    assert_eq!(failure["step"], "finalize_storage");
    let (code, history, _) = invoke(&root, &["--format", "json", "run", "show", run]);
    assert_eq!(code, 0);
    let historical: serde_json::Value = serde_json::from_str(&history).unwrap();
    schema_tests::assert_response(&historical);
    assert!(history.contains(crate::retirement_fs::PERMISSION_DENIED));
    let (code, human, _) = invoke(&root, &["run", "show", run]);
    assert_eq!(code, 0);
    assert!(human.contains(crate::retirement_fs::PERMISSION_DENIED));
    for text in [&out, &history, &human] {
        assert!(!text.contains("private-directory-sentinel"));
        assert!(!text.contains("service-owned-private-sentinel"));
        assert!(!text.contains(&root.to_string_lossy().to_string()));
    }
    let (code, _, _) = invoke(&root, &["instance", "abandon", "retire"]);
    assert_eq!(code, 0);
    let id = allocation.to_string();
    let (code, _, _) = invoke(
        &root,
        &[
            "service-storage",
            "detached",
            "show",
            &id,
            "--reveal-location",
        ],
    );
    assert_eq!(code, 0);
    let (code, _, _) = invoke(
        &root,
        &[
            "service-storage",
            "detached",
            "discard",
            &id,
            "--confirm-discard",
        ],
    );
    assert_eq!(code, 1);
    assert_eq!(
        directory.metadata().unwrap().permissions().mode() & 0o777,
        0
    );
    directory
        .set_permissions(fs::Permissions::from_mode(0o700))
        .unwrap();
    let (code, _, _) = invoke(
        &root,
        &[
            "service-storage",
            "detached",
            "discard",
            &id,
            "--confirm-discard",
        ],
    );
    assert_eq!(code, 0);
}

// Test-ID: PR-TEST-0563
// Verifies: PR-REQ-0359, PR-REQ-0360, PR-REQ-0120
#[test]
fn json_retirement_handoff_and_storage_views_preserve_disclosure_boundaries() {
    let (_temp, root, instance, allocation, path) = fixture();
    let json = |args: &[&str]| {
        let mut all = vec!["--format", "json"];
        all.extend_from_slice(args);
        let (code, out, err) = invoke(&root, &all);
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        schema_tests::assert_response(&value);
        assert_eq!(code, 0, "{value}; {err}");
        assert!(!out.contains("service-owned-private-sentinel"));
        value["result"].clone()
    };
    let storages = json(&["service-storage", "list", "retire"]);
    assert_eq!(storages["items"].as_array().unwrap().len(), 1);
    assert!(
        json(&["resource", "list", "retire"])["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        json(&["service-storage", "detached", "list"])["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let plan = json(&["instance", "abandon", "retire", "--plan"]);
    assert_eq!(plan["work"], "abandon_without_hook");
    assert!(path.exists());
    let result = json(&["instance", "abandon", "retire"]);
    assert_eq!(result["run"]["state"]["outcome"], "succeeded");
    assert!(path.exists());
    let detail = json(&["instance", "deletion", "show", &instance.id.to_string()]);
    let list = json(&["instance", "deletion", "list"]);
    assert_eq!(list["items"][0]["inspection"], detail);
    let allocation_id = allocation.to_string();
    let hidden = json(&["service-storage", "detached", "show", &allocation_id]);
    assert!(hidden["locations"].is_null());
    let shown = json(&[
        "service-storage",
        "detached",
        "show",
        &allocation_id,
        "--reveal-location",
    ]);
    assert!(!shown["locations"].as_array().unwrap().is_empty());
    let discarded = json(&[
        "service-storage",
        "detached",
        "discard",
        &allocation_id,
        "--confirm-discard",
    ]);
    assert_eq!(discarded["new_completion"], true);
    assert!(!path.exists());
}

// Test-ID: PR-TEST-0415
// Verifies: PR-REQ-0335, PR-REQ-0337, PR-REQ-0340
#[test]
fn retirement_parsing_requires_exact_explicit_intent_before_opening_storage() {
    let (_temp, root, _, _, _) = fixture();
    let absent = root.join("not-initialized");
    let id = "00000000000000000000000000000001";
    for args in [
        vec!["service-storage", "detached", "discard", id],
        vec!["service-storage", "detached", "discard", id, "--force"],
        vec![
            "service-storage",
            "detached",
            "discard",
            id,
            "--confirm-discard",
            "--confirm-discard",
        ],
        vec!["service-storage", "detached", "show", id, "--json"],
        vec![
            "instance",
            "deletion",
            "confirm-complete",
            id,
            "--attempt",
            id,
            "--if-version",
            id,
        ],
        vec![
            "instance",
            "deletion",
            "confirm-complete",
            id,
            "--assert-cleanup-complete",
        ],
        vec!["instance", "delete", "retire", "--param", "x=y"],
        vec!["instance", "delete", "retire", "--plan", "--plan"],
    ] {
        let (code, _, _) = invoke(&absent, &args);
        assert_eq!(code, 2, "{args:?}");
        assert!(!absent.exists());
    }
}

// Test-ID: PR-TEST-0416
// Verifies: PR-REQ-0036, PR-REQ-0336, PR-REQ-0338, PR-REQ-0340
#[test]
fn deletion_cli_plans_without_runs_then_removes_storage_and_preserves_exact_history() {
    let (_temp, root, instance, allocation, path) = fixture();
    let (code, out, err) = invoke(&root, &["instance", "delete", "retire", "--plan"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("no_cleanup_declared"));
    assert!(out.contains("Mode: preview"));
    assert!(!out.contains("service-owned-private-sentinel"));
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    assert!(p.list_managed_runs(instance.id).unwrap().is_empty());
    assert!(p.deletion_obligation(instance.id).unwrap().is_none());
    drop(p);
    let (code, _, _) = invoke(
        &root,
        &[
            "service-storage",
            "detached",
            "discard",
            &allocation.to_string(),
            "--confirm-discard",
        ],
    );
    assert_ne!(code, 0);
    assert!(path.join("secret").exists());
    let (code, _, err) = invoke(
        &root,
        &[
            "instance",
            "delete",
            "retire",
            "--if-version",
            &instance.state_version.to_string(),
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(err.contains(&instance.id.to_string()));
    assert!(!path.exists());
    let p = PactrunPersistence::open(&root).unwrap();
    assert!(p.load_instance_by_id(instance.id).unwrap().is_none());
    let replacement = p
        .create_instance(
            instance.name.clone(),
            instance.active_revision.clone(),
            &mut [],
        )
        .unwrap();
    assert_ne!(instance.id, replacement.id);
    drop(p);
    let (code, out, err) = invoke(
        &root,
        &["instance", "deletion", "show", &instance.id.to_string()],
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Managed: No"));
    assert!(out.contains("instance_delete"));
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    assert!(p.load_instance_by_id(replacement.id).unwrap().is_some());
    assert!(p.list_managed_runs(replacement.id).unwrap().is_empty());
}

// Test-ID: PR-TEST-0417
// Verifies: PR-REQ-0181, PR-REQ-0336, PR-REQ-0337, PR-REQ-0340
#[test]
fn abandon_handoff_and_discard_cli_preserve_bytes_until_exact_confirmation() {
    let (_temp, root, instance, allocation, path) = fixture();
    let (code, _, err) = invoke(&root, &["instance", "abandon", "retire"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("preserves remaining service data"));
    assert!(err.contains("Stop external service processes separately"));
    assert_eq!(
        fs::read(path.join("secret")).unwrap(),
        b"service-owned-private-sentinel"
    );
    for args in [
        vec!["service-storage", "detached", "list", "--no-trunc"],
        vec![
            "service-storage",
            "detached",
            "show",
            &allocation.to_string(),
        ],
    ] {
        let (code, out, err) = invoke(&root, &args);
        assert_eq!(code, 0, "{err}");
        assert!(out.contains(&allocation.to_string()));
        assert!(out.contains(&instance.id.to_string()));
        assert!(!out.contains("native_location"));
        assert!(!out.contains("service-owned-private-sentinel"));
        assert!(!out.contains("alloc-"));
    }
    let (code, out, err) = invoke(
        &root,
        &[
            "service-storage",
            "detached",
            "show",
            &allocation.to_string(),
            "--reveal-location",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Native Location"));
    assert!(out.contains(&format!("alloc-{allocation}")));
    let args = [
        "service-storage",
        "detached",
        "discard",
        &allocation.to_string(),
        "--confirm-discard",
    ];
    let (code, out, err) = invoke(&root, &args);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("New Completion: Yes"));
    assert!(!path.exists());
    fs::create_dir(&path).unwrap();
    fs::write(path.join("replacement"), b"must not be removed").unwrap();
    let (code, out, err) = invoke(&root, &args);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("New Completion: No"));
    assert_eq!(
        fs::read(path.join("replacement")).unwrap(),
        b"must not be removed"
    );
}

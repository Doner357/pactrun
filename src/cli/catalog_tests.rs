use super::*;
use crate::persistence::PactrunPersistence;

fn fixture(count: u8) -> (tempfile::TempDir, PathBuf, Vec<RevisionIdentity>) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/catalog-tests");
    fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    let root = temp.path().join("store");
    for child in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(child)).unwrap();
    }
    let p = PactrunPersistence::open(&root).unwrap();
    let core = crate::revision_core_v2::project_revision_core_source_v2(br#"{"format_version":2,"inputs":[],"actions":[],"migrations":[],"service_storages":[],"service_resources":[]}"#).unwrap();
    let runtime =
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
            .unwrap();
    let content = crate::revision_core_v2::validate_revision_content_v2(core, runtime).unwrap();
    let content = content.into();
    let ids = (0..count)
        .map(|tag| {
            p.persist_versioned_revision_with_metadata(
                PackageId::from_bytes([tag; 16]),
                &content,
                &[],
                &RevisionMetadataMutationBatch::new([]).unwrap(),
            )
            .unwrap()
        })
        .collect();
    (temp, root, ids)
}
fn invoke(root: &Path, args: &[&str]) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = crate::cli::run(
        args.iter().map(OsString::from).collect(),
        Some(root.as_os_str().into()),
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
fn ok(root: &Path, args: &[&str]) -> String {
    let (code, out, err) = invoke(root, args);
    assert_eq!(code, 0, "{args:?}: {err}");
    out
}

// Test-ID: PR-TEST-0529
// Verifies: PR-REQ-0353, PR-REQ-0118
#[test]
fn catalog_pages_are_bounded_ordered_and_continue_after_deleted_cursors() {
    let (_temp, root, ids) = fixture(51);
    let first = ok(&root, &["revision", "list", "--no-trunc"]);
    assert!(first.contains("50 records shown."));
    assert!(!first.contains(&ids[50].package_id.to_string()));
    assert!(first.contains(&format!("--after {} --no-trunc", exact_text(&ids[49]))));
    let after = exact_text(&ids[49]);
    ok(&root, &["revision", "delete", &after]);
    let last = ok(
        &root,
        &["revision", "list", "--after", &after, "--no-trunc"],
    );
    assert!(last.contains(&ids[50].package_id.to_string()));
    assert!(last.contains("1 records shown."));
    assert!(!last.contains("More results"));
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    let page = p.catalog_revisions(500, None).unwrap();
    assert!(page.items.windows(2).all(|v| v[0].identity < v[1].identity));
    assert!(p.catalog_revisions(0, None).is_err());
    assert!(p.catalog_revisions(501, None).is_err());
    let short = ok(&root, &["revision", "list", "--limit", "1"]);
    let columns: Vec<_> = short.lines().nth(1).unwrap().split_whitespace().collect();
    assert!(!columns[0].contains("..."));
    assert!(!columns[1].contains("..."));
    let reference = format!("exact:{}/{}", columns[0], columns[1]);
    assert!(ok(&root, &["revision", "show", &reference]).contains("Core format: V2"));
    let show = ok(&root, &["revision", "show", &exact_text(&ids[0])]);
    assert!(show.contains("Core format: V2"));
    assert!(show.contains(&exact_text(&ids[0])));
}

// Test-ID: PR-TEST-0530
// Verifies: PR-REQ-0353, PR-REQ-0253, PR-REQ-0255
#[test]
fn catalog_local_mutations_require_semantic_cas_and_do_not_enforce_trust() {
    let (_temp, root, ids) = fixture(2);
    let a = exact_text(&ids[0]);
    let b = exact_text(&ids[1]);
    ok(
        &root,
        &[
            "revision",
            "alias",
            "set",
            "--literal-alias",
            &a,
            "--expect-absent",
        ],
    );
    assert!(ok(&root, &["revision", "alias", "show", "--literal-alias"]).contains(&a));
    ok(
        &root,
        &[
            "revision",
            "alias",
            "clear",
            "--literal-alias",
            "--expect",
            &a,
        ],
    );
    ok(
        &root,
        &[
            "revision",
            "alias",
            "set",
            "production",
            &a,
            "--expect-absent",
        ],
    );
    ok(
        &root,
        &[
            "revision",
            "alias",
            "set",
            "production",
            &a,
            "--expect-absent",
        ],
    );
    let (code, _, err) = invoke(
        &root,
        &[
            "revision",
            "alias",
            "set",
            "production",
            &b,
            "--expect-absent",
        ],
    );
    assert_eq!(code, 1);
    assert!(err.contains("No changes applied"));
    assert!(err.contains("Inspect:"));
    ok(
        &root,
        &["revision", "alias", "set", "production", &b, "--expect", &a],
    );
    assert!(ok(&root, &["revision", "alias", "show", "production"]).contains(&b));
    ok(
        &root,
        &["revision", "alias", "clear", "production", "--expect", &b],
    );
    ok(
        &root,
        &["revision", "alias", "clear", "production", "--expect", &b],
    );
    let text = "已驗證\nnext\u{1b}[31m";
    ok(
        &root,
        &[
            "revision",
            "note",
            "set",
            &a,
            "--value",
            text,
            "--expect-absent",
        ],
    );
    let shown = ok(&root, &["revision", "note", "show", &a]);
    assert!(shown.contains("已驗證\\n"));
    assert!(!shown.contains('\u{1b}'));
    assert_eq!(
        invoke(
            &root,
            &["revision", "note", "clear", &a, "--expect", "wrong"]
        )
        .0,
        1
    );
    ok(&root, &["revision", "note", "clear", &a, "--expect", text]);
    assert!(ok(&root, &["revision", "note", "show", &a]).contains("Absent"));
    ok(
        &root,
        &[
            "revision",
            "trust",
            "set",
            &a,
            "distrusted",
            "--expect-absent",
        ],
    );
    ok(&root, &["instance", "create", "allowed", "--revision", &a]);
    assert!(ok(&root, &["revision", "trust", "show", &a]).contains("Distrusted"));
    ok(
        &root,
        &["revision", "trust", "clear", &a, "--expect", "distrusted"],
    );
    assert!(ok(&root, &["revision", "trust", "show", &a]).contains("NoDecision"));
}

// Test-ID: PR-TEST-0531
// Verifies: PR-REQ-0353, PR-REQ-0088, PR-REQ-0024
#[test]
fn catalog_history_survives_retirement_and_name_reuse_without_rebinding_runs() {
    let (_temp, root, ids) = fixture(1);
    let revision = exact_text(&ids[0]);
    let p = PactrunPersistence::open(&root).unwrap();
    let old = p
        .create_instance(
            InstanceName::parse("same name; literal").unwrap(),
            ids[0].clone(),
            &mut [],
        )
        .unwrap();
    drop(p);
    ok(&root, &["instance", "delete", "same name; literal"]);
    let old_runs = ok(
        &root,
        &[
            "run",
            "list",
            "--instance-id",
            &old.id.to_string(),
            "--no-trunc",
        ],
    );
    assert!(old_runs.contains(&old.id.to_string()));
    ok(
        &root,
        &[
            "instance",
            "create",
            "same name; literal",
            "--revision",
            &revision,
        ],
    );
    let new_runs = ok(&root, &["run", "list", "same name; literal"]);
    assert!(new_runs.contains("0 records shown."));
    let history = ok(&root, &["instance", "history", "list", "--no-trunc"]);
    assert!(history.contains("Retired"));
    assert!(history.contains("Managed"));
    assert!(history.contains(&old.id.to_string()));
    let deletions = ok(&root, &["instance", "deletion", "list", "--no-trunc"]);
    assert!(deletions.contains("1 records shown."));
    assert!(deletions.contains(&old.id.to_string()));
    let unknown = InstanceId::from_bytes([254; 16]).to_string();
    assert_eq!(
        invoke(&root, &["run", "list", "--instance-id", &unknown]).0,
        1
    );
    assert!(
        ok(&root, &["instance", "history", "show", &old.id.to_string()]).contains("Runs: pactrun")
    );
}

// Test-ID: PR-TEST-0532
// Verifies: PR-REQ-0353, PR-REQ-0089, PR-REQ-0019
#[test]
fn catalog_metadata_preserves_claim_sources_and_refuses_ambiguous_resolution() {
    let (_temp, root, ids) = fixture(2);
    let p = PactrunPersistence::open(&root).unwrap();
    let label = ReferenceLabel::parse("stable").unwrap();
    for revision in &ids {
        p.apply_revision_metadata_batch(
            revision,
            &RevisionMetadataMutationBatch::new([
                RevisionMetadataMutation::AddReferenceLabel(ReferenceLabelBinding {
                    revision: revision.clone(),
                    label: label.clone(),
                    source: ReferenceLabelSource::Unattributed,
                }),
                RevisionMetadataMutation::AddReferenceLabel(ReferenceLabelBinding {
                    revision: revision.clone(),
                    label: label.clone(),
                    source: ReferenceLabelSource::PublisherSourceUri {
                        name: PublisherName::parse("team").unwrap(),
                        namespace: Some(PublisherNamespace::parse("tools").unwrap()),
                        source_uri: SourceUri::parse("https://example.invalid/releases").unwrap(),
                    },
                }),
            ])
            .unwrap(),
        )
        .unwrap();
    }
    drop(p);
    let shown = ok(
        &root,
        &["revision", "metadata", "show", &exact_text(&ids[0])],
    );
    assert!(shown.contains("Unattributed"));
    assert!(shown.contains("PublisherSourceUri"));
    assert!(shown.contains("tools"));
    assert!(shown.contains("https://example.invalid/releases"));
    assert!(shown.contains("Metadata"));
    assert!(!shown.contains("claims are not authentication"));
    let (code, _, err) = invoke(&root, &["revision", "show", "label:stable"]);
    assert_eq!(code, 1);
    assert!(err.contains("resolution.ambiguous_reference"));
    let p = PactrunPersistence::open(&root).unwrap();
    for (label, revision) in [("é", &ids[0]), ("e\u{301}", &ids[1])] {
        p.apply_revision_metadata_batch(
            revision,
            &RevisionMetadataMutationBatch::new([RevisionMetadataMutation::AddReferenceLabel(
                ReferenceLabelBinding {
                    revision: revision.clone(),
                    label: ReferenceLabel::parse(label).unwrap(),
                    source: ReferenceLabelSource::Unattributed,
                },
            )])
            .unwrap(),
        )
        .unwrap();
        let shown = ok(&root, &["revision", "show", &format!("label:{label}")]);
        assert!(shown.contains(&exact_text(revision)));
    }
    assert_eq!(invoke(&root, &["revision", "show", "label:Stable"]).0, 1);
    assert_eq!(invoke(&root, &["revision", "show", "label:stable "]).0, 1);
}

// Test-ID: PR-TEST-0537
// Verifies: PR-REQ-0353
#[test]
fn catalog_reports_actual_core_version_and_distinguishes_present_absence_text() {
    let (_temp, root, _) = fixture(0);
    let p = PactrunPersistence::open(&root).unwrap();
    let core = crate::revision_core_v3::project(br#"{"format_version":3,"inputs":[],"actions":[],"migrations":[],"service_storages":[],"service_resources":[]}"#).unwrap();
    let runtime =
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
            .unwrap();
    let content = crate::revision_core_v3::validate(core, runtime).unwrap();
    let revision = p
        .persist_versioned_revision_with_metadata(
            PackageId::from_bytes([73; 16]),
            &content,
            &[],
            &RevisionMetadataMutationBatch::new([]).unwrap(),
        )
        .unwrap();
    drop(p);
    let exact = exact_text(&revision);
    assert!(ok(&root, &["revision", "show", &exact]).contains("Core format: V3"));
    ok(
        &root,
        &[
            "revision",
            "note",
            "set",
            &exact,
            "--value",
            "Absent",
            "--expect-absent",
        ],
    );
    assert!(ok(&root, &["revision", "note", "show", &exact]).contains("Local note: \"Absent\""));
    ok(
        &root,
        &["revision", "note", "clear", &exact, "--expect", "Absent"],
    );
    assert!(ok(&root, &["revision", "note", "show", &exact]).contains("Local note: Absent\n"));
}

// Test-ID: PR-TEST-0533
// Verifies: PR-REQ-0353
#[test]
fn catalog_rejects_invalid_options_before_storage_and_queries_do_not_bootstrap() {
    let (_temp, root, ids) = fixture(1);
    let absent = root.join("absent");
    let revision = exact_text(&ids[0]);
    for args in [
        vec!["revision", "list", "--limit", "0"],
        vec!["revision", "list", "--limit", "501"],
        vec!["revision", "list", "--no-trunc", "--no-trunc"],
        vec!["revision", "list", "--after", "alias:x"],
        vec!["revision", "show", "exact:123/sha256:123"],
        vec![
            "revision",
            "note",
            "set",
            &revision,
            "--value",
            "",
            "--expect-absent",
        ],
        vec!["revision", "trust", "set", &revision, "trusted"],
        vec!["revision", "alias", "clear", "x", "--expect-absent"],
        vec![
            "run",
            "list",
            "name",
            "--instance-id",
            "00000000000000000000000000000001",
        ],
    ] {
        assert_eq!(invoke(&absent, &args).0, 2, "{args:?}");
    }
    for args in [
        vec!["revision", "list"],
        vec!["run", "list"],
        vec!["instance", "history", "list"],
        vec!["instance", "deletion", "list"],
    ] {
        assert_eq!(invoke(&absent, &args).0, 1);
        assert!(!absent.exists());
        ok(&root, &args);
    }
    let db = rusqlite::Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    let count: i64 = db
        .query_row("SELECT count(*) FROM runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    let version: i64 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 11);
}

// Test-ID: PR-TEST-0534
// Verifies: PR-REQ-0353
#[test]
fn catalog_output_failure_is_read_only_and_control_text_is_escaped() {
    let (_temp, root, _) = fixture(1);
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let command = CatalogCommand::Revisions(PageOptions {
        limit: 50,
        after: None,
        no_trunc: false,
    });
    assert!(execute(command, &root, &mut Broken, presentation::Format::Human).is_err());
    assert!(safe("中文\u{1b}[2J\r\n\u{202e}").starts_with("中文\\u{1b}"));
    assert_ne!(safe("\\n"), safe("\n"));
    assert_eq!(quoted("Absent"), "\"Absent\"");
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    assert!(
        p.catalog_runs(&CatalogRunSelector::All, 50, None)
            .unwrap()
            .items
            .is_empty()
    );
}

// Test-ID: PR-TEST-0536
// Verifies: PR-REQ-0353, PR-REQ-0255
#[test]
fn catalog_concurrent_cas_has_one_winner_and_snapshot_queries_reject_corruption() {
    let (_temp, root, ids) = fixture(1);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|value| {
            let root = root.clone();
            let revision = ids[0].clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let p = PactrunPersistence::open(&root).unwrap();
                let batch = RevisionMetadataMutationBatch::new([
                    RevisionMetadataMutation::CompareAndSetLocalNote {
                        expected: CurrentState::Absent,
                        desired: CurrentState::Present(LocalNote::parse(value).unwrap()),
                    },
                ])
                .unwrap();
                barrier.wait();
                p.apply_revision_metadata_batch(&revision, &batch)
            })
        })
        .collect();
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(
                r,
                Err(crate::persistence::PersistenceError::MetadataConflict(_))
            ))
            .count(),
        1
    );
    let db = rusqlite::Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    db.execute(
        "UPDATE revision_local_current_notes SET note_utf8=?1",
        [vec![0xffu8]],
    )
    .unwrap();
    assert_eq!(
        invoke(
            &root,
            &["revision", "metadata", "show", &exact_text(&ids[0])]
        )
        .0,
        1
    );
    assert_eq!(invoke(&root, &["revision", "list"]).0, 1);
}

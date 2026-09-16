//! Fresh-process coverage of the offline human retirement surface. All data is
//! created beneath this repository's dedicated test directory.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Fixture {
    _temp: tempfile::TempDir,
    store: PathBuf,
    revision: String,
    instance: String,
    allocation: String,
    data: PathBuf,
}
fn call(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pactrun"))
        .args(args)
        .env("PACTRUN_STORAGE_ROOT", root)
        .output()
        .unwrap()
}
fn success(root: &Path, args: &[&str]) -> String {
    let result = call(root, args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}
fn database(root: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        root.join("database/pactrun.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
}
fn fixture() -> Fixture {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m7-process-cli-tests");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let store = temp.path().join("store");
    for path in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(store.join(path)).unwrap();
    }
    let source = temp.path().join("pack");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("pactrun.yaml"),"source_format: 2\npackage_id: 00000000000000000000000000000097\nrevision:\n  service_storages: [{id: data}]\nruntime_content: {}\n").unwrap();
    let revision = success(&store, &["pack", "install", source.to_str().unwrap()])
        .lines()
        .next()
        .unwrap()
        .to_owned();
    success(
        &store,
        &["instance", "create", "sample", "--revision", &revision],
    );
    let db = database(&store);
    let (instance, allocation): (Vec<u8>, Vec<u8>) = db
        .query_row(
            "SELECT instance_id,allocation_id FROM instance_service_storages",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let instance = hex::encode(instance);
    let allocation = hex::encode(allocation);
    let data = store
        .join("service-storage")
        .join(format!("alloc-{allocation}"));
    fs::write(data.join("private-data"), b"service-owned private bytes").unwrap();
    Fixture {
        _temp: temp,
        store,
        revision,
        instance,
        allocation,
        data,
    }
}

// Test-ID: PR-TEST-0436
// Verifies: PR-REQ-0036, PR-REQ-0336, PR-REQ-0338, PR-REQ-0340
#[test]
fn actual_cli_plan_delete_and_old_identity_inspection_preserve_name_reuse_boundary() {
    let f = fixture();
    let plan = success(&f.store, &["instance", "delete", "sample", "--plan"]);
    assert!(plan.contains("admission: not_attempted"));
    assert_eq!(
        database(&f.store)
            .query_row("SELECT count(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(f.data.join("private-data")).unwrap(),
        b"service-owned private bytes"
    );
    let deleted = success(&f.store, &["instance", "delete", "sample"]);
    assert!(deleted.contains(&format!("retired_instance: {}", f.instance)));
    assert!(!f.data.exists());
    success(
        &f.store,
        &["instance", "create", "sample", "--revision", &f.revision],
    );
    let old = success(&f.store, &["instance", "deletion", "show", &f.instance]);
    assert!(old.contains("managed: false"));
    assert!(old.contains("instance_delete"));
    let current: Vec<u8> = database(&f.store)
        .query_row("SELECT instance_id FROM instances", [], |r| r.get(0))
        .unwrap();
    assert_ne!(hex::encode(current), f.instance);
    assert_eq!(
        database(&f.store)
            .query_row("SELECT count(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

// Test-ID: PR-TEST-0437
// Verifies: PR-REQ-0181, PR-REQ-0337, PR-REQ-0340
#[test]
fn actual_cli_abandon_handoff_and_confirmed_discard_do_not_infer_destruction() {
    let f = fixture();
    success(&f.store, &["instance", "abandon", "sample"]);
    let listing = success(
        &f.store,
        &["service-storage", "detached", "show", &f.allocation],
    );
    assert!(listing.contains("state: preserved"));
    assert!(!listing.contains("native_location"));
    assert!(!listing.contains("service-owned private bytes"));
    let handoff = success(
        &f.store,
        &[
            "service-storage",
            "detached",
            "show",
            &f.allocation,
            "--reveal-location",
        ],
    );
    assert!(handoff.contains("native_location:"));
    assert!(handoff.contains(&format!("alloc-{}", f.allocation)));
    assert_eq!(
        call(
            &f.store,
            &["service-storage", "detached", "discard", &f.allocation]
        )
        .status
        .code(),
        Some(2)
    );
    assert_eq!(
        database(&f.store)
            .query_row(
                "SELECT count(*) FROM allocation_discard_receipts",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(f.data.join("private-data")).unwrap(),
        b"service-owned private bytes"
    );
    let args = [
        "service-storage",
        "detached",
        "discard",
        &f.allocation,
        "--confirm-discard",
    ];
    assert!(success(&f.store, &args).contains("new_completion: true"));
    assert!(!f.data.exists());
    fs::create_dir(&f.data).unwrap();
    fs::write(f.data.join("replacement"), b"new unrelated bytes").unwrap();
    assert!(success(&f.store, &args).contains("new_completion: false"));
    assert_eq!(
        fs::read(f.data.join("replacement")).unwrap(),
        b"new unrelated bytes"
    );
}

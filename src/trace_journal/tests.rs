use super::*;
use crate::domain::{PhaseResult, TraceOperation, TracePhase, TracePolicy, TraceWindow};

fn scratch() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/alpha8-trace-tests");
    fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}
fn record(id: u8, full: bool) -> TraceDocument {
    let mut window = TraceWindow::new(
        TraceId::from_bytes([id; 16]),
        TraceOperation::Catalog,
        Some(1),
        TracePolicy {
            detailed: true,
            max_bytes: 1_048_576,
            ..TracePolicy::startup()
        },
    );
    for i in 0..if full { 1500 } else { 1 } {
        let span = window.begin(TracePhase::Command, i * 20).unwrap();
        window.end(
            span,
            TracePhase::Command,
            i * 20 + 10,
            10,
            PhaseResult::Succeeded,
        );
    }
    window.close(0);
    window.snapshot()
}

// Test-ID: PR-TEST-0808
// Verifies: PR-REQ-0398
#[test]
fn journal_expiry_candidates_preserve_cutoff_order_and_bounded_batches() {
    let temp = scratch();
    let location = Location::startup(temp.path().join("diagnostics.sqlite3"));
    let mut journal = Journal::open(&location).unwrap();
    let tx = journal.connection.transaction().unwrap();
    for n in 1u32..151 {
        let mut doc = record(1, false);
        let mut id = [0; 16];
        id[..4].copy_from_slice(&n.to_be_bytes());
        doc.id = TraceId::from_bytes(id);
        tx.execute(
            "INSERT INTO records VALUES(?1,NULL,?2,?3)",
            params![
                id.as_slice(),
                i64::from(n / 3),
                serde_json::to_vec(&doc).unwrap(),
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    assert!(journal.expired_retention_candidates(0).unwrap().is_empty());
    assert!(journal.expired_retention_candidates(u64::MAX).is_err());
    let mut removed = 0;
    loop {
        let expected = journal.retention_inventory(Some(40), None).unwrap().1;
        let actual = journal.expired_retention_candidates(40).unwrap();
        assert_eq!(actual, expected);
        assert!(actual.len() <= 64);
        assert!(
            actual
                .iter()
                .all(|candidate| candidate.updated_at_ms < 40 && !candidate.preferred)
        );
        if actual.is_empty() {
            break;
        }
        removed += actual.len();
        journal.delete_retention_candidates(&actual).unwrap();
    }
    assert_eq!(removed, 119);
    assert_eq!(
        journal
            .connection
            .query_row("SELECT count(*) FROM records", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        31
    );
}

// Test-ID: PR-TEST-0756
// Verifies: PR-REQ-0398, PR-REQ-0399
#[test]
fn diagnostic_journal_readers_never_provision_and_foreign_files_are_unchanged() {
    let temp = scratch();
    let mut deep = temp.path().join("too-deep");
    for _ in 0..300 {
        deep.push("a");
    }
    assert!(Journal::open(&Location::startup(deep.join("log.sqlite3"))).is_err());
    assert!(!temp.path().join("too-deep").exists());
    let location = Location::startup(temp.path().join("missing/log.sqlite3"));
    assert!(
        read(&location, TraceId::from_bytes([1; 16]))
            .unwrap()
            .is_none()
    );
    assert!(list(&location, 10).unwrap().items.is_empty());
    assert!(!temp.path().join("missing").exists());
    assert!(matches!(
        Journal::open(&Location::store(&temp.path().join("absent-store"))),
        Err(Error::Missing)
    ));
    assert!(!temp.path().join("absent-store").exists());
    let foreign = temp.path().join("foreign.sqlite3");
    let db = Connection::open(&foreign).unwrap();
    db.execute_batch("CREATE TABLE keep(value TEXT); INSERT INTO keep VALUES('sentinel');")
        .unwrap();
    drop(db);
    let original = fs::read(&foreign).unwrap();
    assert!(matches!(
        Journal::open(&Location::startup(foreign.clone())),
        Err(Error::ForeignJournal)
    ));
    assert_eq!(fs::read(&foreign).unwrap(), original);
    let empty = temp.path().join("empty.sqlite3");
    fs::write(&empty, []).unwrap();
    assert!(matches!(
        Journal::open(&Location::startup(empty.clone())),
        Err(Error::ForeignJournal)
    ));
    assert_eq!(fs::metadata(empty).unwrap().len(), 0);
}

// Test-ID: PR-TEST-0757
// Verifies: PR-REQ-0398
#[test]
fn diagnostic_journal_retention_is_bounded_and_keeps_current_update_without_managed_data() {
    let temp = scratch();
    let location = Location::startup(temp.path().join("logs/diagnostics.sqlite3"));
    let mut journal = Journal::open(&location).unwrap();
    let now = 100 * 86_400_000;
    journal.save(&record(1, false), 1).unwrap();
    for n in 2..18 {
        journal.save(&record(n, true), now + u64::from(n)).unwrap();
    }
    assert!(
        read(&location, TraceId::from_bytes([1; 16]))
            .unwrap()
            .is_none()
    );
    assert!(
        read(&location, TraceId::from_bytes([17; 16]))
            .unwrap()
            .is_some()
    );
    let retained: i64 = journal
        .connection
        .query_row("SELECT sum(length(document)) FROM records", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(retained <= 1_048_576);
    let items = list(&location, 1).unwrap();
    assert_eq!(items.items.len(), 1);
    assert!(items.has_more);
    assert_eq!(items.items[0].id, TraceId::from_bytes([17; 16]));
    assert!(!temp.path().join("database/pactrun.sqlite3").exists());
    let run = RunId::from_bytes([20; 16]);
    let mut associated = record(30, false);
    associated.run_id = Some(run);
    journal.save(&associated, now + 100).unwrap();
    assert_eq!(read_run(&location, run).unwrap().unwrap(), associated);
}

// Supporting coverage for PR-TEST-0757: policy shrink uses bounded victim batches.
#[test]
fn diagnostic_journal_capacity_shrink_handles_more_than_one_victim_batch() {
    let temp = scratch();
    let location = Location::startup(temp.path().join("diagnostics.sqlite3"));
    let mut journal = Journal::open(&location).unwrap();
    let tx = journal.connection.transaction().unwrap();
    for n in 1u32..2001 {
        let mut doc = record(1, false);
        let mut id = [0; 16];
        id[..4].copy_from_slice(&n.to_be_bytes());
        doc.id = TraceId::from_bytes(id);
        doc.policy.max_bytes = 64 * 1024 * 1024;
        tx.execute(
            "INSERT INTO records VALUES(?1,NULL,100,?2)",
            params![
                doc.id.as_bytes().as_slice(),
                serde_json::to_vec(&doc).unwrap()
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let before: i64 = journal
        .connection
        .query_row("SELECT SUM(length(document)) FROM records", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(before > 1_048_576);
    journal.save(&record(255, false), 101).unwrap();
    let (count, bytes): (i64, i64) = journal
        .connection
        .query_row(
            "SELECT count(*),SUM(length(document)) FROM records",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(count < 2001 - 64 && bytes <= 1_048_576);
    assert!(
        read(&location, TraceId::from_bytes([255; 16]))
            .unwrap()
            .is_some()
    );
}

// Test-ID: PR-TEST-0758
// Verifies: PR-REQ-0398
#[test]
fn diagnostic_journal_refuses_unknown_schema_oversized_records_and_hard_links() {
    let temp = scratch();
    let location = Location::startup(temp.path().join("diagnostics.sqlite3"));
    let mut journal = Journal::open(&location).unwrap();
    journal.save(&record(1, false), 1).unwrap();
    journal
        .connection
        .execute_batch("PRAGMA ignore_check_constraints=ON")
        .unwrap();
    journal
        .connection
        .execute(
            "UPDATE records SET document=zeroblob(?1)",
            [TRACE_DOCUMENT_LIMIT as i64 + 1],
        )
        .unwrap();
    assert!(matches!(
        read(&location, TraceId::from_bytes([1; 16])),
        Err(Error::InvalidRecord)
    ));
    assert!(matches!(list(&location, 10), Err(Error::InvalidRecord)));
    journal
        .connection
        .execute_batch("CREATE TABLE unknown(value TEXT)")
        .unwrap();
    drop(journal);
    let before = fs::read(&location.path).unwrap();
    assert!(matches!(
        Journal::open(&location),
        Err(Error::InvalidJournal)
    ));
    assert_eq!(fs::read(&location.path).unwrap(), before);
    let alias = temp.path().join("alias.sqlite3");
    fs::hard_link(&location.path, &alias).unwrap();
    assert!(matches!(
        Journal::open(&Location::startup(alias)),
        Err(Error::UnsafePath)
    ));
}

// Test-ID: PR-TEST-0759
// Verifies: PR-REQ-0398, PR-REQ-0399
#[cfg(unix)]
#[test]
fn diagnostic_journal_does_not_follow_symlinks_or_block_on_special_files() {
    let temp = scratch();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let link = temp.path().join("link");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    assert!(matches!(
        Journal::open(&Location::startup(link.join("log.sqlite3"))),
        Err(Error::UnsafePath)
    ));
    assert!(!outside.join("log.sqlite3").exists());
    let fifo = temp.path().join("fifo");
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    assert!(matches!(
        Journal::open(&Location::startup(fifo)),
        Err(Error::UnsafePath)
    ));
}

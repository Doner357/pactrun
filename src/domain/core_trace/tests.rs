use super::*;

// Test-ID: PR-TEST-0753
// Verifies: PR-REQ-0397
#[test]
fn core_trace_windows_keep_prefix_tail_and_aggregates_without_unbounded_history() {
    let mut window = TraceWindow::new(
        TraceId::from_bytes([1; 16]),
        TraceOperation::Retirement,
        Some(10),
        TracePolicy {
            detailed: true,
            ..TracePolicy::default()
        },
    );
    for n in 0..10_000 {
        let span = window
            .begin(TracePhase::DurableRecordWrite, n * 20)
            .unwrap();
        window.end(
            span,
            TracePhase::DurableRecordWrite,
            n * 20 + 10,
            10,
            PhaseResult::Succeeded,
        );
    }
    window.close(0);
    let record = window.snapshot();
    assert!(record.valid());
    assert_eq!(record.events.len(), TRACE_EVENT_LIMIT);
    assert_eq!(record.observed, 20_000);
    assert_eq!(record.omitted, 20_000 - TRACE_EVENT_LIMIT as u64);
    assert_eq!(record.events[0].sequence, 1);
    assert_eq!(record.events[255].sequence, 256);
    assert_eq!(record.events.last().unwrap().sequence, 20_000);
    let totals = &record.totals[&TracePhase::DurableRecordWrite];
    assert_eq!(
        (totals.started, totals.finished, totals.elapsed_us),
        (10_000, 10_000, 100_000)
    );
    let encoded = serde_json::to_vec(&record).unwrap();
    assert!(encoded.len() < TRACE_DOCUMENT_LIMIT);
    assert_eq!(
        serde_json::from_slice::<TraceDocument>(&encoded).unwrap(),
        record
    );
    assert!(window.begin(TracePhase::Command, 9).is_none());
}

// Test-ID: PR-TEST-0754
// Verifies: PR-REQ-0397
#[test]
fn core_trace_normal_detail_retains_totals_and_unknown_is_not_success() {
    let mut window = TraceWindow::new(
        TraceId::from_bytes([2; 16]),
        TraceOperation::Unknown,
        None,
        TracePolicy::default(),
    );
    let span = window.begin(TracePhase::DirectorySync, 0).unwrap();
    window.end(
        span,
        TracePhase::DirectorySync,
        4,
        4,
        PhaseResult::Succeeded,
    );
    let span = window.begin(TracePhase::Command, 4).unwrap();
    window.end(span, TracePhase::Command, 8, 4, PhaseResult::Unfinished);
    let record = window.snapshot();
    assert!(record.valid());
    assert_eq!(record.observed, 2);
    assert_eq!(record.totals[&TracePhase::DirectorySync].finished, 1);
    assert_eq!(record.totals[&TracePhase::Command].unfinished, 1);
    assert!(!record.closed && record.exit_code.is_none());
    window.associate(crate::domain::RunId::from_bytes([3; 16]));
    window.associate(crate::domain::RunId::from_bytes([4; 16]));
    assert!(window.snapshot().association_conflict);
    assert_eq!(
        window.snapshot().run_id.unwrap(),
        crate::domain::RunId::from_bytes([3; 16])
    );
}

// Test-ID: PR-TEST-0755
// Verifies: PR-REQ-0397, PR-REQ-0398
#[test]
fn core_trace_decoding_is_closed_and_refuses_invalid_counters_and_states() {
    let mut window = TraceWindow::new(
        TraceId::from_bytes([5; 16]),
        TraceOperation::Catalog,
        Some(10),
        TracePolicy::default(),
    );
    let span = window.begin(TracePhase::Command, 0).unwrap();
    window.end(span, TracePhase::Command, 1, 1, PhaseResult::Failed);
    window.close(1);
    let record = window.snapshot();
    for (key, value) in [
        ("path", serde_json::json!("private-value")),
        ("observed", serde_json::json!(2)),
        ("observed", serde_json::json!("02")),
        ("format_version", serde_json::json!("unknown")),
    ] {
        let mut data = serde_json::to_value(&record).unwrap();
        data[key] = value;
        assert!(serde_json::from_value::<TraceDocument>(data).is_err());
    }
    let mut corrupt = record.clone();
    corrupt.events[1].sequence = 1;
    assert!(!corrupt.valid());
    let mut corrupt = record.clone();
    corrupt.closed = false;
    assert!(!corrupt.valid());
    let mut corrupt = record.clone();
    corrupt.omitted = 99;
    assert!(!corrupt.valid());
    let mut corrupt = record.clone();
    corrupt.policy.max_bytes = 0;
    assert!(!corrupt.valid());
    let mut corrupt = record;
    corrupt.events[0].elapsed_us = Some(0);
    assert!(!corrupt.valid());
    assert!("F".repeat(32).parse::<TraceId>().is_err());
    assert!("0".repeat(31).parse::<TraceId>().is_err());
}

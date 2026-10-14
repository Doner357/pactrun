use super::*;

// Test-ID: PR-TEST-0765
// Verifies: PR-REQ-0400
#[test]
fn service_presence_facts_are_disclosure_safe_and_context_qualified() {
    let mut cause = ServicePresenceFailure {
        reference: ServiceReferenceV2 {
            view: ServiceView::Current,
            role: ServiceRole::Active,
            scope: ServiceScope::Resource(ServiceResourceIdentity::parse("uploads").unwrap()),
        },
        expected: ServicePresenceRequirement::Present,
        observed: ServicePresenceRequirement::Absent,
        edge: None,
    };
    for kind in [
        ManagedExecutionKind::Action,
        ManagedExecutionKind::SnapshotCapture,
        ManagedExecutionKind::SnapshotRestore,
        ManagedExecutionKind::Deletion,
    ] {
        assert!(cause.valid_for(kind));
    }
    assert!(!cause.valid_for(ManagedExecutionKind::Migration));
    cause.observed = cause.expected;
    assert!(!cause.valid_for(ManagedExecutionKind::Action));
    cause.observed = ServicePresenceRequirement::Absent;
    cause.reference.role = ServiceRole::Retained;
    assert!(cause.valid_for(ManagedExecutionKind::Deletion));
    assert!(!cause.valid_for(ManagedExecutionKind::Action));
    cause.reference.view = ServiceView::Source;
    cause.edge = Some(MigrationEdgeIdentity {
        source: RevisionContentDigest::from_bytes([1; 32]),
        target: RevisionContentDigest::from_bytes([2; 32]),
    });
    assert!(cause.valid_for(ManagedExecutionKind::Migration));
    cause.reference.view = ServiceView::Target;
    assert!(!cause.valid_for(ManagedExecutionKind::Migration));
    cause.reference.role = ServiceRole::Active;
    assert!(cause.valid_for(ManagedExecutionKind::Migration));
    cause.edge.as_mut().unwrap().target = cause.edge.as_ref().unwrap().source;
    assert!(!cause.valid_for(ManagedExecutionKind::Migration));
}

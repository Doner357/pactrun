use super::*;
use crate::domain::*;
use crate::hook::{FailureKind, OutcomeWinner, outcome_and_failure};

// Test-ID: PR-TEST-0769
// Verifies: PR-REQ-0400
#[test]
fn service_presence_action_distinguishes_missing_and_unexpected_without_launch() {
    for expected in ["present", "absent"] {
        let fixture = RuntimeFixture::with_source(|_, _, manifest| {
            *manifest = manifest.replace("revision:\n", "revision:\n  service_storages: [{id: data}]\n  service_resources: [{id: uploads, storage_id: data, locator: private-native-locator.txt, kind: file}]\n")
                .replace("        protocol_version: 1.0-alpha.1\n", &format!("        protocol_version: 1.0-alpha.1\n        service_requires: [{{reference: {{view: current, role: active, kind: resource, id: uploads}}, presence: {expected}}}]\n"));
        });
        let p = persistence(&fixture.storage);
        let allocation = p
            .load_instance_service_state(fixture.instance.id)
            .unwrap()
            .unwrap()
            .storages[0]
            .allocation;
        let path = fixture
            .storage
            .join("service-storage")
            .join(format!("alloc-{allocation}"))
            .join("private-native-locator.txt");
        if expected == "absent" {
            fs::write(&path, b"service bytes are not diagnostics").unwrap();
        }
        let marker = fixture.marker("presence-not-launched");
        let admitted = fixture.admit("direct", "success", &marker);
        let run = admitted.run();
        fixture.application.execute_admitted_action(
            admitted,
            policy(None, None, None),
            ActionCancellation::default(),
        );
        assert!(fixture.application.advance_owner_continuation(run).unwrap());
        let view = p.load_managed_run(run).unwrap().unwrap();
        let RunState::Finished(finished) = view.state else {
            panic!("Run not finalized")
        };
        assert_eq!(finished.outcome, RunOutcome::Failed);
        assert_eq!(finished.terminal_risk, RecoveryRiskState::Clear);
        let cause = finished.primary_failure.unwrap().cause.unwrap();
        let RunFailureCause::ServicePresenceRequirement(cause) = cause else {
            panic!("missing resource fact")
        };
        assert_eq!(
            cause.expected == ServicePresenceRequirement::Present,
            expected == "present"
        );
        assert_eq!(
            cause.observed == ServicePresenceRequirement::Present,
            expected == "absent"
        );
        assert_eq!(
            cause.reference.scope,
            ServiceScope::Resource(ServiceResourceIdentity::parse("uploads").unwrap())
        );
        assert!(!marker.exists());
        assert!(!format!("{cause:?}").contains("private-native-locator"));
    }
    let error = crate::hook::materialize::MaterializationError::Service(
        crate::hook::service_storage::NativeServiceError::Access(ServiceAccessError::Unknown(
            ServiceObservationCause::PermissionDenied,
        )),
    );
    let (_, primary) =
        outcome_and_failure(OutcomeWinner::Failed(FailureKind::materialization(&error)));
    let primary = primary.unwrap();
    assert_eq!(primary.failure.error.code(), "observation_unknown");
    assert!(
        primary.cause.is_none(),
        "permission failure was invented as absence"
    );
}

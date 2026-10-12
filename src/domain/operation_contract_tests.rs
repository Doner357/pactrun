use super::*;
use serde_json::{Value, json};

fn identity(n: u8) -> RevisionIdentity {
    RevisionIdentity {
        package_id: PackageId::from_bytes([1; 16]),
        content_digest: RevisionContentDigest::from_bytes([n; 32]),
    }
}
fn hook() -> Value {
    json!({"launch":{"kind":"direct","executable":"tool"},"args":[],"io":{"terminal":"none"}})
}
fn revision(n: u8, value: Value) -> MigrationRevision {
    let source = json!({"source_format":OPERATION_SOURCE_FORMAT,"package_id":PackageId::from_bytes([1;16]).to_string(),"revision":value,"runtime_content":{}});
    let core = crate::authoring::operation::parse(&serde_json::to_vec(&source).unwrap())
        .unwrap()
        .revision;
    let runtime = project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
        files: vec![RuntimeFileV1 {
            id: ContentId::parse("tool").unwrap(),
            path: RuntimePath::parse("bin/tool").unwrap(),
            kind: RuntimeFileKindV1::RegularFile,
            blob_digest: Sha256Digest::from_bytes([5; 32]),
            executable: true,
        }],
    })
    .unwrap();
    MigrationRevision {
        identity: identity(n),
        content: crate::revision_content::validate_revision_content(core.into(), runtime).unwrap(),
    }
}
fn chain() -> (Vec<MigrationRevision>, TransitionRevision) {
    let edge = |from: u8, parameter: Value| {
        json!({
            "source_revision_digest":identity(from).content_digest.to_string(), "transitions":[],"produces_target":[],
            "source_inputs":{"optional":[{"role":"active","input_id":"config"}]},
            "target_inputs":{"optional":["config"]},"parameters":[parameter],"hook":hook(),
        })
    };
    let revisions = vec![
        revision(1, json!({"inputs":[{"id":"config"}]})),
        revision(
            2,
            json!({"inputs":[{"id":"config"}],"migrations":[edge(1,json!({"id":"mode","type":"boolean","sensitive":false,"default":true}))]}),
        ),
        revision(
            3,
            json!({"inputs":[{"id":"config"}],"migrations":[edge(2,json!({"id":"mode","type":"string","sensitive":true}))]}),
        ),
    ];
    let intent = TransitionRevision {
        instance: InstanceId::from_bytes([1; 16]),
        expected_state_version: InstanceStateVersion::from_bytes([2; 16]),
        source: identity(1),
        target: identity(3),
        path: MigrationPathSelection::Exact(vec![identity(1), identity(2), identity(3)]),
        operator_inputs: Vec::new(),
        authorize_declassification: false,
        parameters: Vec::new(),
    };
    (revisions, intent)
}
fn parameter(source: u8, target: u8, text: &str) -> MigrationParameterInput {
    MigrationParameterInput {
        edge: MigrationEdgeIdentity {
            source: identity(source).content_digest,
            target: identity(target).content_digest,
        },
        parameter: RawParameterInput {
            id: ParameterIdentity::parse("mode").unwrap(),
            text: text.into(),
            source: ParameterTextSource::Protected,
        },
    }
}

// Test-ID: PR-TEST-0714
// Verifies: PR-REQ-0390
#[test]
fn migration_parameter_preflight_is_exact_edge_scoped_and_covers_the_whole_path() {
    let (revisions, _) = chain();
    let plan = vec![identity(1), identity(2), identity(3)];
    assert!(
        matches!(bind_migration_parameters(&plan,&revisions,&[]),Err(MigrationError::Parameters {edge,error:ActionResolutionError::MissingParameter(_),..}) if edge.target==identity(3).content_digest)
    );
    let inputs = vec![parameter(1, 2, "false"), parameter(2, 3, "safe")];
    let bound = bind_migration_parameters(&plan, &revisions, &inputs).unwrap();
    assert!(matches!(
        bound[&inputs[0].edge][0].value(),
        InvocationParameterValue::Boolean(false)
    ));
    assert!(
        matches!(bound[&inputs[1].edge][0].value(),InvocationParameterValue::String(value) if value=="safe")
    );
    assert!(bound[&inputs[1].edge][0].effective_redaction);
    let defaults = bind_migration_parameters(&plan, &revisions, &inputs[1..]).unwrap();
    assert!(matches!(
        defaults[&inputs[0].edge][0].value(),
        InvocationParameterValue::Boolean(true)
    ));
    assert!(matches!(
        bind_migration_parameters(&plan, &revisions, &[parameter(1, 3, "safe")]),
        Err(MigrationError::InvalidParameterTarget)
    ));
    assert!(matches!(
        bind_migration_parameters(
            &plan,
            &revisions,
            &[inputs[0].clone(), inputs[0].clone(), inputs[1].clone()]
        ),
        Err(MigrationError::Parameters {
            error: ActionResolutionError::DuplicateParameter(_),
            ..
        })
    ));
    assert!(matches!(
        bind_migration_parameters(
            &plan,
            &revisions,
            &[parameter(1, 2, "not-bool"), inputs[1].clone()]
        ),
        Err(MigrationError::Parameters {
            error: ActionResolutionError::InvalidParameter { .. },
            ..
        })
    ));
}

// Test-ID: PR-TEST-0715
// Verifies: PR-REQ-0381
#[test]
fn optional_migration_reads_are_not_promoted_to_execution_requirements() {
    let (revisions, intent) = chain();
    for (source, target) in [(1, 2), (2, 3)] {
        let edge = revisions[target - 1]
            .content
            .core
            .migration_declaration(&Sha256Digest::from_bytes(
                *identity(source).content_digest.as_bytes(),
            ))
            .unwrap();
        assert!(edge.requires_source.is_empty());
        assert!(edge.requires_target.is_empty());
    }
    let plan = build_migration_binding_plan(&intent, &revisions, &[]).unwrap();
    assert_eq!(plan.edges().len(), 2);
    assert!(
        plan.edges()
            .iter()
            .all(|edge| edge.declaration().requires_source.is_empty()
                && edge.declaration().requires_target.is_empty())
    );
    // The structural view still validates optional role references.
    assert_eq!(
        revisions[1].content.core.migrations()[0]
            .requires_source
            .len(),
        1
    );
}

// Test-ID: PR-TEST-0716
// Verifies: PR-REQ-0381, PR-REQ-0385
#[test]
fn restore_requirements_use_snapshot_presence_not_current_instance_readiness() {
    let revision = revision(
        1,
        json!({"inputs":[{"id":"config"},{"id":"unused"}],"snapshot":{"restore":{"parameters":[],"inputs":{"required":["config"],"optional":["unused"]},"hook":hook()}}}),
    );
    let snapshot = |bound| {
        SnapshotManifest::new(SnapshotManifestParts {
            version: SnapshotIntegrityVersion::BASELINE,
            snapshot_id: SnapshotId::from_bytes([9; 16]),
            producer: identity(1),
            origin_instance_id: InstanceId::from_bytes([8; 16]),
            captured_at: SnapshotTimestamp::new(0, 0).unwrap(),
            service_content: vec![],
            managed_bindings: vec![SnapshotBinding {
                input_id: InputIdentity::parse("config").unwrap(),
                role: SnapshotBindingRole::Active,
                state: if bound {
                    SnapshotBindingState::Bound(Sha256Digest::from_bytes([0; 32]))
                } else {
                    SnapshotBindingState::Absent
                },
                protection: ManagedInputProtection::Normal,
            }],
        })
        .unwrap()
    };
    assert!(
        matches!(validate_restore_requirements(&revision.content.core,&snapshot(false)),Err(SnapshotPlanError::MissingRequired(ids)) if ids==vec![InputIdentity::parse("config").unwrap()])
    );
    assert!(validate_restore_requirements(&revision.content.core, &snapshot(true)).is_ok());
}

// Test-ID: PR-TEST-0717
// Verifies: PR-REQ-0390
#[test]
fn cleanup_finalization_only_never_requires_or_replays_hook_parameters() {
    let revision = revision(
        1,
        json!({"cleanup":{"parameters":[{"id":"confirm","type":"boolean","sensitive":false}],"hook":hook()}}),
    );
    let instance = InstanceView {
        id: InstanceId::from_bytes([2; 16]),
        name: InstanceName::parse("demo").unwrap(),
        active_revision: identity(1),
        state_version: InstanceStateVersion::from_bytes([3; 16]),
        required_inputs_satisfied: true,
        recovery_guard: None,
        bindings: vec![],
    };
    let observed = |obligation| DeletionCompilationObservation {
        instance: instance.clone(),
        revision_identity: identity(1),
        revision: revision.content.clone(),
        service_state: None,
        obligation,
    };
    let mut intent = DeleteInstance {
        instance: instance.id,
        expected: instance.state_version,
        mode: DeletionMode::ManagedCleanup,
        parameters: vec![],
    };
    assert!(matches!(
        build_deletion_plan(&intent, observed(None), None),
        Err(DeletionPlanError::Parameters(
            ActionResolutionError::MissingParameter(_)
        ))
    ));
    let obligation = Some(DeletionObligation {
        instance: instance.id,
        attempt: RunId::from_bytes([4; 16]),
        phase: DeletionPhase::FinalizationAuthorized,
    });
    let plan = build_deletion_plan(&intent, observed(obligation), None).unwrap();
    assert!(matches!(plan.work, DeletionWork::FinalizationOnly { .. }));
    assert_eq!(
        rebuild_deletion_plan(&plan, observed(obligation)).unwrap(),
        plan
    );
    intent.parameters.push(RawParameterInput {
        id: ParameterIdentity::parse("confirm").unwrap(),
        text: "true".into(),
        source: ParameterTextSource::Ordinary,
    });
    assert!(matches!(
        build_deletion_plan(&intent, observed(obligation), None),
        Err(DeletionPlanError::InvalidParameters)
    ));
}

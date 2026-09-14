use super::*;

// Test-ID: PR-TEST-0286
// Verifies: PR-REQ-0303, PR-REQ-0309
#[test]
fn uniqueness_matches_exhaustive_simple_paths_and_ignores_dead_end_cycles() {
    fn enumerate(graph: &[Vec<usize>], path: &mut Vec<usize>, found: &mut Vec<Vec<usize>>) {
        if path.last() == Some(&3) {
            found.push(path.clone());
            return;
        }
        for next in &graph[*path.last().unwrap()] {
            if !path.contains(next) {
                path.push(*next);
                enumerate(graph, path, found);
                path.pop();
            }
        }
    }
    let pairs: Vec<_> = (0..4)
        .flat_map(|from| {
            (0..4)
                .filter(move |to| from != *to)
                .map(move |to| (from, to))
        })
        .collect();
    for mask in 0..(1 << pairs.len()) {
        let mut graph = vec![vec![]; 4];
        let mut incoming = vec![vec![]; 4];
        for (bit, (from, to)) in pairs.iter().enumerate() {
            if mask & (1 << bit) != 0 {
                graph[*from].push(*to);
                incoming[*to].push(edge(*from as u8 + 1, vec![]));
            }
        }
        let nodes: Vec<_> = incoming
            .into_iter()
            .enumerate()
            .map(|(i, edges)| revision(i as u8 + 1, vec![], edges))
            .collect();
        let mut expected = vec![];
        enumerate(&graph, &mut vec![0], &mut expected);
        let actual = resolve_migration_path(&nodes, &identity(1), &identity(4), &[]);
        let mut listed = Vec::new();
        let mut cursor = None;
        loop {
            let page = list_migration_paths(&nodes, &identity(1), &identity(4), cursor.as_ref(), 1)
                .unwrap();
            for candidate in &page.candidates {
                assert_eq!(
                    candidate
                        .id
                        .resolve(&nodes, &identity(1), &identity(4))
                        .unwrap(),
                    candidate.revisions
                );
                listed.push(candidate.revisions.clone());
            }
            if !page.has_more {
                break;
            }
            cursor = Some(page.candidates.last().unwrap().id.clone());
        }
        assert_eq!(
            listed,
            expected
                .iter()
                .map(|p| p.iter().map(|i| identity(*i as u8 + 1)).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );
        match expected.len() {
            0 => assert_eq!(actual, Err(MigrationError::NoPath)),
            1 => assert_eq!(
                actual.unwrap(),
                expected[0]
                    .iter()
                    .map(|n| identity(*n as u8 + 1))
                    .collect::<Vec<_>>()
            ),
            _ => assert_eq!(actual, Err(MigrationError::AmbiguousPath)),
        }
    }
    // A dense component can reach the target only by revisiting source. It
    // contributes no simple path and must not trigger factorial enumeration.
    let mut nodes = vec![revision(
        1,
        vec![],
        (2..14).map(|n| edge(n, vec![])).collect(),
    )];
    for n in 2..14 {
        nodes.push(revision(
            n,
            vec![],
            (1..14)
                .filter(|i| *i != n)
                .map(|i| edge(i, vec![]))
                .collect(),
        ));
    }
    nodes.push(revision(14, vec![], vec![edge(1, vec![])]));
    assert_eq!(
        resolve_migration_path(&nodes, &identity(1), &identity(14), &[]).unwrap(),
        vec![identity(1), identity(14)]
    );
    let page = list_migration_paths(&nodes, &identity(1), &identity(14), None, 1).unwrap();
    assert_eq!(page.candidates.len(), 1);
    assert!(!page.has_more);
}

// Test-ID: PR-TEST-0287
// Verifies: PR-REQ-0309
#[test]
fn path_ids_have_exact_independently_calculated_v1_spelling() {
    let direct = MigrationPathId::for_path(&[identity(1), identity(3)]).unwrap();
    let via = MigrationPathId::for_path(&[identity(1), identity(2), identity(3)]).unwrap();
    // SHA-256 fixtures independently calculated using .NET SHA256, not this codec.
    assert_eq!(
        direct.to_string(),
        "mp1-1629d3ccccd9d61243f29ec8a2cdb3beafbd4c103d2a775e6c235b759298263d"
    );
    assert_eq!(
        via.to_string(),
        format!(
            "mp1-cd67e9785277dedd161ce117d05a75902dbcba0f9000162bd6c67e4121d7d379-{}",
            "02".repeat(32)
        )
    );
    for token in [direct, via] {
        assert_eq!(token.to_string().parse::<MigrationPathId>().unwrap(), token);
    }
    for bad in [
        "1".to_owned(),
        "mp1-abc".to_owned(),
        format!("mp1-{}-", "00".repeat(32)),
        format!("mp1-{}", "AF".repeat(32)),
        format!("mp2-{}", "00".repeat(32)),
    ] {
        assert_eq!(
            bad.parse::<MigrationPathId>(),
            Err(MigrationError::InvalidPathId)
        );
    }
}

fn path_graph() -> Vec<MigrationRevision> {
    vec![
        revision(1, vec![], vec![]),
        revision(2, vec![], vec![edge(1, vec![])]),
        revision(
            3,
            vec![],
            vec![edge(0, vec![]), edge(1, vec![]), edge(2, vec![])],
        ),
    ]
}

// Test-ID: PR-TEST-0288
// Verifies: PR-REQ-0303, PR-REQ-0309
#[test]
fn listed_ids_survive_reordering_and_newly_installed_routes_without_remapping() {
    let mut graph = path_graph();
    let before = list_migration_paths(&graph, &identity(1), &identity(3), None, 20).unwrap();
    assert_eq!(before.candidates.len(), 2);
    graph.push(revision(0, vec![], vec![edge(1, vec![])]));
    graph.reverse();
    let after = list_migration_paths(&graph, &identity(1), &identity(3), None, 20).unwrap();
    assert_eq!(after.candidates.len(), 3);
    for old in before.candidates {
        assert_eq!(
            old.id.resolve(&graph, &identity(1), &identity(3)).unwrap(),
            old.revisions
        );
        assert!(after.candidates.contains(&old));
        let mut request = intent(1, 3);
        request.path = MigrationPathSelection::Exact(old.revisions.clone());
        assert_eq!(
            build_migration_binding_plan(&request, &graph, &[])
                .unwrap()
                .edges()
                .len(),
            old.revisions.len() - 1
        );
    }
}

// Test-ID: PR-TEST-0289
// Verifies: PR-REQ-0309
#[test]
fn stale_context_removed_edges_tampering_and_invalid_cursors_fail_closed() {
    let graph = path_graph();
    let page = list_migration_paths(&graph, &identity(1), &identity(3), None, 1).unwrap();
    assert!(page.has_more);
    let id = page.candidates[0].id.clone();
    let mut other_lineage = path_graph();
    for node in &mut other_lineage {
        node.identity.package_id = PackageId::from_bytes([2; 16]);
    }
    assert_eq!(
        id.resolve(
            &other_lineage,
            &other_lineage[0].identity,
            &other_lineage[2].identity
        ),
        Err(MigrationError::PathContextMismatch)
    );
    assert_eq!(
        id.resolve(&graph, &identity(2), &identity(3)),
        Err(MigrationError::PathContextMismatch)
    );
    assert_eq!(
        id.resolve(&graph, &identity(1), &identity(2)),
        Err(MigrationError::PathContextMismatch)
    );
    let removed: Vec<_> = graph
        .into_iter()
        .filter(|n| n.identity != identity(2))
        .collect();
    assert_eq!(
        id.resolve(&removed, &identity(1), &identity(3)),
        Err(MigrationError::InvalidEdge)
    );
    assert_eq!(
        list_migration_paths(&removed, &identity(1), &identity(3), Some(&id), 1),
        Err(MigrationError::InvalidEdge)
    );
    let forged = format!("mp1-{}", "00".repeat(32))
        .parse::<MigrationPathId>()
        .unwrap();
    assert_eq!(
        forged.resolve(&removed, &identity(1), &identity(3)),
        Err(MigrationError::PathContextMismatch)
    );
    for limit in [0, 101] {
        assert_eq!(
            list_migration_paths(&removed, &identity(1), &identity(3), None, limit),
            Err(MigrationError::InvalidPageSize)
        );
    }
}

fn id(value: &str) -> InputIdentity {
    InputIdentity::parse(value).unwrap()
}
fn identity(value: u8) -> RevisionIdentity {
    RevisionIdentity::new(
        PackageId::from_bytes([1; 16]),
        RevisionContentDigest::from_bytes([value; 32]),
    )
}
fn input(name: &str, required: bool, secret: bool) -> InputDeclarationV1 {
    InputDeclarationV1 {
        id: id(name),
        required,
        protection: if secret {
            InputProtectionV1::Secret
        } else {
            InputProtectionV1::Normal
        },
    }
}
fn reference(name: &str, retained: bool) -> InputBindingRefV1 {
    InputBindingRefV1 {
        input_id: id(name),
        role: if retained {
            InputBindingRoleV1::Retained
        } else {
            InputBindingRoleV1::Active
        },
    }
}
fn carry(from: &str, to: &str) -> MigrationTransitionV1 {
    MigrationTransitionV1::Carry {
        source: reference(from, false),
        target_input_id: id(to),
    }
}
fn edge(source: u8, transitions: Vec<MigrationTransitionV1>) -> MigrationV1 {
    MigrationV1 {
        source_revision_digest: Sha256Digest::from_bytes([source; 32]),
        transitions,
        requires_source: vec![],
        requires_target: vec![],
        produces_target: vec![],
        hook: None,
    }
}
fn revision(
    value: u8,
    inputs: Vec<InputDeclarationV1>,
    edges: Vec<MigrationV1>,
) -> MigrationRevision {
    let file = RuntimeFileV1 {
        id: ContentId::parse("tool").unwrap(),
        path: RuntimePath::parse("bin/tool").unwrap(),
        kind: RuntimeFileKindV1::RegularFile,
        blob_digest: Sha256Digest::from_bytes([99; 32]),
        executable: true,
    };
    let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
        inputs,
        actions: vec![],
        snapshot: None,
        migrations: edges,
        cleanup: None,
    })
    .unwrap();
    let closure =
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![file] })
            .unwrap();
    MigrationRevision {
        identity: identity(value),
        content: validate_revision_content_v1(core, closure).unwrap(),
    }
}
fn hook() -> HookV1 {
    HookV1 {
        protocol_version: PositiveVersion::new(1).unwrap(),
        launch: HookLaunchV1::Direct {
            executable: ContentId::parse("tool").unwrap(),
        },
        args: vec![],
        io: IOContractV1 {
            terminal: TerminalContractV1::None,
        },
    }
}
fn intent(from: u8, to: u8) -> TransitionRevision {
    TransitionRevision {
        instance: InstanceId::from_bytes([1; 16]),
        expected_state_version: InstanceStateVersion::from_bytes([2; 16]),
        source: identity(from),
        target: identity(to),
        path: MigrationPathSelection::Automatic,
        operator_inputs: vec![],
        authorize_declassification: false,
    }
}
fn binding(name: &str, value: u8, secret: bool) -> MigrationBinding {
    MigrationBinding {
        input: id(name),
        origin: MigrationValueOrigin::Existing(ManagedInputPayloadId::from_bytes([value; 16])),
        protection: if secret {
            ManagedInputProtection::Secret
        } else {
            ManagedInputProtection::Normal
        },
    }
}

// Test-ID: PR-TEST-0277
// Verifies: PR-REQ-0303, PR-REQ-0153, PR-REQ-0165
#[test]
fn migration_paths_are_exact_unique_and_independent_of_order() {
    let a = revision(1, vec![], vec![]);
    let b = revision(2, vec![], vec![edge(1, vec![])]);
    let c = revision(3, vec![], vec![edge(2, vec![])]);
    for nodes in [
        vec![a.clone(), b.clone(), c.clone()],
        vec![c.clone(), b.clone(), a.clone()],
    ] {
        assert_eq!(
            resolve_migration_path(&nodes, &identity(1), &identity(3), &[]).unwrap(),
            vec![identity(1), identity(2), identity(3)]
        );
        assert_eq!(
            resolve_migration_path(&nodes, &identity(1), &identity(3), &[identity(2)]).unwrap(),
            vec![identity(1), identity(2), identity(3)]
        );
        assert_eq!(
            resolve_migration_path(&nodes, &identity(3), &identity(1), &[]),
            Err(MigrationError::NoPath)
        );
    }
    let direct = revision(3, vec![], vec![edge(1, vec![]), edge(2, vec![])]);
    let nodes = vec![a, b, direct];
    assert_eq!(
        resolve_migration_path(&nodes, &identity(1), &identity(3), &[]),
        Err(MigrationError::AmbiguousPath)
    );
    assert!(resolve_migration_path(&nodes, &identity(1), &identity(3), &[identity(2)]).is_ok());
    assert_eq!(
        resolve_migration_path(&nodes, &identity(1), &identity(3), &[identity(1)]),
        Err(MigrationError::RepeatedRevision)
    );
    assert_eq!(
        resolve_migration_path(&nodes, &identity(1), &identity(1), &[]),
        Err(MigrationError::ZeroEdges)
    );
    let foreign = RevisionIdentity::new(PackageId::from_bytes([2; 16]), identity(3).content_digest);
    assert_eq!(
        resolve_migration_path(&nodes, &identity(1), &foreign, &[]),
        Err(MigrationError::CrossLineage)
    );
}

// Test-ID: PR-TEST-0278
// Verifies: PR-REQ-0303, PR-REQ-0153
#[test]
fn missing_invalid_and_cyclic_edges_do_not_become_executable() {
    let mut invalid = edge(1, vec![]);
    invalid.requires_source.push(reference("config", true));
    let a = revision(
        1,
        vec![input("config", false, false)],
        vec![edge(2, vec![])],
    );
    let b = revision(2, vec![], vec![invalid]);
    assert_eq!(
        resolve_migration_path(&[a.clone(), b], &identity(1), &identity(2), &[]),
        Err(MigrationError::NoPath)
    );
    let b = revision(2, vec![], vec![edge(1, vec![])]);
    let c = revision(3, vec![], vec![edge(2, vec![]), edge(4, vec![])]);
    assert_eq!(
        resolve_migration_path(&[b.clone(), c.clone()], &identity(1), &identity(3), &[]),
        Err(MigrationError::MissingRevision)
    );
    let nodes = vec![a, b, c];
    assert_eq!(
        resolve_migration_path(&nodes, &identity(1), &identity(3), &[]).unwrap(),
        vec![identity(1), identity(2), identity(3)]
    );
    assert_eq!(
        resolve_migration_path(&nodes, &identity(1), &identity(2), &[identity(3)]),
        Err(MigrationError::InvalidEdge)
    );
}

// Test-ID: PR-TEST-0279
// Verifies: PR-REQ-0304, PR-REQ-0154, PR-REQ-0162
#[test]
fn absence_is_not_a_value_and_does_not_release_a_declared_writer() {
    let a = revision(1, vec![input("old", false, false)], vec![]);
    let b = revision(
        2,
        vec![input("new", true, false)],
        vec![edge(1, vec![carry("old", "new")])],
    );
    let nodes = vec![a.clone(), b];
    let mut request = intent(1, 2);
    let plan = build_migration_binding_plan(&request, &nodes, &[]).unwrap();
    assert!(plan.edges()[0].committed_bindings().is_empty());
    assert!(!plan.edges()[0].required_inputs_satisfied());
    request.operator_inputs.push(MigrationTargetInput {
        revision: identity(2).content_digest,
        input: id("new"),
    });
    assert_eq!(
        build_migration_binding_plan(&request, &nodes, &[]),
        Err(MigrationError::MultipleWriters(id("new")))
    );
    let mut required = edge(1, vec![carry("old", "new")]);
    required.requires_source.push(reference("old", false));
    let b = revision(2, vec![input("new", false, false)], vec![required.clone()]);
    assert_eq!(
        build_migration_binding_plan(&intent(1, 2), &[a.clone(), b], &[]),
        Err(MigrationError::MissingSource(id("old")))
    );
    required.requires_source.clear();
    required.requires_target.push(id("new"));
    let b = revision(2, vec![input("new", false, false)], vec![required]);
    assert_eq!(
        build_migration_binding_plan(&intent(1, 2), &[a.clone(), b.clone()], &[]),
        Err(MigrationError::MissingTarget(id("new")))
    );
    // Presence is represented by an opaque payload reference, including empty files.
    assert!(
        build_migration_binding_plan(&intent(1, 2), &[a, b], &[binding("old", 1, false)]).is_ok()
    );
}

// Test-ID: PR-TEST-0280
// Verifies: PR-REQ-0304, PR-REQ-0305, PR-REQ-0157, PR-REQ-0158, PR-REQ-0161
#[test]
fn keep_discard_rename_and_retained_continuity_share_one_registry() {
    let a = revision(
        1,
        vec![input("old", false, false), input("gone", false, false)],
        vec![],
    );
    let mut retained = carry("saved", "new");
    if let MigrationTransitionV1::Carry { source, .. } = &mut retained {
        source.role = InputBindingRoleV1::Retained;
    }
    let b = revision(
        2,
        vec![input("new", true, false)],
        vec![edge(
            1,
            vec![
                retained,
                MigrationTransitionV1::Keep {
                    source: reference("old", false),
                },
                MigrationTransitionV1::Discard {
                    source: reference("gone", false),
                },
            ],
        )],
    );
    let observed = vec![
        binding("saved", 1, false),
        binding("old", 2, true),
        binding("gone", 3, false),
        binding("unrelated", 4, true),
    ];
    let plan = build_migration_binding_plan(&intent(1, 2), &[a, b], &observed).unwrap();
    let result = plan.edges()[0].committed_bindings();
    assert_eq!(
        result.iter().map(|b| b.input.as_str()).collect::<Vec<_>>(),
        vec!["new", "old", "unrelated"]
    );
    assert_eq!(result[0].origin, observed[0].origin);
    assert_eq!(result[1].protection, ManagedInputProtection::Secret);
    assert_eq!(result[2].protection, ManagedInputProtection::Secret);
    assert!(plan.edges()[0].required_inputs_satisfied());
}

// Test-ID: PR-TEST-0281
// Verifies: PR-REQ-0304, PR-REQ-0305, PR-REQ-0156, PR-REQ-0157, PR-REQ-0160
#[test]
fn protection_requires_declared_and_authorized_declassification() {
    let a = revision(1, vec![input("old", false, true)], vec![]);
    let b = revision(
        2,
        vec![input("new", false, false)],
        vec![edge(1, vec![carry("old", "new")])],
    );
    let carried =
        build_migration_binding_plan(&intent(1, 2), &[a.clone(), b], &[binding("old", 1, true)])
            .unwrap();
    assert_eq!(
        carried.edges()[0].committed_bindings()[0].protection,
        ManagedInputProtection::Secret
    );
    let b = revision(
        2,
        vec![input("new", false, false)],
        vec![edge(
            1,
            vec![MigrationTransitionV1::Declassify {
                source: reference("old", false),
                target_input_id: id("new"),
            }],
        )],
    );
    let mut request = intent(1, 2);
    assert_eq!(
        build_migration_binding_plan(&request, &[a.clone(), b.clone()], &[]),
        Err(MigrationError::DeclassificationNotAuthorized)
    );
    request.authorize_declassification = true;
    let plan = build_migration_binding_plan(
        &request,
        &[a.clone(), b.clone()],
        &[binding("old", 1, true)],
    )
    .unwrap();
    assert_eq!(
        plan.edges()[0].committed_bindings()[0].protection,
        ManagedInputProtection::Normal
    );
    assert!(
        build_migration_binding_plan(&request, &[a, b], &[])
            .unwrap()
            .edges()[0]
            .committed_bindings()
            .is_empty()
    );
}

// Test-ID: PR-TEST-0282
// Verifies: PR-REQ-0305, PR-REQ-0154, PR-REQ-0163, PR-REQ-0164
#[test]
fn hook_outputs_feed_later_edges_without_requiring_intermediate_readiness() {
    let a = revision(1, vec![], vec![]);
    let mut produce = edge(1, vec![]);
    produce.hook = Some(hook());
    produce.produces_target.push(id("made"));
    let b = revision(
        2,
        vec![input("made", true, false), input("missing", true, false)],
        vec![produce.clone()],
    );
    let mut onward = edge(2, vec![carry("made", "final")]);
    onward.requires_source.push(reference("made", false));
    let c = revision(3, vec![input("final", true, false)], vec![onward]);
    let plan = build_migration_binding_plan(&intent(1, 3), &[a.clone(), b, c], &[]).unwrap();
    assert!(!plan.edges()[0].required_inputs_satisfied());
    assert!(plan.edges()[0].staged_bindings().is_empty());
    assert!(plan.edges()[1].required_inputs_satisfied());
    assert!(matches!(
        plan.edges()[1].committed_bindings()[0].origin,
        MigrationValueOrigin::Hook(_)
    ));
    assert!(validate_migration_completion(&produce, true, &[id("made")]).is_ok());
    for outputs in [vec![], vec![id("other")], vec![id("made"), id("made")]] {
        assert_eq!(
            validate_migration_completion(&produce, true, &outputs),
            Err(MigrationError::InvalidCompletion)
        );
    }
    assert!(validate_migration_completion(&produce, false, &[]).is_ok());
    assert!(validate_migration_completion(&produce, false, &[id("made")]).is_err());
    let mut overlap = produce.clone();
    overlap.requires_target.push(id("made"));
    assert!(
        project_revision_core_v1(RevisionCoreProjectionInputV1 {
            inputs: vec![input("made", true, false)],
            actions: vec![],
            snapshot: None,
            migrations: vec![overlap],
            cleanup: None,
        })
        .is_err()
    );
    produce.hook = None;
    let b = revision(2, vec![input("made", true, false)], vec![produce]);
    assert_eq!(
        build_migration_binding_plan(&intent(1, 2), &[a, b], &[]),
        Err(MigrationError::OutputsWithoutHook)
    );
}

// Test-ID: PR-TEST-0283
// Verifies: PR-REQ-0304, PR-REQ-0305, PR-REQ-0156, PR-REQ-0162
#[test]
fn operator_and_reactivation_writers_cannot_overwrite_existing_bindings() {
    let a = revision(1, vec![], vec![]);
    let b = revision(2, vec![input("new", false, false)], vec![edge(1, vec![])]);
    let mut request = intent(1, 2);
    request.operator_inputs.push(MigrationTargetInput {
        revision: identity(2).content_digest,
        input: id("new"),
    });
    assert!(build_migration_binding_plan(&request, &[a.clone(), b.clone()], &[]).is_ok());
    assert_eq!(
        build_migration_binding_plan(
            &request,
            &[a.clone(), b.clone()],
            &[binding("new", 1, true)]
        ),
        Err(MigrationError::MultipleWriters(id("new")))
    );
    request
        .operator_inputs
        .push(request.operator_inputs[0].clone());
    assert_eq!(
        build_migration_binding_plan(&request, &[a.clone(), b.clone()], &[]),
        Err(MigrationError::MultipleWriters(id("new")))
    );
    request.operator_inputs = vec![MigrationTargetInput {
        revision: identity(1).content_digest,
        input: id("new"),
    }];
    assert_eq!(
        build_migration_binding_plan(&request, &[a, b], &[]),
        Err(MigrationError::InvalidOperatorTarget)
    );
}

#[test]
fn simultaneous_renames_use_original_sources_not_transition_order() {
    let declarations = vec![input("a", false, false), input("b", false, false)];
    let a = revision(1, declarations.clone(), vec![]);
    let b = revision(
        2,
        declarations,
        vec![edge(1, vec![carry("a", "b"), carry("b", "a")])],
    );
    let plan = build_migration_binding_plan(
        &intent(1, 2),
        &[a, b],
        &[binding("a", 1, false), binding("b", 2, false)],
    )
    .unwrap();
    let values = plan.edges()[0].committed_bindings();
    assert_eq!(values[0].origin, binding("b", 2, false).origin);
    assert_eq!(values[1].origin, binding("a", 1, false).origin);
}

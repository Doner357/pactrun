use super::*;
use serde_json::{Value, json};

fn identity(value: u8) -> RevisionIdentity {
    RevisionIdentity::new(
        PackageId::from_bytes([1; 16]),
        RevisionContentDigest::from_bytes([value; 32]),
    )
}
fn resource(id: &str, storage: &str, locator: &str) -> Value {
    json!({"id":id,"storage_id":storage,"locator":locator,"kind":"file","read_exposure":"hidden","user_mutation":{"kind":"unavailable"}})
}
fn core(storages: &[&str], resources: Vec<Value>, migration: Option<Value>) -> RevisionCore {
    crate::revision_core_v2::project_revision_core_source_v2(&serde_json::to_vec(&json!({"format_version":2,"inputs":[],"actions":[],
        "migrations":migration.into_iter().collect::<Vec<_>>(),"service_storages":storages.iter().map(|id| json!({"id":id})).collect::<Vec<_>>(),"service_resources":resources})).unwrap()).unwrap().into()
}
fn edge(source: u8, storages: Value, resources: Value) -> Value {
    json!({"source_revision_digest":Sha256Digest::from_bytes([source;32]).as_str(),"transitions":[],"requires_source":[],"requires_target":[],"produces_target":[],
        "storage_transitions":storages,"resource_transitions":resources})
}
fn storage(kind: &str, source: &str, target: &str) -> Value {
    json!({"kind":kind,"source":{"role":if kind=="reattach" {"retained"} else {"active"},"storage_id":source},"target_storage_id":target})
}
fn reuse(kind: &str, source: &str, target: &str) -> Value {
    json!({"kind":kind,"source":{"role":if kind=="reattach" {"retained"} else {"active"},"resource_id":source},"target_resource_id":target})
}
fn initial() -> (RevisionCore, ServiceMigrationState) {
    let source = core(
        &["state"],
        vec![resource("config", "state", "config.json")],
        None,
    );
    let c = source.service_core().unwrap();
    let observed = InstanceServiceState {
        instance: InstanceId::from_bytes([1; 16]),
        state_version: InstanceStateVersion::from_bytes([2; 16]),
        current_revision: identity(1),
        storages: vec![ServiceStorageAssociation {
            declaration: c.storages()[0].clone(),
            declaration_revision: identity(1),
            allocation: ServiceAllocationId::from_bytes([7; 16]),
            role: ServiceRole::Active,
        }],
        resources: vec![ServiceResourceAssociation {
            declaration: c.resources()[0].clone(),
            declaration_revision: identity(1),
            allocation: ServiceAllocationId::from_bytes([7; 16]),
            role: ServiceRole::Active,
        }],
        preserved: vec![],
    };
    (
        source,
        ServiceMigrationState::from_observed(&observed).unwrap(),
    )
}

// Test-ID: PR-TEST-0371
// Verifies: PR-REQ-0319, PR-REQ-0326, PR-REQ-0327, PR-REQ-0241, PR-REQ-0245
#[test]
fn explicit_renames_consume_sources_without_ghost_aliases_or_allocation_changes() {
    let (source, before) = initial();
    let target = core(
        &["data"],
        vec![resource("settings", "data", "config.json")],
        Some(edge(
            1,
            json!([storage("reuse", "state", "data")]),
            json!([reuse("reuse", "config", "settings")]),
        )),
    );
    let result =
        evaluate_service_migration_edge(&before, &source, &identity(2), &target, 0).unwrap();
    let foreign = RevisionIdentity::new(PackageId::from_bytes([9; 16]), identity(2).content_digest);
    assert!(evaluate_service_migration_edge(&before, &source, &foreign, &target, 0).is_err());
    assert_eq!(before.resources.len(), 1);
    assert!(
        before
            .resources
            .contains_key(&ServiceResourceIdentity::parse("config").unwrap())
    );
    assert!(
        !result
            .after
            .resources
            .contains_key(&ServiceResourceIdentity::parse("config").unwrap())
    );
    assert!(
        !result
            .after
            .storages
            .contains_key(&ServiceStorageIdentity::parse("state").unwrap())
    );
    let settings = ServiceResourceIdentity::parse("settings").unwrap();
    assert_eq!(
        result.after.resources[&settings].allocation,
        ServiceAllocationOrigin::Existing(ServiceAllocationId::from_bytes([7; 16]))
    );
    let next = core(
        &["data"],
        vec![resource("settings", "data", "config.json")],
        Some(edge(
            2,
            json!([storage("reuse", "data", "data")]),
            json!([reuse("reuse", "settings", "settings")]),
        )),
    );
    let next =
        evaluate_service_migration_edge(&result.after, &target, &identity(3), &next, 1).unwrap();
    assert_eq!(next.after.resources.len(), 1);
    assert_eq!(
        next.after.resources[&settings].allocation,
        result.after.resources[&settings].allocation
    );
}

// Test-ID: PR-TEST-0372
// Verifies: PR-REQ-0319, PR-REQ-0327
#[test]
fn only_unmapped_sources_are_retained_and_reattachment_is_explicit() {
    let (source, before) = initial();
    let empty = core(&[], vec![], Some(edge(1, json!([]), json!([]))));
    let dropped =
        evaluate_service_migration_edge(&before, &source, &identity(2), &empty, 0).unwrap();
    assert_eq!(dropped.after.resources, before.resources);
    assert_eq!(dropped.after.storages, before.storages);
    let attached = core(
        &["state"],
        vec![resource("config", "state", "config.json")],
        Some(edge(
            2,
            json!([storage("reattach", "state", "state")]),
            json!([reuse("reattach", "config", "config")]),
        )),
    );
    let attached =
        evaluate_service_migration_edge(&dropped.after, &empty, &identity(3), &attached, 1)
            .unwrap();
    assert_eq!(
        attached
            .after
            .resources
            .values()
            .next()
            .unwrap()
            .declaration_revision,
        identity(3)
    );
    let disguised = core(
        &["state"],
        vec![resource("new_name", "state", "config.json")],
        Some(edge(
            2,
            json!([storage("reattach", "state", "state")]),
            json!([{"kind":"create","target_resource_id":"new_name","presence":"any"}]),
        )),
    );
    assert!(
        evaluate_service_migration_edge(&dropped.after, &empty, &identity(3), &disguised, 1)
            .is_err()
    );
    let overwrite = core(
        &["state"],
        vec![resource("config", "state", "other.json")],
        Some(edge(
            2,
            json!([storage("reattach", "state", "state")]),
            json!([{"kind":"create","target_resource_id":"config","presence":"any"}]),
        )),
    );
    assert!(
        evaluate_service_migration_edge(&dropped.after, &empty, &identity(3), &overwrite, 1)
            .is_err()
    );
}

// Test-ID: PR-TEST-0373
// Verifies: PR-REQ-0319, PR-REQ-0326
#[test]
fn split_transform_plans_fresh_roots_without_allocating_or_presuming_presence() {
    let (source, before) = initial();
    let mut transition = edge(
        1,
        json!([{"kind":"create","target_storage_id":"new_state"}]),
        json!([{"kind":"transform","sources":[{"role":"active","resource_id":"config"}],"targets":["left","right"]}]),
    );
    transition["hook"] = json!({"protocol_version":2,"launch":{"kind":"direct","executable":"tool"},"args":[],"io":{"terminal":"none"},"service_requires":[],
        "service_access":[{"reference":{"view":"source","role":"active","kind":"resource","id":"config"},"mode":"read"},
        {"reference":{"view":"target","role":"active","kind":"storage","id":"new_state"},"mode":"write"}]});
    let target = core(
        &["new_state"],
        vec![
            resource("left", "new_state", "left.json"),
            resource("right", "new_state", "right.json"),
        ],
        Some(transition),
    );
    let result =
        evaluate_service_migration_edge(&before, &source, &identity(2), &target, 0).unwrap();
    assert!(result.transform);
    assert_eq!(result.after.resources.len(), 2);
    assert!(result.after.resources.values().all(|r| r.allocation
        == ServiceAllocationOrigin::Created {
            edge: 0,
            storage: ServiceStorageIdentity::parse("new_state").unwrap()
        }));
    assert!(
        result
            .after
            .storages
            .contains_key(&ServiceStorageIdentity::parse("state").unwrap())
    );
    assert_eq!(result.grants.len(), 2);
    assert!(result.create_presence.is_empty());
    assert!(result.requires.is_empty());
}

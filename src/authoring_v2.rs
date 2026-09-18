//! Version-2 schema-directed source projection. Acquisition remains separate.
use super::*;
use crate::{domain::RevisionCoreV2, revision_core_v2::project_revision_core_source_v2};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NormalizedPackSourceCandidateV2 {
    pub(crate) package_id: PackageId,
    pub(crate) revision: RevisionCoreV2,
    pub(crate) runtime_sources: Vec<RuntimeSourceRecordV1>,
    pub(crate) portable_metadata: PortableMetadataTemplate,
}

pub(crate) fn parse_pack_source_yaml_v2(
    bytes: &[u8],
) -> Result<NormalizedPackSourceCandidateV2, AuthoringError> {
    let source = parse_service_source(bytes, 2)?;
    let crate::domain::RevisionCore::V2(revision) = source.revision else {
        unreachable!("explicit V2 source selection")
    };
    Ok(NormalizedPackSourceCandidateV2 {
        package_id: source.package_id,
        revision: *revision,
        runtime_sources: source.runtime_sources,
        portable_metadata: source.portable_metadata,
    })
}

pub(super) fn parse_service_source(
    bytes: &[u8],
    source_version: u8,
) -> Result<VersionedPackSourceCandidate, AuthoringError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| AuthoringError::new("pactrun.yaml must be valid UTF-8"))?;
    reject_forbidden_tokens(text)?;
    let mut root = closed_map(
        parse_document(text)?,
        &["source_format", "package_id", "revision", "runtime_content"],
        &["portable_metadata"],
        "PackSourceYamlV2",
    )?;
    let version = take_scalar(&mut root, "source_format")?;
    if version.style != TScalarStyle::Plain
        || capture_numeric_token(text, &version)? != source_version.to_string()
    {
        return Err(AuthoringError::new(
            "source_format must use the selected raw integer token",
        ));
    }
    let package_id = PackageId::from_str(&string_scalar(take(&mut root, "package_id")?)?)
        .map_err(AuthoringError::new)?;
    let mut json = revision_v2_json(text, take(&mut root, "revision")?)?;
    json["format_version"] = source_version.into();
    let bytes = serde_json::to_vec(&json).map_err(|e| AuthoringError::new(e.to_string()))?;
    let revision: crate::domain::RevisionCore = match source_version {
        2 => project_revision_core_source_v2(&bytes).map(Into::into),
        3 => crate::revision_core_v3::project(&bytes).map(Into::into),
        _ => unreachable!("closed source dispatch"),
    }
    .map_err(|e| AuthoringError::new(format!("project Core: {e}")))?;
    let runtime_sources = runtime_sources(take(&mut root, "runtime_content")?)?;
    let portable_metadata = root
        .remove("portable_metadata")
        .map(portable_metadata)
        .transpose()?
        .unwrap_or_default();
    for p in &portable_metadata.presentation {
        if !revision.common().contains_presentation_target(&p.target) {
            return Err(AuthoringError::new(
                "presentation target is not declared by the normalized Revision",
            ));
        }
    }
    Ok(VersionedPackSourceCandidate {
        package_id,
        revision,
        runtime_sources,
        portable_metadata,
    })
}

fn hook_defaults(node: &mut Node) -> Result<(), AuthoringError> {
    let owner = mapping_mut(node, "Hook owner")?;
    if let Some(hook) = owner.get_mut("hook") {
        let hook = mapping_mut(hook, "Hook")?;
        for key in ["service_access", "service_requires"] {
            hook.entry(key.into())
                .or_insert_with(|| Node::Sequence(vec![]));
        }
    }
    Ok(())
}
fn revision_v2_json(source: &str, node: Node) -> Result<Value, AuthoringError> {
    let mut revision = closed_map(
        node,
        &[],
        &[
            "inputs",
            "actions",
            "snapshot",
            "migrations",
            "cleanup",
            "service_storages",
            "service_resources",
        ],
        "revision",
    )?;
    for key in [
        "inputs",
        "actions",
        "migrations",
        "service_storages",
        "service_resources",
    ] {
        revision
            .entry(key.into())
            .or_insert_with(|| Node::Sequence(vec![]));
    }
    if let Some(Node::Sequence(inputs)) = revision.get_mut("inputs") {
        for input in inputs {
            let o = mapping_mut(input, "Input")?;
            o.entry("required".into()).or_insert_with(|| plain("false"));
            o.entry("protection".into())
                .or_insert_with(|| plain("normal"));
        }
    }
    if let Some(Node::Sequence(resources)) = revision.get_mut("service_resources") {
        for resource in resources {
            let o = mapping_mut(resource, "ServiceResource")?;
            o.entry("read_exposure".into())
                .or_insert_with(|| plain("hidden"));
            o.entry("user_mutation".into()).or_insert_with(|| {
                Node::Mapping(BTreeMap::from([("kind".into(), plain("unavailable"))]))
            });
        }
    }
    if let Some(Node::Sequence(actions)) = revision.get_mut("actions") {
        for a in actions {
            hook_defaults(a)?;
        }
    }
    if let Some(snapshot) = revision.get_mut("snapshot") {
        let s = mapping_mut(snapshot, "Snapshot capability")?;
        for key in ["capture", "restore"] {
            if let Some(op) = s.get_mut(key) {
                hook_defaults(op)?;
            }
        }
    }
    if let Some(cleanup) = revision.get_mut("cleanup") {
        hook_defaults(cleanup)?;
    }
    if let Some(Node::Sequence(migrations)) = revision.get_mut("migrations") {
        for m in migrations {
            hook_defaults(m)?;
            let o = mapping_mut(m, "Migration")?;
            for key in ["storage_transitions", "resource_transitions"] {
                o.entry(key.into())
                    .or_insert_with(|| Node::Sequence(vec![]));
            }
        }
    }
    let mut json = Map::new();
    json.insert("format_version".into(), Value::Number(Number::from(2)));
    for (key, node) in revision {
        json.insert(key, node_to_json(source, node, None)?);
    }
    Ok(Value::Object(json))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::*, revision_core_v2::encode_canonical_revision_core_v2};

    // Test-ID: PR-TEST-0494
    // Verifies: PR-REQ-0350
    #[test]
    fn explicit_yaml_v3_preserves_closed_projection_and_rejects_implicit_fallback() {
        let text = "source_format: 3\npackage_id: 00000000000000000000000000000065\nrevision:\n  actions:\n    - id: run\n      access: observe\n      parameters: []\n      outputs: []\n      hook:\n        protocol_version: 1\n        launch: {kind: shell_loader, shell: sh, command: sh, script: script}\n        args: []\n        io: {terminal: none}\nruntime_content:\n  files: [{id: script, source: script.sh, path: script.sh, executable: false}]\n";
        let candidate = super::super::parse_pack_source_yaml(text.as_bytes()).unwrap();
        assert_eq!(candidate.revision.version(), 3);
        assert!(matches!(
            candidate.revision.actions()[0].hook.launch,
            HookLaunchV1::ShellLoader {
                shell: ShellKind::Sh,
                ..
            }
        ));
        for token in ["1", "2", "3.0", "'3'", "4"] {
            assert!(
                super::super::parse_pack_source_yaml(
                    text.replace("source_format: 3", &format!("source_format: {token}"))
                        .as_bytes()
                )
                .is_err(),
                "{token}"
            );
        }
        for changed in [
            text.replace("shell: sh", "shell: unknown"),
            text.replace("shell: sh", "shell: sh, shell: bash"),
            text.replace("script: script", "script: script, interpreter_args: []"),
            text.replace("shell: sh", "shell: null"),
        ] {
            assert!(super::super::parse_pack_source_yaml(changed.as_bytes()).is_err());
        }
    }
    const BASIC: &str = "source_format: 2\npackage_id: 00000000000000000000000000000065\nrevision:\n  service_storages: [{id: state}]\n  service_resources: [{id: config, storage_id: state, locator: Config.json, kind: file}]\nruntime_content: {}\n";

    // Test-ID: PR-TEST-0336
    // Verifies: PR-REQ-0320
    #[test]
    fn yaml_v2_defaults_are_typed_and_never_change_v1_projection() {
        let candidate = parse_pack_source_yaml_v2(BASIC.as_bytes()).unwrap();
        let r = &candidate.revision.resources()[0];
        assert_eq!(r.read_exposure, ServiceReadExposure::Hidden);
        assert_eq!(r.user_mutation, ServiceUserMutation::Unavailable);
        assert_eq!(r.locator.as_str(), "Config.json");
        assert!(candidate.runtime_sources.is_empty());
        assert!(parse_pack_source_yaml_v1(BASIC.as_bytes()).is_err());
        assert!(
            parse_pack_source_yaml_v2(
                BASIC
                    .replace("source_format: 2", "source_format: 1")
                    .as_bytes()
            )
            .is_err()
        );
        assert_eq!(
            serde_json::from_slice::<Value>(
                &encode_canonical_revision_core_v2(&candidate.revision).unwrap()
            )
            .unwrap()["format_version"],
            2
        );
        for bad in [
            BASIC.replace("source_format: 2", "source_format: 2.0"),
            BASIC.replace("source_format: 2", "source_format: '2'"),
            BASIC.replace("id: state", "id: !!str state"),
            BASIC.replace("id: state", "id: state, id: other"),
            BASIC.replace("kind: file", "kind: file, observed_presence: present"),
            BASIC.replace("Config.json", "../outside"),
            BASIC.replace("revision:", "revision:\n  format_version: 2"),
        ] {
            assert!(parse_pack_source_yaml_v2(bad.as_bytes()).is_err(), "{bad}");
        }
        let legacy=b"source_format: 1\npackage_id: 00000000000000000000000000000065\nrevision: {}\nruntime_content: {}\n";
        let v1 = parse_pack_source_yaml_v1(legacy).unwrap();
        assert_eq!(
            super::parse_pack_source_yaml(legacy)
                .unwrap()
                .revision
                .version(),
            1
        );
        assert_eq!(
            super::parse_pack_source_yaml(BASIC.as_bytes())
                .unwrap()
                .revision
                .version(),
            2
        );
        let bytes =
            crate::revision_core_v1::encode_canonical_revision_core_v1(&v1.revision).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            serde_json::json!({"format_version":1,"inputs":[],"actions":[],"migrations":[]})
        );
    }

    // Test-ID: PR-TEST-0337
    // Verifies: PR-REQ-0320, PR-REQ-0317
    #[test]
    fn yaml_v2_preserves_exact_numbers_and_supplies_only_declared_hook_defaults() {
        let actions = "  actions:\n    - id: inspect\n      access: observe\n      parameters: [{id: count, type: integer, sensitive: false, default: 9007199254740991}, {id: text, type: string, sensitive: false, default: null}]\n      hook:\n        protocol_version: 1\n        launch: {kind: direct, executable: tool}\n        args: []\n        io: {terminal: none}\n      outputs: []\n";
        let source = BASIC.replace(
            "  service_storages:",
            &format!("{actions}  service_storages:"),
        );
        let candidate = parse_pack_source_yaml_v2(source.as_bytes()).unwrap();
        let h = candidate.revision.hooks().values().next().unwrap();
        assert!(h.access.is_empty() && h.requires.is_empty());
        assert_eq!(
            candidate.revision.common().actions()[0].parameters[0].default,
            Some(ParameterDefaultV1::Integer(
                SafeIntegerV1::new(9007199254740991).unwrap()
            ))
        );
        assert_eq!(
            candidate.revision.common().actions()[0].parameters[1].default,
            Some(ParameterDefaultV1::String("null".into()))
        );
        assert!(
            parse_pack_source_yaml_v2(
                source
                    .replace("9007199254740991", "9007199254740992")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(
            parse_pack_source_yaml_v2(
                source
                    .replace("protocol_version: 1", "protocol_version: '1'")
                    .as_bytes()
            )
            .is_err()
        );
    }
}

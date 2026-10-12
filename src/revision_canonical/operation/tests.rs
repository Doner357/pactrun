use super::*;

fn common_source() -> serde_json::Value {
    use serde_json::json;
    let hook = json!({
        "launch": {"kind":"direct", "executable":"tool"},
        "args":[], "io":{"terminal":"none"}
    });
    json!({
        "source_format":"1.0-alpha.3",
        "package_id":"00000000000000000000000000000001",
        "runtime_content":{},
        "revision":{
            "snapshot":{
                "capture":{"access":"observe", "parameters":[], "hook":hook},
                "restore":{"parameters":[], "hook":hook}
            },
            "cleanup":{"hook":hook},
            "migrations":[{
                "source_revision_digest":format!("sha256:{}", "1".repeat(64)),
                "transitions":[], "produces_target":[], "hook":hook
            }]
        }
    })
}

fn parse_source(
    source: &serde_json::Value,
) -> Result<OperationRevision, crate::authoring::AuthoringError> {
    crate::authoring::operation::parse(&serde_json::to_vec(source).unwrap()).map(|c| c.revision)
}

// Test-ID: PR-TEST-0702
// Verifies: PR-REQ-0380
#[test]
fn common_capabilities_defaults_and_roundtrip() {
    let core = parse_source(&common_source()).unwrap();
    let bytes = encode(&core).unwrap();
    assert_eq!(decode(&bytes).unwrap(), core);
    let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for op in [
        &wire["snapshot"]["capture"],
        &wire["snapshot"]["restore"],
        &wire["cleanup"],
        &wire["migrations"][0],
    ] {
        assert_eq!(op["parameters"], serde_json::json!([]));
        assert_eq!(op["outputs"], serde_json::json!([]));
    }
    for path in ["capture", "restore"] {
        let mut invalid = wire.clone();
        invalid["snapshot"][path]
            .as_object_mut()
            .unwrap()
            .remove("outputs");
        assert_eq!(
            project(&serde_json::to_vec(&invalid).unwrap())
                .unwrap_err()
                .code(),
            "missing_field"
        );
    }
}

// Test-ID: PR-TEST-0703
// Verifies: PR-REQ-0380
#[test]
fn common_capabilities_normalize_and_reject_duplicates() {
    use serde_json::json;
    let mut source = common_source();
    for path in ["capture", "restore"] {
        source["revision"]["snapshot"][path]["outputs"] = json!([{"id":"z"},{"id":"a"}]);
    }
    for path in ["/revision/cleanup", "/revision/migrations/0"] {
        let op = source.pointer_mut(path).unwrap();
        op["outputs"] = json!([{"id":"z"},{"id":"a"}]);
        op["parameters"] = json!([
            {"id":"z", "type":"boolean", "sensitive":false, "default":false},
            {"id":"a", "type":"string", "sensitive":true}
        ]);
    }
    let core = parse_source(&source).unwrap();
    let bytes = encode(&core).unwrap();
    assert_eq!(decode(&bytes).unwrap(), core);
    let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(wire["cleanup"]["outputs"][0]["id"], "a");
    assert_eq!(wire["migrations"][0]["parameters"][0]["id"], "a");
    for path in [
        "/revision/snapshot/capture",
        "/revision/snapshot/restore",
        "/revision/cleanup",
        "/revision/migrations/0",
    ] {
        let mut invalid = source.clone();
        invalid.pointer_mut(path).unwrap()["outputs"] = json!([{"id":"a"},{"id":"a"}]);
        assert!(parse_source(&invalid).is_err(), "{path}");
    }
    for path in ["/revision/cleanup", "/revision/migrations/0"] {
        let mut invalid = source.clone();
        invalid.pointer_mut(path).unwrap()["parameters"] = json!([
            {"id":"a", "type":"boolean", "sensitive":false},
            {"id":"a", "type":"string", "sensitive":false}
        ]);
        assert!(parse_source(&invalid).is_err(), "{path}");
    }
}

// Test-ID: PR-TEST-0704
// Verifies: PR-REQ-0380
#[test]
fn withdrawn_restore_allowlist_and_hook_free_capabilities_are_rejected() {
    use serde_json::json;
    let mut source = common_source();
    source["revision"]["snapshot"]["restore"]["compatible_revisions"] = json!([]);
    assert!(parse_source(&source).is_err());
    let mut source = common_source();
    source["revision"]["migrations"][0]
        .as_object_mut()
        .unwrap()
        .remove("hook");
    assert!(parse_source(&source).is_ok());
    source["revision"]["migrations"][0]["outputs"] = json!([{"id":"report"}]);
    assert!(parse_source(&source).is_err());
    source["revision"]["migrations"][0]["outputs"] = json!([]);
    source["revision"]["migrations"][0]["parameters"] =
        json!([{"id":"mode","type":"string","sensitive":false}]);
    assert!(parse_source(&source).is_err());
}

#[test]
fn codec_bootstrap_empty_roundtrip() {
    let source = b"source_format: 1.0-alpha.3\npackage_id: 00000000000000000000000000000001\nrevision: {}\nruntime_content: {}\n";
    let candidate = crate::authoring::operation::parse(source).unwrap();
    let bytes = encode(&candidate.revision).unwrap();
    assert_eq!(decode(&bytes).unwrap(), candidate.revision);
    assert!(super::super::decode_canonical_service_revision(&bytes).is_err());
    let public = crate::authoring::parse_pack_source_yaml(source).unwrap();
    assert_eq!(public.revision.version().to_string(), OPERATION_CORE_FORMAT);
    assert_eq!(
        crate::revision_content::encode_canonical_revision_core(&public.revision).unwrap(),
        bytes
    );
}

// Test-ID: PR-TEST-0705
// Verifies: PR-REQ-0380
#[test]
fn independent_operation_vectors_match_core_and_content_identity() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/vectors/revision_canonical/operations.json"
    ))
    .unwrap();
    for vector in vectors["valid"].as_array().unwrap() {
        let name = vector["name"].as_str().unwrap();
        let core = project(&serde_json::to_vec(&vector["normalized_core"]).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let bytes = encode(&core).unwrap();
        assert_eq!(
            hex::encode(&bytes),
            vector["expected"]["core_jcs_hex"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(decode(&bytes).unwrap(), core, "{name}");
        let runtime = hex::decode(vector["expected"]["content_jcs_hex"].as_str().unwrap()).unwrap();
        let content = decode_content(&bytes, &runtime).unwrap();
        let facade = crate::revision_content::validate_revision_content(
            core.clone().into(),
            content.runtime_content().clone(),
        )
        .unwrap();
        assert_eq!(facade.core.version().to_string(), OPERATION_CORE_FORMAT);
        assert_eq!(
            crate::revision_content::encode_canonical_revision_core(&facade.core).unwrap(),
            bytes
        );
        assert_eq!(
            crate::revision_content::calculate_revision_content_digest(&facade).unwrap(),
            digest(&content).unwrap()
        );
        assert_eq!(
            hex::encode(frame_revision_content(&bytes, &runtime)),
            vector["expected"]["frame_hex"].as_str().unwrap(),
            "{name}"
        );
        let expected = format!(
            "sha256:{}",
            hex::encode(Sha256::digest(frame_revision_content(&bytes, &runtime)))
        );
        assert_eq!(
            expected,
            vector["expected"]["digest"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(
            digest(&content).unwrap(),
            RevisionContentDigest::from_bytes(
                Sha256::digest(frame_revision_content(&bytes, &runtime)).into()
            )
        );
    }
}

// Test-ID: PR-TEST-0706
// Verifies: PR-REQ-0380
#[test]
fn operation_schema_rejects_null_unknown_fields_and_invalid_parameter_types() {
    use serde_json::json;
    let source = common_source();
    for path in [
        "/revision/snapshot/capture",
        "/revision/snapshot/restore",
        "/revision/cleanup",
        "/revision/migrations/0",
    ] {
        for field in ["outputs", "parameters"] {
            let mut invalid = source.clone();
            invalid.pointer_mut(path).unwrap()[field] = serde_json::Value::Null;
            assert!(parse_source(&invalid).is_err(), "{path}/{field}");
        }
        let mut invalid = source.clone();
        invalid.pointer_mut(path).unwrap()["outputs"] = json!([{"id":"report", "required":false}]);
        assert!(parse_source(&invalid).is_err(), "{path}");
    }
    for value in [
        json!({"id":"mode", "type":"boolean", "sensitive":false, "default":"false"}),
        json!({"id":"mode", "type":"integer", "sensitive":false, "default":1.5}),
        json!({"id":"mode", "type":"integer", "sensitive":false, "default":9007199254740992_u64}),
        json!({"id":"mode", "type":"boolean", "sensitive":false, "default":null}),
        json!({"id":"mode", "type":"boolean", "sensitive":false, "unknown":false}),
    ] {
        for path in ["/revision/cleanup", "/revision/migrations/0"] {
            let mut invalid = source.clone();
            invalid.pointer_mut(path).unwrap()["parameters"] = json!([value]);
            assert!(parse_source(&invalid).is_err(), "{path}: {value}");
        }
    }
    let mut invalid = source.clone();
    invalid["revision"]["inputs"] = json!([{"id":"config", "required":false}]);
    assert!(parse_source(&invalid).is_err());
    let mut wire: serde_json::Value =
        serde_json::from_slice(&encode(&parse_source(&source).unwrap()).unwrap()).unwrap();
    let mut null_default = wire.clone();
    null_default["cleanup"]["parameters"] =
        json!([{"id":"mode", "type":"string", "sensitive":false, "default":null}]);
    assert!(project(&serde_json::to_vec(&null_default).unwrap()).is_err());
    // Source string slots preserve literal tokens; Core null is not a string.
    let mut literal = source.clone();
    literal["revision"]["cleanup"]["parameters"] =
        json!([{"id":"mode", "type":"string", "sensitive":false, "default":null}]);
    let core = parse_source(&literal).unwrap();
    assert_eq!(
        core.cleanup().unwrap().capabilities.parameters[0].default,
        Some(ParameterDefaultV1::String("null".into()))
    );
    wire["snapshot"]["restore"]["compatible_revisions"] = json!([]);
    assert_eq!(
        project(&serde_json::to_vec(&wire).unwrap())
            .unwrap_err()
            .code(),
        "unknown_field"
    );
}

// Test-ID: PR-TEST-0707
// Verifies: PR-REQ-0380
#[test]
fn operation_grants_are_closed_disjoint_sets_with_valid_roles() {
    use serde_json::json;
    let mut source = common_source();
    source["revision"]["inputs"] = json!([{"id":"config"}]);
    for path in ["/revision/snapshot/capture", "/revision/snapshot/restore"] {
        let mut valid = source.clone();
        valid.pointer_mut(path).unwrap()["inputs"] = json!({"optional":["config"]});
        assert!(parse_source(&valid).is_ok());
        for inputs in [
            json!({"required":["config"], "optional":["config"]}),
            json!({"required":["config","config"]}),
            json!({"optional":["unknown"]}),
            json!({"required":null}),
            json!({"extra":[]}),
        ] {
            let mut invalid = source.clone();
            invalid.pointer_mut(path).unwrap()["inputs"] = inputs.clone();
            assert!(parse_source(&invalid).is_err(), "{path}: {inputs}");
        }
    }
    for reference in [
        json!({"role":"retained","input_id":"config"}),
        json!({"role":"active","input_id":"unknown"}),
        json!({"role":"other","input_id":"config"}),
    ] {
        let mut invalid = source.clone();
        invalid["revision"]["cleanup"]["inputs"] = json!({"optional":[reference]});
        assert!(parse_source(&invalid).is_err(), "{reference}");
    }
    let mut invalid = source;
    invalid["revision"]["migrations"][0]["target_inputs"] = json!({"optional":["config"]});
    invalid["revision"]["migrations"][0]["produces_target"] = json!(["config"]);
    assert!(parse_source(&invalid).is_err());
}

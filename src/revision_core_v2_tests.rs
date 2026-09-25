use super::*;
use serde_json::{Value, json};

fn vectors() -> Value {
    serde_json::from_str(include_str!(
        "../tests/vectors/revision_core_format_v2/vectors.json"
    ))
    .unwrap()
}
fn example(name: &str) -> Value {
    vectors()["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == name)
        .unwrap()["input_core"]
        .clone()
}
fn project(value: &Value) -> Result<RevisionCoreV2, Error> {
    project_revision_core_source_v2(&serde_json::to_vec(value).unwrap())
}

// Test-ID: PR-TEST-0331
// Verifies: PR-REQ-0012, PR-REQ-0014, PR-REQ-0018, PR-REQ-0079, PR-REQ-0142, PR-REQ-0240, PR-REQ-0317, PR-REQ-0318
#[test]
fn checked_in_v2_vectors_match_canonical_components_frames_and_independent_digests() {
    for v in vectors()["valid"].as_array().unwrap() {
        let core = project(&v["input_core"]).unwrap_or_else(|e| panic!("{}: {e}", v["name"]));
        let bytes = encode_canonical_revision_core_v2(&core).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            v["normalized_core"]
        );
        assert_eq!(decode_canonical_revision_core_v2(&bytes).unwrap(), core);
        let content_bytes = serde_jcs::to_vec(&v["normalized_content"]).unwrap();
        let content = decode_canonical_revision_content_v2(&bytes, &content_bytes).unwrap();
        let actual = json!({"core_jcs_hex":hex::encode(&bytes),"content_jcs_hex":hex::encode(&content_bytes),
            "frame_hex":hex::encode(frame_revision_content_v2(&bytes,&content_bytes)),
            "digest":calculate_revision_content_digest_v2(&content).unwrap().to_string()});
        assert_eq!(actual, v["expected"], "{}", v["name"]);
        assert!(revision_core_v1::decode_canonical_revision_core_v1(&bytes).is_err());
        let mut spaced = vec![b' '];
        spaced.extend_from_slice(&bytes);
        assert_eq!(
            decode_canonical_revision_core_v2(&spaced)
                .unwrap_err()
                .code(),
            "noncanonical_json"
        );
    }
    let original = example("full");
    let canonical = encode_canonical_revision_core_v2(&project(&original).unwrap()).unwrap();
    let mut reordered = original.clone();
    for field in [
        "inputs",
        "actions",
        "migrations",
        "service_storages",
        "service_resources",
    ] {
        reordered[field].as_array_mut().unwrap().reverse();
    }
    for action in reordered["actions"].as_array_mut().unwrap() {
        for field in ["service_access", "service_requires"] {
            action["hook"][field].as_array_mut().unwrap().reverse();
        }
    }
    assert_eq!(
        encode_canonical_revision_core_v2(&project(&reordered).unwrap()).unwrap(),
        canonical
    );
    reordered["actions"][0]["hook"]["args"] = json!(["first", "second"]);
    let ordered = encode_canonical_revision_core_v2(&project(&reordered).unwrap()).unwrap();
    reordered["actions"][0]["hook"]["args"] = json!(["second", "first"]);
    assert_ne!(
        encode_canonical_revision_core_v2(&project(&reordered).unwrap()).unwrap(),
        ordered
    );
}

// Test-ID: PR-TEST-0333
// Verifies: PR-REQ-0016, PR-REQ-0079, PR-REQ-0317, PR-REQ-0318
#[test]
fn v2_raw_input_preserves_exact_numeric_unicode_and_duplicate_validation() {
    let invalid: Value = serde_json::from_str(include_str!(
        "../tests/vectors/revision_core_format_v2/invalid.json"
    ))
    .unwrap();
    for vector in invalid["invalid"].as_array().unwrap() {
        let input = vector["raw_core"].as_str().unwrap();
        assert_eq!(
            project_revision_core_source_v2(input.as_bytes())
                .unwrap_err()
                .code(),
            vector["error_code"].as_str().unwrap(),
            "{}",
            vector["name"]
        );
    }
    let minimal = serde_json::to_string(&example("empty")).unwrap();
    for (input, expected) in [
        (
            minimal.replacen(
                "\"format_version\":2",
                "\"format_version\":2,\"format_version\":2",
                1,
            ),
            "duplicate_property",
        ),
        (
            minimal.replacen(
                "\"format_version\":2",
                "\"format_version\":2.00000000000000001",
                1,
            ),
            "invalid_number",
        ),
        (
            minimal.replacen("\"format_version\":2", "\"format_version\":1", 1),
            "unsupported_format_version",
        ),
    ] {
        assert_eq!(
            project_revision_core_source_v2(input.as_bytes())
                .unwrap_err()
                .code(),
            expected
        );
    }
    let mut full = example("full");
    full["actions"][0]["parameters"][2]["default"] = json!(9007199254740992_u64);
    // Reverse input order makes count the last parameter in this fixture.
    assert!(project(&full).is_err());
    let bad=br#"{"actions":[],"format_version":2,"inputs":[],"migrations":[],"service_resources":[],"service_storages":[{"id":"\uD800"}]}"#;
    assert_eq!(
        project_revision_core_source_v2(bad).unwrap_err().code(),
        "invalid_unicode_scalar"
    );
    assert!(project_revision_core_source_v2(&[0xff]).is_err());
    let canonical =
        encode_canonical_revision_core_v2(&project(&example("empty")).unwrap()).unwrap();
    let content =
        revision_core_v1::decode_canonical_runtime_content_v1(br#"{"files":[]}"#).unwrap();
    let common = project(&example("empty")).unwrap().common().clone();
    let legacy = validate_revision_content_v1(common, content.clone()).unwrap();
    let v2 = validate_revision_content_v2(
        decode_canonical_revision_core_v2(&canonical).unwrap(),
        content,
    )
    .unwrap();
    assert_ne!(
        calculate_revision_content_digest_v2(&v2).unwrap(),
        revision_core_v1::calculate_revision_content_digest_v1(&legacy).unwrap()
    );
}

// Test-ID: PR-TEST-0334
// Verifies: PR-REQ-0317, PR-REQ-0319
#[test]
fn v2_intrinsics_reject_scope_exposure_mapping_and_locator_shortcuts() {
    for case in 0..13 {
        let mut v = example("full");
        // input_core resource order is database, config.
        match case {
            0=>v["service_resources"][0]["allocation_id"]=json!("not-core"),
            1=>v["service_resources"][0]["locator"]=json!("../outside"),
            2=>v["service_resources"][0]["locator"]=json!("NUL.bin"),
            3=>v["service_resources"][0]["locator"]=json!("config.JSON"),
            4=>v["service_resources"][0]["storage_id"]=json!("missing"),
            5=>v["actions"][0]["hook"]["protocol_version"]=json!(1),
            6=>v["actions"][0]["access"]=json!("observe"),
            7=>v["actions"][0]["hook"]["service_requires"][0]["reference"]["kind"]=json!("storage"),
            8=>v["actions"][0]["hook"]["service_access"][0]["reference"]["view"]=json!("target"),
            9=>v["service_resources"][1]["user_mutation"]["action_id"]=json!("missing"),
            10=>v["actions"][0]["hook"]["service_access"].as_array_mut().unwrap().push(json!({"reference":{"view":"current","role":"active","kind":"resource","id":"config"},"mode":"read"})),
            11=>v["service_resources"][0]["user_mutation"]=json!({"kind":"direct","action_id":"initialize"}),
            12=>{v["service_resources"][0]["kind"]=json!("file");v["service_resources"][1]["locator"]=json!("db/config");},
            _=>unreachable!(),
        }
        assert!(project(&v).is_err(), "intrinsic case {case}");
    }
    for case in 0..7 {
        let mut v = example("transform");
        let edge = &mut v["migrations"][0];
        match case {
            0 => edge["storage_transitions"] = json!([]),
            1 => edge["resource_transitions"] = json!([]),
            2 => {
                let duplicate = edge["resource_transitions"][0].clone();
                edge["resource_transitions"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
            3 => edge["resource_transitions"][0]["sources"] = json!([]),
            4 => {
                edge.as_object_mut().unwrap().remove("hook");
            }
            5 => edge["hook"]["service_access"]
                .as_array_mut()
                .unwrap()
                .retain(|g| g["reference"]["view"] != "target"),
            6 => edge["hook"]["service_access"][0]["reference"]["role"] = json!("retained"),
            _ => unreachable!(),
        }
        assert!(project(&v).is_err(), "mapping case {case}");
    }
    for path in ["DB/data.sqlite", ".config", "a-b/_cache", "a/b.c"] {
        assert!(ServiceLocatorV2::parse(path).is_ok());
    }
    for path in [
        "", "/a", "a//b", "a/../b", "a\\b", "a.", "a b", "資料", "C:/a", "a/LPT1",
    ] {
        assert!(ServiceLocatorV2::parse(path).is_err(), "{path}");
    }
}

// Test-ID: PR-TEST-0335
// Verifies: PR-REQ-0319
#[test]
fn v2_relational_checks_distinguish_explicit_rename_and_runtime_retained_facts() {
    let mut source_json = example("rename");
    source_json["migrations"] = json!([]);
    source_json["service_resources"][0]["id"] = json!("config");
    let source = project(&source_json).unwrap();
    let digest = Sha256Digest::parse(format!("sha256:{}", "1".repeat(64))).unwrap();
    let context = BTreeMap::from([(digest.clone(), ServiceSourceCore::V2(&source))]);
    let target = project(&example("rename")).unwrap();
    assert_eq!(
        validate_service_sources_v2(&target, &BTreeMap::new()).unwrap(),
        RelationalValidationV1::NotEvaluated
    );
    assert_eq!(
        validate_service_sources_v2(&target, &context).unwrap(),
        RelationalValidationV1::Valid
    );
    // A fresh declaration over the named source location is not a rename.
    let mut disguised = example("rename");
    disguised["migrations"][0]["resource_transitions"][0] =
        json!({"kind":"create","target_resource_id":"settings","presence":"any"});
    assert!(validate_service_sources_v2(&project(&disguised).unwrap(), &context).is_err());
    let mut moved = example("rename");
    moved["service_resources"][0]["locator"] = json!("elsewhere");
    assert!(validate_service_sources_v2(&project(&moved).unwrap(), &context).is_err());
    let transform = project(&example("transform")).unwrap();
    assert_eq!(
        validate_service_sources_v2(&transform, &context).unwrap(),
        RelationalValidationV1::Valid
    );
    let mut unreadable = example("transform");
    unreadable["migrations"][0]["hook"]["service_access"]
        .as_array_mut()
        .unwrap()
        .retain(|g| g["reference"]["view"] != "source");
    assert!(validate_service_sources_v2(&project(&unreadable).unwrap(), &context).is_err());
    let retained = project(&example("retained")).unwrap();
    assert!(
        validate_service_sources_v2(&retained, &context).is_err(),
        "active source cannot be called retained"
    );
    source_json["service_resources"] = json!([]);
    let no_active = project(&source_json).unwrap();
    let context = BTreeMap::from([(digest, ServiceSourceCore::V2(&no_active))]);
    assert_eq!(
        validate_service_sources_v2(&retained, &context).unwrap(),
        RelationalValidationV1::Valid,
        "format-role validity does not assert actual retained association availability"
    );
}

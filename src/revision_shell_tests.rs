#[cfg(test)]
mod shell_tests {
    use super::*;
    use crate::{revision_declarations, revision_canonical};
    fn project(bytes: &[u8]) -> Result<ServiceRevision, ServiceContractError> { project_service_core(bytes) }
    fn encode(core: &ServiceRevision) -> Result<Vec<u8>, ServiceContractError> { encode_service_core(core) }
    fn decode(bytes: &[u8]) -> Result<ServiceRevision, ServiceContractError> { decode_canonical_service_revision(bytes) }
    fn validate(core: ServiceRevision, runtime: RuntimeContentClosureIdentityV1) -> Result<ServiceRevisionContent, ServiceContractError> { validate_service_revision_content(core, runtime) }
    fn digest(core: &ServiceRevision, runtime: &RuntimeContentClosureIdentityV1) -> Result<RevisionContentDigest, ServiceContractError> { calculate_service_revision_digest(&ServiceRevisionContent { core: core.clone(), runtime_content: runtime.clone() }) }

    use serde_json::{Value, json};

    // Test-ID: PR-TEST-0493
    // Verifies: PR-REQ-0012, PR-REQ-0014, PR-REQ-0018, PR-REQ-0079, PR-REQ-0348
    #[test]
    fn valid_shell_vectors_match_independent_node_components_frame_and_digest() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../tests/vectors/revision_canonical/shell.json"
        ))
        .unwrap();
        let output = std::process::Command::new("node")
            .arg("tests/oracles/revision_canonical_shell.mjs")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
        let runtime_bytes = serde_jcs::to_vec(&fixture["normalized_content"]).unwrap();
        let runtime =
            revision_declarations::decode_canonical_runtime_content(&runtime_bytes).unwrap();
        for (index, vector) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let mut core = fixture["normalized_core"].clone();
            core["actions"][0]["hook"]["launch"]["shell"] = vector["shell"].clone();
            let core = project(&serde_json::to_vec(&core).unwrap()).unwrap();
            validate(core.clone(), runtime.clone()).unwrap();
            let bytes = encode(&core).unwrap();
            let mut frame = b"pactrun.revision-content-digest\0".to_vec();
            frame.extend(b"revision-core\0");
            frame.extend((bytes.len() as u64).to_be_bytes());
            frame.extend(&bytes);
            frame.extend(b"runtime-content-closure\0");
            frame.extend((runtime_bytes.len() as u64).to_be_bytes());
            frame.extend(&runtime_bytes);
            assert_eq!(hex::encode(bytes), oracle[index]["core_jcs_hex"]);
            assert_eq!(
                hex::encode(&runtime_bytes),
                oracle[index]["content_jcs_hex"]
            );
            assert_eq!(hex::encode(frame), oracle[index]["frame_hex"]);
            assert_eq!(
                digest(&core, &runtime).unwrap().to_string(),
                vector["digest"]
            );
            assert_eq!(vector["digest"], oracle[index]["digest"]);
        }
    }

    fn core(shell: &str) -> Value {
        json!({"format_version":"1.0-alpha.1","inputs":[],"migrations":[],"service_storages":[],"service_resources":[],
            "actions":[{"id":"run","access":"observe","parameters":[],"outputs":[],"hook":{
                "protocol_version":"1.0-alpha.1","launch":{"kind":"shell_loader","shell":shell,"command":"shell","script":"script"},
                "args":[],"io":{"terminal":"none"},"service_access":[],"service_requires":[]}}]})
    }

    // Test-ID: PR-TEST-0487
    // Verifies: PR-REQ-0348
    #[test]
    fn shell_launch_is_explicit_closed_and_identity_bearing() {
        for shell in ["sh", "bash", "powershell_7", "windows_powershell_5_1"] {
            let input = core(shell);
            let parsed = project(&serde_json::to_vec(&input).unwrap()).unwrap();
            let encoded = encode(&parsed).unwrap();
            assert_eq!(decode(&encoded).unwrap(), parsed);
            assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), input);
            for version in [1, 2] {
                let mut old = input.clone();
                old["format_version"] = version.into();
                assert!(
                    revision_canonical::project_service_revision_source(
                        &serde_json::to_vec(&old).unwrap()
                    )
                    .is_err()
                );
                assert!(
                    revision_declarations::project_service_free_revision_source(
                        &serde_json::to_vec(&old).unwrap()
                    )
                    .is_err()
                );
            }
            for (field, replacement) in [
                ("shell", json!("other")),
                ("interpreter_args", json!([])),
                ("extra", json!(true)),
                ("script", Value::Null),
            ] {
                let mut bad = input.clone();
                bad["actions"][0]["hook"]["launch"][field] = replacement;
                assert!(
                    project(&serde_json::to_vec(&bad).unwrap()).is_err(),
                    "{field}"
                );
            }
            let empty_runtime =
                revision_declarations::decode_canonical_runtime_content(br#"{"files":[]}"#).unwrap();
            assert!(validate(parsed.clone(), empty_runtime.clone()).is_err());
            let original = digest(&parsed, &empty_runtime).unwrap();
            for (field, replacement) in
                [("command", json!("other")), ("script", json!("different"))]
            {
                let mut changed = input.clone();
                changed["actions"][0]["hook"]["launch"][field] = replacement;
                let changed = project(&serde_json::to_vec(&changed).unwrap()).unwrap();
                assert_ne!(digest(&changed, &empty_runtime).unwrap(), original);
            }
            assert_eq!(revision_canonical::encode_canonical_service_revision(&parsed).unwrap(), encoded);
            assert!(
                revision_declarations::encode_service_free_revision(parsed.common()).is_ok()
            );
        }
        let duplicate = serde_json::to_string(&core("sh"))
            .unwrap()
            .replace("\"shell\":\"sh\"", "\"shell\":\"sh\",\"shell\":\"bash\"");
        assert!(project(duplicate.as_bytes()).is_err());
    }
}

//! Explicit Core V3 codec; no changes to Frozen Core V1/V2 byte contracts.

use crate::{domain::*, revision_core_v1, revision_core_v2};
use sha2::{Digest, Sha256};

pub(crate) fn project(bytes: &[u8]) -> Result<RevisionCoreV3, ServiceContractError> {
    revision_core_v2::project_service_core(bytes, 3).map(RevisionCoreV3)
}

pub(crate) fn encode(core: &RevisionCoreV3) -> Result<Vec<u8>, ServiceContractError> {
    revision_core_v2::encode_service_core(&core.0, 3)
}

pub(crate) fn decode(bytes: &[u8]) -> Result<RevisionCoreV3, ServiceContractError> {
    let core = project(bytes)?;
    if encode(&core)? != bytes {
        return Err(ServiceContractError::new(
            "noncanonical_json",
            "Core V3 bytes are not exact JCS",
        ));
    }
    Ok(core)
}

pub(crate) fn validate(
    core: RevisionCoreV3,
    runtime: RuntimeContentClosureIdentityV1,
) -> Result<ValidatedRevisionContent, ServiceContractError> {
    project_revision_content_v2(core.0.clone(), runtime.clone())?;
    Ok(ValidatedRevisionContent {
        core: core.into(),
        runtime_content: runtime,
    })
}

pub(crate) fn digest(
    core: &RevisionCoreV3,
    runtime: &RuntimeContentClosureIdentityV1,
) -> Result<RevisionContentDigest, ServiceContractError> {
    let core = encode(core)?;
    let runtime = revision_core_v1::encode_canonical_runtime_content_v1(runtime)?;
    let mut hash = Sha256::new();
    hash.update(b"pactrun.revision-content-digest\0");
    hash.update(3_u32.to_be_bytes());
    hash.update(b"revision-core\0");
    hash.update((core.len() as u64).to_be_bytes());
    hash.update(core);
    hash.update(b"runtime-content-closure\0");
    hash.update((runtime.len() as u64).to_be_bytes());
    hash.update(runtime);
    Ok(RevisionContentDigest::from_bytes(hash.finalize().into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    // Test-ID: PR-TEST-0493
    // Verifies: PR-REQ-0348
    #[test]
    fn valid_shell_vectors_match_independent_node_components_frame_and_digest() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../tests/vectors/revision_core_format_v3/launch.json"
        ))
        .unwrap();
        let output = std::process::Command::new("node")
            .arg("tests/oracles/revision_core_format_v3.mjs")
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
            revision_core_v1::decode_canonical_runtime_content_v1(&runtime_bytes).unwrap();
        for (index, vector) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let mut core = fixture["normalized_core"].clone();
            core["actions"][0]["hook"]["launch"]["shell"] = vector["shell"].clone();
            let core = project(&serde_json::to_vec(&core).unwrap()).unwrap();
            validate(core.clone(), runtime.clone()).unwrap();
            let bytes = encode(&core).unwrap();
            let mut frame = b"pactrun.revision-content-digest\0".to_vec();
            frame.extend(3_u32.to_be_bytes());
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
        json!({"format_version":3,"inputs":[],"migrations":[],"service_storages":[],"service_resources":[],
            "actions":[{"id":"run","access":"observe","parameters":[],"outputs":[],"hook":{
                "protocol_version":1,"launch":{"kind":"shell_loader","shell":shell,"command":"shell","script":"script"},
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
                    revision_core_v2::project_revision_core_source_v2(
                        &serde_json::to_vec(&old).unwrap()
                    )
                    .is_err()
                );
                assert!(
                    revision_core_v1::project_revision_core_source_v1(
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
                revision_core_v1::decode_canonical_runtime_content_v1(br#"{"files":[]}"#).unwrap();
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
            assert!(revision_core_v2::encode_canonical_revision_core_v2(&parsed.0).is_err());
            assert!(
                revision_core_v1::encode_canonical_revision_core_v1(parsed.0.common()).is_err()
            );
        }
        let duplicate = serde_json::to_string(&core("sh"))
            .unwrap()
            .replace("\"shell\":\"sh\"", "\"shell\":\"sh\",\"shell\":\"bash\"");
        assert!(project(duplicate.as_bytes()).is_err());
    }
}

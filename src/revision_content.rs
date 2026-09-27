//! Exact supported Revision canonical boundary; ordering never grants codec support.
use crate::{
    domain::*,
    revision_canonical as canonical, revision_declarations as common,
    strict_json::{self, RawJsonValue},
};
use std::fmt;
#[derive(Debug)]
pub(crate) enum RevisionContentError {
    Declarations(RevisionError),
    Contract(ServiceContractError),
    Envelope(String),
}
impl fmt::Display for RevisionContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Declarations(e) => e.fmt(f),
            Self::Contract(e) => e.fmt(f),
            Self::Envelope(e) => f.write_str(e),
        }
    }
}
impl std::error::Error for RevisionContentError {}
impl From<RevisionError> for RevisionContentError {
    fn from(e: RevisionError) -> Self {
        Self::Declarations(e)
    }
}
impl From<ServiceContractError> for RevisionContentError {
    fn from(e: ServiceContractError) -> Self {
        Self::Contract(e)
    }
}
pub(crate) fn validate_revision_content(
    core: RevisionCore,
    runtime: RuntimeContentClosureIdentityV1,
) -> Result<ValidatedRevisionContent, RevisionContentError> {
    Ok(validate_service_content(core.0, runtime)?.into())
}
pub(crate) fn core_format_version(bytes: &[u8]) -> Result<FormatVersion, RevisionContentError> {
    let value = strict_json::parse_json(bytes, 16 * 1024 * 1024)
        .map_err(|e| RevisionContentError::Envelope(e.to_string()))?;
    let RawJsonValue::Object(entries) = value else {
        return Err(RevisionContentError::Envelope(
            "Core must be an object".into(),
        ));
    };
    let Some((_, RawJsonValue::String(version))) =
        entries.into_iter().find(|(key, _)| key == "format_version")
    else {
        return Err(RevisionContentError::Envelope(
            "Core requires string format_version".into(),
        ));
    };
    VersionDomain::Revision
        .require(&version)
        .map_err(RevisionContentError::Envelope)
}
pub(crate) fn decode_canonical_revision_core(
    bytes: &[u8],
) -> Result<RevisionCore, RevisionContentError> {
    core_format_version(bytes)?;
    Ok(canonical::decode_canonical_service_revision(bytes)?.into())
}
pub(crate) fn decode_canonical_revision_content(
    core: &[u8],
    runtime: &[u8],
) -> Result<ValidatedRevisionContent, RevisionContentError> {
    validate_revision_content(
        decode_canonical_revision_core(core)?,
        common::decode_canonical_runtime_content(runtime)?,
    )
}
pub(crate) fn encode_canonical_revision_core(
    core: &RevisionCore,
) -> Result<Vec<u8>, RevisionContentError> {
    Ok(canonical::encode_service_core(&core.0)?)
}
pub(crate) fn calculate_revision_content_digest(
    content: &ValidatedRevisionContent,
) -> Result<RevisionContentDigest, RevisionContentError> {
    Ok(canonical::calculate_service_revision_digest(
        &ServiceRevisionContent {
            core: content.core.0.clone(),
            runtime_content: content.runtime_content.clone(),
        },
    )?)
}
#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0338
    // Verifies: PR-REQ-0018, PR-REQ-0077, PR-REQ-0318
    #[test]
    fn exact_baseline_roundtrips_and_refuses_development_or_unknown_versions() {
        let bytes = br#"{"actions":[],"format_version":"1.0-alpha.1","inputs":[],"migrations":[],"service_resources":[],"service_storages":[]}"#;
        let content = decode_canonical_revision_content(bytes, br#"{"files":[]}"#).unwrap();
        assert_eq!(content.core.version(), VersionDomain::Revision.current());
        assert_eq!(
            encode_canonical_revision_core(&content.core).unwrap(),
            bytes
        );
        for version in [
            "1",
            "2",
            "3",
            "0",
            "\"1.0\"",
            "\"1.0-alpha.2\"",
            "\"1.0-alpha.01\"",
        ] {
            let bad = String::from_utf8(bytes.to_vec())
                .unwrap()
                .replace("\"1.0-alpha.1\"", version);
            assert!(core_format_version(bad.as_bytes()).is_err());
            assert!(decode_canonical_revision_content(bad.as_bytes(), br#"{"files":[]}"#).is_err());
        }
        assert!(
            core_format_version(
                br#"{"format_version":"1.0-alpha.1","format_version":"1.0-alpha.1"}"#
            )
            .is_err()
        );
        assert!(decode_canonical_revision_content(bytes, br#"{"files":[],"extra":0}"#).is_err());
    }
}

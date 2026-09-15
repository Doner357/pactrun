//! Strict codec dispatch. This does not grant a schema/version publishing
//! capability; persistence must enforce its own format activation boundary.
#![allow(dead_code)]

use crate::{
    domain::*,
    revision_core_v1 as v1, revision_core_v2 as v2,
    strict_json::{self, RawJsonValue},
};
use std::fmt;

#[derive(Debug)]
pub(crate) enum RevisionContentError {
    V1(RevisionCoreV1Error),
    V2(ServiceContractError),
    Envelope(&'static str),
}
impl fmt::Display for RevisionContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V1(e) => e.fmt(f),
            Self::V2(e) => e.fmt(f),
            Self::Envelope(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for RevisionContentError {}
impl From<RevisionCoreV1Error> for RevisionContentError {
    fn from(e: RevisionCoreV1Error) -> Self {
        Self::V1(e)
    }
}
impl From<ServiceContractError> for RevisionContentError {
    fn from(e: ServiceContractError) -> Self {
        Self::V2(e)
    }
}

pub(crate) fn validate_revision_content(
    core: RevisionCore,
    runtime: RuntimeContentClosureIdentityV1,
) -> Result<ValidatedRevisionContent, RevisionContentError> {
    match core {
        RevisionCore::V1(core) => {
            Ok(crate::domain::validate_revision_content_v1(*core, runtime)?.into())
        }
        RevisionCore::V2(core) => {
            Ok(crate::domain::project_revision_content_v2(*core, runtime)?.into())
        }
    }
}

pub(crate) fn core_format_version(bytes: &[u8]) -> Result<u8, RevisionContentError> {
    let value = strict_json::parse_json(bytes, 16 * 1024 * 1024)
        .map_err(|_| RevisionContentError::Envelope("invalid strict Core JSON"))?;
    let RawJsonValue::Object(entries) = value else {
        return Err(RevisionContentError::Envelope("Core must be an object"));
    };
    let version = entries
        .into_iter()
        .find(|(k, _)| k == "format_version")
        .ok_or(RevisionContentError::Envelope(
            "missing Core format_version",
        ))?
        .1;
    match v1::exact_integer(version)? {
        1 => Ok(1),
        2 => Ok(2),
        _ => Err(RevisionContentError::Envelope(
            "unsupported Core format version",
        )),
    }
}
pub(crate) fn decode_canonical_revision_content(
    core: &[u8],
    runtime: &[u8],
) -> Result<ValidatedRevisionContent, RevisionContentError> {
    match core_format_version(core)? {
        1 => Ok(v1::decode_canonical_revision_content_v1(core, runtime)?.into()),
        2 => Ok(v2::decode_canonical_revision_content_v2(core, runtime)?.into()),
        _ => unreachable!("closed dispatcher"),
    }
}

pub(crate) fn decode_canonical_revision_core(
    bytes: &[u8],
) -> Result<RevisionCore, RevisionContentError> {
    match core_format_version(bytes)? {
        1 => Ok(v1::decode_canonical_revision_core_v1(bytes)?.into()),
        2 => Ok(v2::decode_canonical_revision_core_v2(bytes)?.into()),
        _ => unreachable!("closed dispatcher"),
    }
}
pub(crate) fn encode_canonical_revision_core(
    core: &RevisionCore,
) -> Result<Vec<u8>, RevisionContentError> {
    match core {
        RevisionCore::V1(c) => Ok(v1::encode_canonical_revision_core_v1(c)?),
        RevisionCore::V2(c) => Ok(v2::encode_canonical_revision_core_v2(c)?),
    }
}
pub(crate) fn calculate_revision_content_digest(
    content: &ValidatedRevisionContent,
) -> Result<RevisionContentDigest, RevisionContentError> {
    match &content.core {
        RevisionCore::V1(c) => Ok(v1::calculate_revision_content_digest_v1(
            &ValidatedRevisionContentV1 {
                core: c.as_ref().clone(),
                runtime_content: content.runtime_content.clone(),
            },
        )?),
        RevisionCore::V2(c) => Ok(v2::calculate_revision_content_digest_v2(
            &ValidatedRevisionContentV2 {
                core: c.as_ref().clone(),
                runtime_content: content.runtime_content.clone(),
            },
        )?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0338
    // Verifies: PR-REQ-0318
    #[test]
    fn dispatch_is_explicit_and_preserves_v1_and_candidate_v2_bytes() {
        let runtime = br#"{"files":[]}"#;
        let v1_bytes = br#"{"actions":[],"format_version":1,"inputs":[],"migrations":[]}"#;
        let v2_bytes=br#"{"actions":[],"format_version":2,"inputs":[],"migrations":[],"service_resources":[],"service_storages":[]}"#;
        let first = decode_canonical_revision_content(v1_bytes, runtime).unwrap();
        let second = decode_canonical_revision_content(v2_bytes, runtime).unwrap();
        assert_eq!(first.core.version(), 1);
        assert_eq!(second.core.version(), 2);
        assert_eq!(
            encode_canonical_revision_core(&first.core).unwrap(),
            v1_bytes
        );
        assert_eq!(
            encode_canonical_revision_core(&second.core).unwrap(),
            v2_bytes
        );
        assert_ne!(
            calculate_revision_content_digest(&first).unwrap(),
            calculate_revision_content_digest(&second).unwrap()
        );
        assert!(core_format_version(br#"{"format_version":2,"format_version":1}"#).is_err());
        assert!(core_format_version(br#"{"format_version":2.000000000000001}"#).is_err());
        assert!(core_format_version(br#"{"format_version":3}"#).is_err());
        assert!(decode_canonical_revision_content(v2_bytes, br#"{"files":[],"extra":0}"#).is_err());
    }
}

//! Codec-only Source reader. No acquisition, installation or execution dispatch.
#![allow(dead_code)]
use super::*;
use crate::domain::{OPERATION_SOURCE_FORMAT, OperationRevision};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OperationSourceCandidate {
    pub(crate) package_id: PackageId,
    pub(crate) revision: OperationRevision,
    pub(crate) runtime_sources: Vec<RuntimeSourceRecordV1>,
    pub(crate) portable_metadata: PortableMetadataTemplate,
}

pub(crate) fn parse(bytes: &[u8]) -> Result<OperationSourceCandidate, AuthoringError> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| AuthoringError::new("Pack source must be UTF-8"))?;
    reject_forbidden_tokens(text)?;
    let mut root = closed_map(
        parse_document(text)?,
        &["source_format", "package_id", "revision", "runtime_content"],
        &["portable_metadata"],
        "Pack source",
    )?;
    if string_scalar(take(&mut root, "source_format")?)? != OPERATION_SOURCE_FORMAT {
        return Err(AuthoringError::new("expected operation Source 1.0-alpha.3"));
    }
    let package_id = PackageId::from_str(&string_scalar(take(&mut root, "package_id")?)?)
        .map_err(AuthoringError::new)?;
    let json = source_projection::operation_revision_json(text, take(&mut root, "revision")?)?;
    let bytes = serde_json::to_vec(&json).map_err(|e| AuthoringError::new(e.to_string()))?;
    let revision = crate::revision_canonical::operation::project(&bytes)
        .map_err(|e| AuthoringError::new(format!("project operation Revision: {e}")))?;
    let runtime_sources = runtime_sources(take(&mut root, "runtime_content")?)?;
    let portable_metadata = root
        .remove("portable_metadata")
        .map(portable_metadata)
        .transpose()?
        .unwrap_or_default();
    for presentation in &portable_metadata.presentation {
        if !revision.contains_presentation_target(&presentation.target) {
            return Err(AuthoringError::new(
                "presentation target is not declared by the operation Revision",
            ));
        }
    }
    Ok(OperationSourceCandidate {
        package_id,
        revision,
        runtime_sources,
        portable_metadata,
    })
}

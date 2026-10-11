//! Explicit user naming commands. Resolve once and mutate exact identity.
use super::*;
use crate::domain::{LocalName, LocalRevisionReference, LocalSelector};
use serde::Serialize;

pub(super) enum NameCommand {
    Package {
        reference: LocalSelector,
        name: Option<LocalName>,
    },
    Revision {
        reference: LocalRevisionReference,
        name: Option<LocalName>,
    },
}
impl NameCommand {
    pub(super) fn command_name(&self) -> &'static str {
        match self {
            Self::Package { name: Some(_), .. } => "package rename",
            Self::Package { name: None, .. } => "package unname",
            Self::Revision { name: Some(_), .. } => "revision rename",
            Self::Revision { name: None, .. } => "revision unname",
        }
    }
}
pub(super) fn parse_name(value: String) -> Result<LocalName, CliError> {
    LocalName::parse(value).map_err(|e| CliError::usage(e.to_string()))
}
pub(super) fn parse_package(parser: &mut Parser) -> Result<Command, CliError> {
    let operation = required_value_string(parser, "Package command")?;
    if !matches!(operation.as_str(), "rename" | "unname") {
        return Err(CliError::usage("Package command must be rename or unname"));
    }
    let reference = LocalSelector::parse(required_value_string(parser, "Package reference")?)
        .map_err(|e| CliError::usage(e.to_string()))?;
    let name = if operation == "rename" {
        Some(parse_name(required_value_string(parser, "new name")?)?)
    } else {
        None
    };
    require_end(parser)?;
    Ok(Command::Name(NameCommand::Package { reference, name }))
}
pub(super) fn parse_revision(parser: &mut Parser, rename: bool) -> Result<Command, CliError> {
    let reference =
        LocalRevisionReference::parse(&required_value_string(parser, "Revision reference")?)
            .map_err(|e| CliError::usage(e.to_string()))?;
    let name = if rename {
        Some(parse_name(required_value_string(parser, "new name")?)?)
    } else {
        None
    };
    require_end(parser)?;
    Ok(Command::Name(NameCommand::Revision { reference, name }))
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct NameResult {
    package_id: String,
    revision_content_digest: Option<String>,
    name: Option<String>,
}
pub(super) fn execute(
    command: NameCommand,
    root: &Path,
    format: reply::OutputContext,
) -> Result<(), CliError> {
    let reader = PactrunApplication::open_read_only(root).map_err(app_error)?;
    let label = command.command_name();
    let result = match command {
        NameCommand::Package { reference, name } => {
            let id = reader
                .resolve_named_package(&reference)
                .map_err(app_error)?;
            let writer = PactrunApplication::open(root).map_err(app_error)?;
            writer
                .rename_package(id, name.as_ref())
                .map_err(app_error)?;
            NameResult {
                package_id: id.to_string(),
                revision_content_digest: None,
                name: name.map(|n| n.as_str().to_owned()),
            }
        }
        NameCommand::Revision { reference, name } => {
            let id = reader
                .resolve_named_revision(&reference)
                .map_err(app_error)?;
            let writer = PactrunApplication::open(root).map_err(app_error)?;
            writer
                .rename_revision(&id, name.as_ref())
                .map_err(app_error)?;
            NameResult {
                package_id: id.package_id.to_string(),
                revision_content_digest: Some(id.content_digest.to_string()),
                name: name.map(|n| n.as_str().to_owned()),
            }
        }
    };
    presentation::emit_result(format, label, &result)
}

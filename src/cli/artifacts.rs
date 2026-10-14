use super::*;
use crate::domain::{ManagedOutputIdentity, ManagedRunIdentity, MigrationEdgeIdentity};

pub(super) struct EdgeSelector {
    source: Selector<RevisionContentDigest>,
    target: Selector<RevisionContentDigest>,
}
impl EdgeSelector {
    fn full(self) -> MigrationEdgeIdentity {
        MigrationEdgeIdentity {
            source: self.source.full(),
            target: self.target.full(),
        }
    }
    fn matches(&self, source: RevisionContentDigest, target: RevisionContentDigest) -> bool {
        let matches = |selector: &Selector<RevisionContentDigest>, value: RevisionContentDigest| {
            match selector {
                Selector::Full(id) => *id == value,
                Selector::Prefix(prefix) => value
                    .to_string()
                    .trim_start_matches("sha256:")
                    .starts_with(prefix),
            }
        };
        matches(&self.source, source) && matches(&self.target, target)
    }
}

pub(super) enum ArtifactCommand {
    Export {
        edge: Option<EdgeSelector>,
        run: Selector<RunId>,
        output: ManagedOutputIdentity,
        destination: PathBuf,
        authorized: bool,
    },
    Delete {
        edge: Option<EdgeSelector>,
        run: Selector<RunId>,
        output: ManagedOutputIdentity,
    },
}

impl ArtifactCommand {
    pub(super) fn resolve_ids(&mut self, r: &mut short_ids::Resolver<'_>) -> Result<(), CliError> {
        let (run, edge) = match self {
            Self::Export { run, edge, .. } | Self::Delete { run, edge, .. } => (run, edge),
        };
        run.resolve(r)?;
        if let Some(edge) = edge {
            let inspection = r
                .app()?
                .managed_run_inspection(run.clone().full())
                .map_err(app_error)?
                .ok_or_else(|| CliError::operation("parent Run is not persisted"))?;
            let ManagedRunIdentity::Migration(invocation) = inspection.run.operation else {
                return Err(CliError::operation("--edge requires a Migration Run"));
            };
            let matching: Vec<_> = invocation
                .path()
                .windows(2)
                .filter(|pair| edge.matches(pair[0].content_digest, pair[1].content_digest))
                .collect();
            if matching.len() != 1 {
                return Err(CliError::operation(
                    "artifact edge is absent or ambiguous on this Run; provide exact source and target digests",
                ));
            }
            edge.source = Selector::Full(matching[0][0].content_digest);
            edge.target = Selector::Full(matching[0][1].content_digest);
        }
        Ok(())
    }
}

pub(super) fn parse(parser: &mut Parser) -> Result<ArtifactCommand, CliError> {
    let operation = required_value_string(parser, "Artifact command")?;
    if operation != "export" && operation != "delete" {
        return Err(CliError::usage("unknown Artifact command"));
    }
    let run = parse_run_id(&value_string(parser, "RunId")?)?;
    let output = ManagedOutputIdentity::parse(required_value_string(parser, "output identity")?)
        .map_err(|_| CliError::usage("invalid output identity"))?;
    let mut edge = None;
    let mut destination = None;
    let mut authorized = false;
    while let Some(argument) = parser.next().map_err(lex_error)? {
        match argument {
            Arg::Long("edge") => {
                let value = value_string(parser, "source/target Revision digests")?;
                let (source, target) = value.split_once('/').ok_or_else(|| {
                    CliError::usage("--edge requires <source-digest>/<target-digest>")
                })?;
                set_once(
                    &mut edge,
                    EdgeSelector {
                        source: source.parse()?,
                        target: target.parse()?,
                    },
                    "--edge",
                )?;
            }
            Arg::Long("output") if operation == "export" => {
                let path = required_parser_value(parser, "output path")?;
                if path == "-" {
                    return Err(CliError::usage(
                        "Artifact export requires a file destination",
                    ));
                }
                set_once(&mut destination, PathBuf::from(path), "--output")?;
            }
            Arg::Long("authorize-sensitive-export") if operation == "export" && !authorized => {
                authorized = true
            }
            _ => return Err(CliError::usage("unsupported or duplicate Artifact option")),
        }
    }
    if operation == "delete" {
        return Ok(ArtifactCommand::Delete { run, output, edge });
    }
    Ok(ArtifactCommand::Export {
        edge,
        run,
        output,
        destination: destination.ok_or_else(|| CliError::usage("missing --output"))?,
        authorized,
    })
}

#[cfg(test)]
#[path = "artifact_tests.rs"]
mod tests;

pub(super) fn execute(
    command: ArtifactCommand,
    root: &Path,
    format: reply::OutputContext,
    trace: &std::sync::Arc<crate::trace::Trace>,
) -> Result<(), CliError> {
    let scoped = match &command {
        ArtifactCommand::Export { edge, .. } | ArtifactCommand::Delete { edge, .. } => {
            edge.is_some()
        }
    };
    if scoped && format.version() == machine::Version::Legacy {
        return Err(CliError::operation(
            "machine interface 1.0-alpha.3 cannot represent edge-scoped artifacts; select --machine-version 1.0-alpha.4",
        ));
    }
    if matches!(
        command,
        ArtifactCommand::Export {
            authorized: false,
            ..
        }
    ) {
        return Err(CliError::operation(
            "Artifact export requires --authorize-sensitive-export",
        ));
    }
    let application = PactrunApplication::open(root).map_err(app_error)?;
    match command {
        ArtifactCommand::Export {
            edge,
            run,
            output,
            destination,
            authorized,
        } => {
            let run = run.full();
            let edge = edge.map(EdgeSelector::full);
            let exported = match edge.as_ref() {
                Some(edge) => {
                    application.export_run_artifact_at(run, &output, Some(edge), authorized)
                }
                None => application.export_run_artifact(run, &output, authorized),
            };
            let staged = exported.map_err(|error| match error {
                ApplicationError::Persistence(
                    crate::persistence::PersistenceError::MissingRun(_),
                ) => CliError::operation("parent Run is not persisted"),
                _ => CliError::operation(
                    "Artifact export failed; check the Run, published output and storage",
                ),
            })?;
            crate::output_publication::publish_reported_traced(&staged, &destination, trace)
                .map_err(|error| {
                    let message = if error.destination_published {
                        "Artifact destination was published but durability could not be confirmed; the file may already exist and was not removed"
                    } else if error.source.kind() == io::ErrorKind::AlreadyExists {
                        "Artifact export destination already exists; it was not overwritten"
                    } else {
                        "Artifact export destination could not be published"
                    };
                    let mut result = publication_error(error, &destination);
                    result.message = message.into();
                    result
                })?;
            presentation::emit_result(
                format,
                "run artifact export",
                &presentation::ArtifactResult {
                    edge: edge.as_ref().map(Into::into),
                    run_id: run.to_string(),
                    output_id: output.as_str().into(),
                    outcome: "exported",
                },
            )
        }
        ArtifactCommand::Delete { run, output, edge } => {
            let run = run.full();
            let edge = edge.map(EdgeSelector::full);
            let deleted = application
                .delete_run_artifact_at(run, &output, edge.as_ref())
                .map_err(|error| match error {
                    ApplicationError::Persistence(
                        crate::persistence::PersistenceError::MissingRun(_),
                    ) => CliError::operation("parent Run is not persisted"),
                    _ => CliError::operation(
                        "Artifact deletion failed; check that storage is writable",
                    ),
                })?;
            presentation::emit_result(
                format,
                "run artifact delete",
                &presentation::ArtifactResult {
                    edge: edge.as_ref().map(Into::into),
                    run_id: run.to_string(),
                    output_id: output.as_str().into(),
                    outcome: if deleted { "deleted" } else { "already_absent" },
                },
            )
        }
    }
}

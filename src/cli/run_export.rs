use super::*;
use crate::domain::RunExportOptions;
use serde::Serialize;
mod archive;
mod errors;
mod manifest;

pub(super) struct Request {
    pub(super) run: Selector<RunId>,
    destination: PathBuf,
    options: RunExportOptions,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Exported {
    run_id: String,
    outcome: &'static str,
    copied_inputs: usize,
    unavailable_inputs: usize,
    copied_artifacts: usize,
    review_before_sharing: bool,
}
pub(super) fn parse(parser: &mut Parser) -> Result<Request, CliError> {
    let run = parse_run_id(&value_string(parser, "RunId")?)?;
    let mut destination = None;
    let mut options = RunExportOptions::default();
    let mut seen = BTreeSet::new();
    while let Some(argument) = parser.next().map_err(lex_error)? {
        let Arg::Long(key) = argument else {
            return Err(CliError::usage("unsupported Run export argument"));
        };
        if !seen.insert(key.to_owned()) {
            return Err(CliError::usage("duplicate Run export option"));
        }
        match key {
            "output" => {
                let path = required_parser_value(parser, "output path")?;
                if path.is_empty() || path == "-" {
                    return Err(CliError::usage("Run export requires a file destination"));
                }
                destination = Some(PathBuf::from(path));
            }
            "include-hook-text" => options.hook_text = true,
            "include-artifacts" => options.artifacts = true,
            "include-inputs" => options.inputs = true,
            "include-secrets" => options.secrets = true,
            "authorize-sensitive-export" => options.authorize_sensitive = true,
            "authorize-secret-export" => options.authorize_secret = true,
            _ => return Err(CliError::usage("unsupported Run export option")),
        }
    }
    options.validate().map_err(CliError::usage)?;
    Ok(Request {
        run,
        options,
        destination: destination.ok_or_else(|| CliError::usage("missing --output"))?,
    })
}

pub(super) fn execute(
    request: Request,
    root: &Path,
    format: reply::OutputContext,
) -> Result<(), CliError> {
    if format.version() == machine::Version::Legacy {
        return Err(CliError::operation(
            "run export requires machine interface 1.0-alpha.4",
        ));
    }
    request.options.validate().map_err(CliError::operation)?;
    let run = request.run.full();
    let application = PactrunApplication::open(root).map_err(app_error)?;
    let trace = crate::trace_journal::read_run(&crate::trace_journal::Location::store(root), run)
        .map_err(|error| {
        CliError::operation(format!(
            "Run phase trace could not be read: {error}; no report was published"
        ))
    })?;
    let mut stage = application.create_run_export_stage().map_err(app_error)?;
    let result = {
        let mut writer = archive::Writer::new(&mut stage, run, request.options, trace);
        application
            .write_run_export(run, request.options, &mut writer)
            .map_err(errors::read)?;
        writer.finish().map_err(errors::encode)?
    };
    stage.finish_operation_file().map_err(|error| {
        errors::before_publication(
            "Run report staging could not be synchronized",
            errors::io_reason(error.kind()),
        )
    })?;
    crate::output_publication::publish_reported(&stage, &request.destination).map_err(|error| {
        let message = if error.destination_published {
            "Run report was published but durability could not be confirmed; the existing report was not removed"
        } else if error.source.kind() == io::ErrorKind::AlreadyExists {
            "Run report destination already exists; it was not overwritten"
        } else { "Run report destination could not be published" };
        let mut result = publication_error(error, &request.destination);
        result.message = message.into();
        result
    })?;
    presentation::emit_result(format, "run export", &result)
}

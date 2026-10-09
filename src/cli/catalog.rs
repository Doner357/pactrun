//! Human catalog commands. Text formatting is not a machine protocol.
use super::*;
use crate::domain::*;

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;

pub(super) struct PageOptions<K> {
    pub(super) limit: usize,
    after: Option<K>,
    pub(super) no_trunc: bool,
}

pub(super) enum CatalogCommand {
    Revisions(PageOptions<RevisionCursor>),
    Revision(RevisionReference, bool),
    History(PageOptions<Selector<InstanceId>>, bool),
    Instance(Selector<InstanceId>),
    Runs(RunSelector, PageOptions<Selector<RunId>>),
    Local(RevisionReference, LocalKind),
    Mutation {
        revision: RevisionReference,
        operation: RevisionMetadataMutation,
        inspect: String,
    },
}

pub(super) enum RunSelector {
    All,
    Name(InstanceName),
    Identity(Selector<InstanceId>),
}
impl CatalogCommand {
    pub(super) fn resolve_ids(&mut self, r: &mut short_ids::Resolver<'_>) -> Result<(), CliError> {
        match self {
            Self::Revisions(_) => {}
            Self::Revision(id, _) | Self::Local(id, _) | Self::Mutation { revision: id, .. } => {
                r.revision(id)?
            }
            Self::History(o, _) => {
                if let Some(id) = &mut o.after {
                    id.resolve(r)?;
                }
            }
            Self::Instance(id) => id.resolve(r)?,
            Self::Runs(s, o) => {
                if let RunSelector::Identity(id) = s {
                    id.resolve(r)?;
                }
                if let Some(id) = &mut o.after {
                    id.resolve(r)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) enum LocalKind {
    Note,
    Trust,
}

fn exact(value: String) -> Result<RevisionReference, CliError> {
    parse_revision_reference(value)
}
fn id<T: FromStr>(value: String) -> Result<T, CliError> {
    value
        .parse()
        .map_err(|_| CliError::usage("invalid full identity"))
}
fn trust(value: String) -> Result<TrustAssessment, CliError> {
    match value.as_str() {
        "trusted" => Ok(TrustAssessment::Trusted),
        "distrusted" => Ok(TrustAssessment::Distrusted),
        _ => Err(CliError::usage("trust must be trusted or distrusted")),
    }
}
fn note(value: String) -> Result<LocalNote, CliError> {
    LocalNote::parse(value).map_err(|e| CliError::usage(e.to_string()))
}
fn paging<K>(
    parser: &mut Parser,
    cursor: impl Fn(String) -> Result<K, CliError>,
) -> Result<PageOptions<K>, CliError> {
    let mut limit = None;
    let mut after = None;
    let mut no_trunc = false;
    while let Some(arg) = parser.next().map_err(lex_error)? {
        match arg {
            Arg::Long("limit") => set_once(
                &mut limit,
                parse_limit(value_string(parser, "limit")?)?,
                "--limit",
            )?,
            Arg::Long("after") => set_once(
                &mut after,
                cursor(value_string(parser, "cursor")?)?,
                "--after",
            )?,
            Arg::Long("no-trunc") if !no_trunc => no_trunc = true,
            _ => return Err(CliError::usage("unsupported or duplicate catalog option")),
        }
    }
    Ok(PageOptions {
        limit: limit.unwrap_or(50),
        after,
        no_trunc,
    })
}
fn parse_limit(value: String) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .ok()
        .filter(|v| (1..=500).contains(v))
        .ok_or_else(|| CliError::usage("--limit must be 1..500"))
}

pub(super) fn parse_revision(parser: &mut Parser) -> Result<Command, CliError> {
    let operation = required_value_string(parser, "Revision command")?;
    if matches!(operation.as_str(), "rename" | "unname") {
        return names::parse_revision(parser, operation == "rename");
    }
    let command = match operation.as_str() {
        "export" => {
            let revision =
                parse_revision_reference(required_value_string(parser, "Revision reference")?)?;
            let mut output = None;
            let mut include_metadata = false;
            while let Some(arg) = parser.next().map_err(lex_error)? {
                match arg {
                    Arg::Long("output") => set_once(
                        &mut output,
                        PathBuf::from(required_value(parser, "output base path")?),
                        "--output",
                    )?,
                    Arg::Long("include-portable-metadata") if !include_metadata => {
                        include_metadata = true
                    }
                    _ => {
                        return Err(CliError::usage(
                            "unsupported or duplicate Revision export option",
                        ));
                    }
                }
            }
            let output = output.ok_or_else(|| CliError::usage("--output is required"))?;
            let output =
                export_paths::destination(output.into_os_string(), export_paths::Format::Pack)?;
            return Ok(Command::ExportRevision {
                revision,
                output,
                include_metadata,
            });
        }
        "list" => CatalogCommand::Revisions(paging(parser, |text| {
            text.parse::<RevisionCursor>().map_err(|_| {
                CliError::usage("--after requires the complete cursor from revision list")
            })
        })?),
        "show" => {
            let reference =
                parse_revision_reference(required_value_string(parser, "Revision reference")?)?;
            require_end(parser)?;
            CatalogCommand::Revision(reference, false)
        }
        "metadata" => {
            if required_value_string(parser, "metadata command")? != "show" {
                return Err(CliError::usage("expected metadata show"));
            }
            let reference =
                parse_revision_reference(required_value_string(parser, "Revision reference")?)?;
            require_end(parser)?;
            CatalogCommand::Revision(reference, true)
        }
        "delete" => {
            let reference =
                parse_revision_reference(required_value_string(parser, "Revision reference")?)?;
            require_end(parser)?;
            return Ok(Command::Lifecycle(lifecycle::LifecycleCommand::Revision(
                reference,
            )));
        }
        "alias" => {
            return Err(CliError::usage(
                "Use package/revision rename or unname to manage local names",
            ));
        }
        "note" => parse_local(parser, LocalKind::Note)?,
        "trust" => parse_local(parser, LocalKind::Trust)?,
        _ => return Err(CliError::usage("unknown Revision command")),
    };
    Ok(Command::Catalog(command))
}
pub(super) fn parse_history(parser: &mut Parser) -> Result<Command, CliError> {
    let command = match required_value_string(parser, "history command")?.as_str() {
        "list" => CatalogCommand::History(paging(parser, id)?, false),
        "show" => {
            let value = id(required_value_string(parser, "InstanceId")?)?;
            require_end(parser)?;
            CatalogCommand::Instance(value)
        }
        _ => return Err(CliError::usage("expected history list or show")),
    };
    Ok(Command::Catalog(command))
}
pub(super) fn parse_deletion(parser: &mut Parser) -> Result<Command, CliError> {
    let operation = required_value_string(parser, "deletion command")?;
    if operation == "list" {
        Ok(Command::Catalog(CatalogCommand::History(
            paging(parser, id)?,
            true,
        )))
    } else {
        retirements::parse_deletion_operation(parser, &operation).map(Command::Retirement)
    }
}
pub(super) fn parse_runs(parser: &mut Parser) -> Result<Command, CliError> {
    let mut selector = None;
    let mut limit = None;
    let mut after = None;
    let mut no_trunc = false;
    while let Some(arg) = parser.next().map_err(lex_error)? {
        match arg {
            Arg::Value(value) => set_once(
                &mut selector,
                RunSelector::Name(parse_instance_name(os_string(value, "Instance name")?)?),
                "Run selector",
            )?,
            Arg::Long("instance-id") => set_once(
                &mut selector,
                RunSelector::Identity(id(value_string(parser, "InstanceId")?)?),
                "Run selector",
            )?,
            Arg::Long("limit") => set_once(
                &mut limit,
                parse_limit(value_string(parser, "limit")?)?,
                "--limit",
            )?,
            Arg::Long("after") => {
                set_once(&mut after, id(value_string(parser, "RunId")?)?, "--after")?
            }
            Arg::Long("no-trunc") if !no_trunc => no_trunc = true,
            _ => return Err(CliError::usage("unsupported or duplicate Run list option")),
        }
    }
    Ok(Command::Catalog(CatalogCommand::Runs(
        selector.unwrap_or(RunSelector::All),
        PageOptions {
            limit: limit.unwrap_or(50),
            after,
            no_trunc,
        },
    )))
}

fn expectation<T>(
    parser: &mut Parser,
    parse: impl Fn(String) -> Result<T, CliError>,
) -> Result<CurrentState<T>, CliError> {
    let mut expected = None;
    while let Some(arg) = parser.next().map_err(lex_error)? {
        let value = match arg {
            Arg::Long("expect-absent") => CurrentState::Absent,
            Arg::Long("expect") => {
                CurrentState::Present(parse(value_string(parser, "expected value")?)?)
            }
            _ => return Err(CliError::usage("requires only --expect or --expect-absent")),
        };
        set_once(&mut expected, value, "expected state")?;
    }
    expected.ok_or_else(|| CliError::usage("missing --expect or --expect-absent"))
}
fn parse_local(parser: &mut Parser, kind: LocalKind) -> Result<CatalogCommand, CliError> {
    let command = required_value_string(parser, "local metadata command")?;
    let reference = required_value_string(parser, "Revision reference")?;
    if command == "show" {
        let reference = parse_revision_reference(reference)?;
        require_end(parser)?;
        return Ok(CatalogCommand::Local(reference, kind));
    }
    let revision = exact(reference)?;
    if command != "set" && command != "clear" {
        return Err(CliError::usage("expected show, set or clear"));
    }
    let operation = match kind {
        LocalKind::Note => {
            let mut desired = None;
            let mut expected = None;
            while let Some(arg) = parser.next().map_err(lex_error)? {
                match arg {
                    Arg::Long("value") if command == "set" => set_once(
                        &mut desired,
                        note(value_string(parser, "note")?)?,
                        "--value",
                    )?,
                    Arg::Long("expect") => set_once(
                        &mut expected,
                        CurrentState::Present(note(value_string(parser, "expected note")?)?),
                        "expected state",
                    )?,
                    Arg::Long("expect-absent") => {
                        set_once(&mut expected, CurrentState::Absent, "expected state")?
                    }
                    _ => return Err(CliError::usage("unsupported or duplicate note option")),
                }
            }
            let desired = if command == "set" {
                CurrentState::Present(desired.ok_or_else(|| CliError::usage("missing --value"))?)
            } else {
                CurrentState::Absent
            };
            RevisionMetadataMutation::CompareAndSetLocalNote {
                expected: expected.ok_or_else(|| CliError::usage("missing expected state"))?,
                desired,
            }
        }
        LocalKind::Trust => {
            let desired = if command == "set" {
                CurrentState::Present(trust(required_value_string(parser, "trust")?)?)
            } else {
                CurrentState::Absent
            };
            RevisionMetadataMutation::CompareAndSetLocalTrust {
                expected: expectation(parser, trust)?,
                desired,
            }
        }
    };
    let inspect = "pactrun revision metadata show <revision-reference>".into();
    Ok(CatalogCommand::Mutation {
        revision,
        operation,
        inspect,
    })
}

fn selector(reference: RevisionReference) -> CatalogRevisionSelector {
    match reference {
        RevisionReference::Prefix { .. } => unreachable!("resolved CLI selector"),
        RevisionReference::Named(_) => unreachable!("resolved local reference"),
        RevisionReference::Exact(id) => CatalogRevisionSelector::Exact(id),
    }
}
pub(super) fn safe(value: &str) -> String {
    value.chars().flat_map(|c| {
        if c == '\\' || c == '"' || c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{2028}' | '\u{2029}') {
            c.escape_default().collect::<Vec<_>>()
        } else { vec![c] }
    }).collect()
}

pub(super) fn execute(
    command: CatalogCommand,
    root: &Path,
    format: reply::OutputContext,
) -> Result<(), CliError> {
    let app = if matches!(command, CatalogCommand::Mutation { .. }) {
        // Validate an existing store before opening a writer; metadata is never an installer.
        drop(PactrunApplication::open_read_only(root).map_err(app_error)?);
        PactrunApplication::open(root)
    } else {
        PactrunApplication::open_read_only(root)
    }
    .map_err(app_error)?;
    match command {
        CatalogCommand::Revisions(options) => {
            let page = app
                .catalog_revisions(options.limit, options.after.as_ref())
                .map_err(app_error)?;
            let result = catalog_presentation::Page {
                items: page
                    .items
                    .iter()
                    .map(|row| catalog_presentation::RevisionEntry::new(row, true))
                    .collect(),
                next: page.next.as_ref().map(ToString::to_string),
            };
            presentation::emit_result(format, "revision list", &result)
        }
        CatalogCommand::Revision(reference, metadata_only) => {
            let row = app
                .catalog_resolve_revision(&selector(reference))
                .map_err(app_error)?;
            let author =
                capability_presentation::project(&row, &[&capability_presentation::Selection::All]);
            if !metadata_only {
                return presentation::emit_result(
                    format,
                    "revision show",
                    &capability_presentation::Presented {
                        value: catalog_presentation::RevisionEntry::new(&row, true),
                        presentation: author,
                    },
                );
            }
            {
                presentation::emit_result(
                    format,
                    if metadata_only {
                        "revision metadata show"
                    } else {
                        "revision show"
                    },
                    &catalog_presentation::RevisionEntry::new(&row, !metadata_only),
                )
            }
        }
        CatalogCommand::History(options, deletions) => {
            let after = options.after.clone().map(Selector::full);
            if deletions {
                let page = app
                    .inspect_retirements(options.limit, after)
                    .map_err(app_error)?;
                let result = catalog_presentation::Page {
                    items: page
                        .items
                        .iter()
                        .map(|entry| catalog_presentation::Retirement {
                            history: (&entry.history).into(),
                            inspection: entry.into(),
                        })
                        .collect(),
                    next: page.next.map(|id| id.to_string()),
                };
                return presentation::emit_result(format, "instance deletion list", &result);
            }
            let page = app
                .catalog_history(options.limit, after, deletions)
                .map_err(app_error)?;
            {
                let result = catalog_presentation::Page {
                    items: page
                        .items
                        .iter()
                        .map(catalog_presentation::History::from)
                        .collect(),
                    next: page.next.map(|id| id.to_string()),
                };
                presentation::emit_result(
                    format,
                    if deletions {
                        "instance deletion list"
                    } else {
                        "instance history list"
                    },
                    &result,
                )
            }
        }
        CatalogCommand::Instance(id) => {
            let id = id.full();
            let row = app.catalog_instance(id).map_err(app_error)?;
            {
                presentation::emit_result(
                    format,
                    "instance history show",
                    &catalog_presentation::History::from(&row),
                )
            }
        }
        CatalogCommand::Runs(selector, options) => {
            let selector = match selector {
                RunSelector::All => CatalogRunSelector::All,
                RunSelector::Name(n) => CatalogRunSelector::Name(n),
                RunSelector::Identity(id) => CatalogRunSelector::Identity(id.full()),
            };
            let after = options.after.clone().map(Selector::full);
            {
                let full = app
                    .inspect_runs_complete(&selector, options.limit, after)
                    .map_err(app_error)?;
                let page = catalog_presentation::Page {
                    items: full
                        .inspections
                        .iter()
                        .map(|i| {
                            let mut value = execution_presentation::Run::from(&i.run);
                            if let Some((run, instance)) = full.selectors.get(&i.run.id) {
                                value.run_prefix_length = *run;
                                value.instance_prefix_length = *instance;
                            }
                            definitions::InspectedRun {
                                value,
                                inspection: i.into(),
                            }
                        })
                        .collect(),
                    next: full.page.next.map(|id| id.to_string()),
                };
                presentation::emit_result(
                    format,
                    "run list",
                    &definitions::Related::new(page, &full.revisions, &full.unavailable_revisions),
                )
            }
        }
        CatalogCommand::Local(reference, kind) => {
            let row = app
                .catalog_resolve_revision(&selector(reference))
                .map_err(app_error)?;
            {
                presentation::emit_result(
                    format,
                    match kind {
                        LocalKind::Note => "revision note show",
                        LocalKind::Trust => "revision trust show",
                    },
                    &catalog_presentation::Local::new(&row),
                )
            }
        }
        CatalogCommand::Mutation {
            revision,
            operation,
            inspect,
        } => {
            let revision = short_ids::exact(revision);
            app.mutate_local_metadata(&revision, operation.clone()).map_err(|e| {
                if matches!(e, ApplicationError::Persistence(crate::persistence::PersistenceError::MetadataConflict(_))) {
                    CliError::operation(format!("Metadata conflict: current state differs from expected. No changes applied.\nInspect: {inspect}"))
                } else { app_error(e) }
            })?;
            {
                let command =
                    presentation::command_name(&Command::Catalog(CatalogCommand::Mutation {
                        revision: RevisionReference::Exact(revision.clone()),
                        operation: operation.clone(),
                        inspect: inspect.clone(),
                    }));
                presentation::emit_result(
                    format,
                    command,
                    &catalog_presentation::Mutation::new(&revision, &operation),
                )
            }
        }
    }
}

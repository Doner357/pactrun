//! Human catalog commands. Text formatting is not a machine protocol.
use super::*;
use crate::domain::*;

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;

pub(super) struct PageOptions<K> {
    limit: usize,
    after: Option<K>,
    no_trunc: bool,
}

pub(super) enum CatalogCommand {
    Revisions(PageOptions<RevisionReference>),
    Revision(RevisionReference, bool),
    History(PageOptions<Selector<InstanceId>>, bool),
    Instance(Selector<InstanceId>),
    Runs(RunSelector, PageOptions<Selector<RunId>>),
    Alias(LocalAlias),
    Local(RevisionReference, LocalKind),
    Mutation {
        revision: RevisionReference,
        operation: RevisionMetadataMutation,
        inspect: String,
    },
    AliasMutation {
        target: RevisionReference,
        alias: LocalAlias,
        expected: CurrentState<RevisionReference>,
        desired: bool,
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
            Self::Revisions(o) => {
                if let Some(id) = &mut o.after {
                    r.revision(id)?;
                }
            }
            Self::Revision(id, _) | Self::Local(id, _) | Self::Mutation { revision: id, .. } => {
                r.revision(id)?
            }
            Self::AliasMutation {
                target, expected, ..
            } => {
                r.revision(target)?;
                if let CurrentState::Present(id) = expected {
                    r.revision(id)?;
                }
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
            _ => {}
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
    match parse_revision_reference(value)? {
        id @ (RevisionReference::Exact(_) | RevisionReference::Prefix { .. }) => Ok(id),
        _ => Err(CliError::usage(
            "writes and Revision cursors require exact:<PackageId>/sha256:<digest>",
        )),
    }
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
        "list" => CatalogCommand::Revisions(paging(parser, exact)?),
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
        "alias" => parse_alias(parser)?,
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
fn parse_alias(parser: &mut Parser) -> Result<CatalogCommand, CliError> {
    let command = required_value_string(parser, "alias command")?;
    let alias = LocalAlias::parse(value_string(parser, "alias")?)
        .map_err(|e| CliError::usage(e.to_string()))?;
    if command == "show" {
        require_end(parser)?;
        return Ok(CatalogCommand::Alias(alias));
    }
    let (revision, desired, expected) = match command.as_str() {
        "set" => {
            let revision = exact(required_value_string(parser, "exact Revision")?)?;
            let expected = expectation(parser, exact)?;
            (revision.clone(), CurrentState::Present(revision), expected)
        }
        "clear" => {
            let expected = expectation(parser, exact)?;
            let CurrentState::Present(revision) = &expected else {
                return Err(CliError::usage(
                    "alias clear requires --expect <exact-revision>",
                ));
            };
            (revision.clone(), CurrentState::Absent, expected)
        }
        _ => return Err(CliError::usage("expected alias show, set or clear")),
    };
    let inspect = format!(
        "pactrun revision alias show <alias> (alias value: {})",
        quoted(alias.as_str())
    );
    Ok(CatalogCommand::AliasMutation {
        target: revision,
        alias,
        expected,
        desired: matches!(desired, CurrentState::Present(_)),
        inspect,
    })
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

fn exact_text(id: &RevisionIdentity) -> String {
    format!("exact:{}/{}", id.package_id, id.content_digest)
}
fn selector(reference: RevisionReference) -> CatalogRevisionSelector {
    match reference {
        RevisionReference::Prefix { .. } => unreachable!("resolved CLI selector"),
        RevisionReference::Exact(id) => CatalogRevisionSelector::Exact(id),
        RevisionReference::Alias(alias) => CatalogRevisionSelector::Alias(alias),
        RevisionReference::Label(label) => CatalogRevisionSelector::Label(label),
    }
}
pub(super) fn safe(value: &str) -> String {
    value.chars().flat_map(|c| {
        if c == '\\' || c == '"' || c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{2028}' | '\u{2029}') {
            c.escape_default().collect::<Vec<_>>()
        } else { vec![c] }
    }).collect()
}
fn quoted(value: &str) -> String {
    format!("\"{}\"", safe(value))
}
fn cell(value: &str, full: bool) -> String {
    let value = safe(value);
    if !full && value.chars().count() > 24 {
        format!("{}...", value.chars().take(21).collect::<String>())
    } else {
        value
    }
}
fn summary(values: Vec<&str>, full: bool) -> String {
    if values.is_empty() {
        return "-".into();
    }
    if full {
        return values.into_iter().map(safe).collect::<Vec<_>>().join(", ");
    }
    let first = cell(values[0], false);
    if values.len() > 1 {
        format!("{first} (+{} more)", values.len() - 1)
    } else {
        first
    }
}
fn continuation<K>(
    out: &mut dyn Write,
    count: usize,
    command: &str,
    next: Option<String>,
    options: &PageOptions<K>,
) -> Result<(), CliError> {
    writeln!(out, "\n{count} records shown.").map_err(io_operation)?;
    if let Some(next) = next {
        writeln!(
            out,
            "More results available. Continue:\n  pactrun {command} --limit {} --after {next}{}",
            options.limit,
            if options.no_trunc { " --no-trunc" } else { "" }
        )
        .map_err(io_operation)?;
    }
    Ok(())
}

pub(super) fn execute(
    command: CatalogCommand,
    root: &Path,
    out: &mut dyn Write,
    format: presentation::Format,
) -> Result<(), CliError> {
    let command = match command {
        CatalogCommand::AliasMutation {
            target,
            alias,
            expected,
            desired,
            inspect,
        } => {
            let target = short_ids::exact(target);
            let expected = match expected {
                CurrentState::Absent => CurrentState::Absent,
                CurrentState::Present(id) => CurrentState::Present(short_ids::exact(id)),
            };
            CatalogCommand::Mutation {
                revision: RevisionReference::Exact(target.clone()),
                operation: RevisionMetadataMutation::CompareAndSetLocalAlias {
                    alias,
                    expected,
                    desired: if desired {
                        CurrentState::Present(target)
                    } else {
                        CurrentState::Absent
                    },
                },
                inspect,
            }
        }
        other => other,
    };
    let app = if matches!(command, CatalogCommand::Mutation { .. }) {
        // Validate an existing store before opening a writer; metadata is never an installer.
        drop(PactrunApplication::open_read_only(root).map_err(app_error)?);
        PactrunApplication::open(root)
    } else {
        PactrunApplication::open_read_only(root)
    }
    .map_err(app_error)?;
    match command {
        CatalogCommand::AliasMutation { .. } => unreachable!("normalized alias mutation"),
        CatalogCommand::Revisions(options) => {
            let after = options.after.clone().map(short_ids::exact);
            let page = app
                .catalog_revisions(options.limit, after.as_ref())
                .map_err(app_error)?;
            if format == presentation::Format::Json {
                let result = catalog_presentation::Page {
                    items: page
                        .items
                        .iter()
                        .map(|r| catalog_presentation::RevisionEntry::new(r, true))
                        .collect(),
                    next: page.next.as_ref().map(exact_text),
                };
                return presentation::render(
                    format,
                    "revision list",
                    &result,
                    out,
                    |_, _| unreachable!(),
                );
            }
            writeln!(out, "PACKAGE ID  REVISION DIGEST  ALIASES  LABELS").map_err(io_operation)?;
            let labels = if options.no_trunc {
                page.items
                    .iter()
                    .map(|r| {
                        (
                            r.identity.package_id.to_string(),
                            r.identity.content_digest.to_string(),
                        )
                    })
                    .collect()
            } else {
                app.revision_abbreviations(
                    &page
                        .items
                        .iter()
                        .map(|r| r.identity.clone())
                        .collect::<Vec<_>>(),
                )
                .map_err(app_error)?
            };
            for (row, (package, digest)) in page.items.iter().zip(labels) {
                let aliases = row
                    .metadata
                    .items
                    .iter()
                    .filter_map(|m| {
                        if let RevisionMetadataItem::LocalAlias { alias, .. } = m {
                            Some(alias.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                let labels = row
                    .metadata
                    .items
                    .iter()
                    .filter_map(|m| {
                        if let RevisionMetadataItem::ReferenceLabel(b) = m {
                            Some(b.label.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                writeln!(
                    out,
                    "{}  {}  {}  {}",
                    package,
                    digest,
                    summary(aliases, options.no_trunc),
                    summary(labels, options.no_trunc)
                )
                .map_err(io_operation)?;
            }
            continuation(
                out,
                page.items.len(),
                "revision list",
                page.next.as_ref().map(exact_text),
                &options,
            )
        }
        CatalogCommand::Revision(reference, metadata_only) => {
            let row = app
                .catalog_resolve_revision(&selector(reference))
                .map_err(app_error)?;
            let author =
                capability_presentation::project(&row, &[&capability_presentation::Selection::All]);
            if format == presentation::Format::Json && !metadata_only {
                return presentation::render(
                    format,
                    "revision show",
                    &capability_presentation::Presented {
                        value: catalog_presentation::RevisionEntry::new(&row, true),
                        presentation: author,
                    },
                    out,
                    |_, _| unreachable!(),
                );
            }
            if format == presentation::Format::Json {
                return presentation::render(
                    format,
                    if metadata_only {
                        "revision metadata show"
                    } else {
                        "revision show"
                    },
                    &catalog_presentation::RevisionEntry::new(&row, !metadata_only),
                    out,
                    |_, _| unreachable!(),
                );
            }
            writeln!(
                out,
                "Revision: {}\nCore format: {}",
                exact_text(&row.identity),
                row.core.version()
            )
            .map_err(io_operation)?;
            if !metadata_only {
                declarations(out, &row.core)?;
                capability_presentation::write(out, &author, false)?;
                let mut remaining = row.metadata.clone();
                remaining
                    .items
                    .retain(|item| !matches!(item, RevisionMetadataItem::Presentation(_)));
                return metadata(out, &remaining);
            }
            metadata(out, &row.metadata)
        }
        CatalogCommand::History(options, deletions) => {
            let after = options.after.clone().map(Selector::full);
            if deletions && format == presentation::Format::Json {
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
                return presentation::render(
                    format,
                    "instance deletion list",
                    &result,
                    out,
                    |_, _| unreachable!(),
                );
            }
            let page = app
                .catalog_history(options.limit, after, deletions)
                .map_err(app_error)?;
            if format == presentation::Format::Json {
                let result = catalog_presentation::Page {
                    items: page
                        .items
                        .iter()
                        .map(catalog_presentation::History::from)
                        .collect(),
                    next: page.next.map(|id| id.to_string()),
                };
                return presentation::render(
                    format,
                    if deletions {
                        "instance deletion list"
                    } else {
                        "instance history list"
                    },
                    &result,
                    out,
                    |_, _| unreachable!(),
                );
            }
            writeln!(
                out,
                "INSTANCE ID  CURRENT NAME  RECORDED NAME  MANAGEMENT  DELETION"
            )
            .map_err(io_operation)?;
            let labels = short_ids::labels(
                &app,
                CatalogIdentityKind::Instance,
                page.items.iter().map(|r| r.id.to_string()).collect(),
                options.no_trunc,
            )?;
            for (row, label) in page.items.iter().zip(labels) {
                history_row(out, row, options.no_trunc, &label)?;
            }
            continuation(
                out,
                page.items.len(),
                if deletions {
                    "instance deletion list"
                } else {
                    "instance history list"
                },
                page.next.map(|id| id.to_string()),
                &options,
            )
        }
        CatalogCommand::Instance(id) => {
            let id = id.full();
            let row = app.catalog_instance(id).map_err(app_error)?;
            if format == presentation::Format::Json {
                return presentation::render(
                    format,
                    "instance history show",
                    &catalog_presentation::History::from(&row),
                    out,
                    |_, _| unreachable!(),
                );
            }
            writeln!(
                out,
                "INSTANCE ID  CURRENT NAME  RECORDED NAME  MANAGEMENT  DELETION"
            )
            .map_err(io_operation)?;
            history_row(out, &row, true, &id.to_string())?;
            writeln!(out, "\nRuns: pactrun run list --instance-id {id}").map_err(io_operation)?;
            if row.retired || row.deletion_phase.is_some() {
                writeln!(out, "Deletion: pactrun instance deletion show {id}")
                    .map_err(io_operation)?;
            }
            Ok(())
        }
        CatalogCommand::Runs(selector, options) => {
            let selector = match selector {
                RunSelector::All => CatalogRunSelector::All,
                RunSelector::Name(n) => CatalogRunSelector::Name(n),
                RunSelector::Identity(id) => CatalogRunSelector::Identity(id.full()),
            };
            let after = options.after.clone().map(Selector::full);
            if format == presentation::Format::Json {
                let full = app
                    .inspect_runs_complete(&selector, options.limit, after)
                    .map_err(app_error)?;
                let page = catalog_presentation::Page {
                    items: full
                        .inspections
                        .iter()
                        .map(|i| definitions::InspectedRun {
                            value: execution_presentation::Run::from(&i.run),
                            inspection: i.into(),
                        })
                        .collect(),
                    next: full.page.next.map(|id| id.to_string()),
                };
                return presentation::render(
                    format,
                    "run list",
                    &definitions::Related::new(page, &full.revisions, &full.unavailable_revisions),
                    out,
                    |_, _| unreachable!(),
                );
            }
            let page = app
                .catalog_runs(&selector, options.limit, after)
                .map_err(app_error)?;
            writeln!(out, "RUN ID  INSTANCE ID  OPERATION  PHASE  OUTCOME")
                .map_err(io_operation)?;
            let runs = short_ids::labels(
                &app,
                CatalogIdentityKind::Run,
                page.items.iter().map(|r| r.id.to_string()).collect(),
                options.no_trunc,
            )?;
            let instances = short_ids::labels(
                &app,
                CatalogIdentityKind::Instance,
                page.items.iter().map(|r| r.instance.to_string()).collect(),
                options.no_trunc,
            )?;
            for ((row, run), instance) in page.items.iter().zip(runs).zip(instances) {
                let operation = match &row.operation {
                    ManagedRunIdentity::Action(a) => safe(a.action.as_str()),
                    other => format!("{:?}", other.kind()),
                };
                let outcome = match &row.state {
                    RunState::Running(_) => "-".into(),
                    RunState::Finished(f) => format_outcome(f.outcome).to_owned(),
                };
                writeln!(
                    out,
                    "{}  {}  {}  {}  {}",
                    run,
                    instance,
                    cell(&operation, options.no_trunc),
                    format_phase(row.state.phase()),
                    outcome
                )
                .map_err(io_operation)?;
            }
            let command = match &selector {
                CatalogRunSelector::All => "run list".into(),
                CatalogRunSelector::Identity(id) => format!("run list --instance-id {id}"),
                // A full page proves the resolved identity. Preserve selection by ID,
                // even if this name is retired/reused before the next command.
                CatalogRunSelector::Name(_) => page
                    .items
                    .first()
                    .map(|r| format!("run list --instance-id {}", r.instance))
                    .unwrap_or_else(|| "run list".into()),
            };
            continuation(
                out,
                page.items.len(),
                &command,
                page.next.map(|id| id.to_string()),
                &options,
            )
        }
        CatalogCommand::Alias(alias) => {
            let target = app.catalog_alias(&alias).map_err(app_error)?;
            if format == presentation::Format::Json {
                let result = catalog_presentation::Alias {
                    alias: alias.as_str().into(),
                    target: target.as_ref().map(Into::into),
                };
                return presentation::render(
                    format,
                    "revision alias show",
                    &result,
                    out,
                    |_, _| unreachable!(),
                );
            }
            writeln!(
                out,
                "Alias: {}\nTarget: {}",
                safe(alias.as_str()),
                target
                    .as_ref()
                    .map(exact_text)
                    .unwrap_or_else(|| "Absent".into())
            )
            .map_err(io_operation)
        }
        CatalogCommand::Local(reference, kind) => {
            let row = app
                .catalog_resolve_revision(&selector(reference))
                .map_err(app_error)?;
            if format == presentation::Format::Json {
                return presentation::render(
                    format,
                    match kind {
                        LocalKind::Note => "revision note show",
                        LocalKind::Trust => "revision trust show",
                    },
                    &catalog_presentation::Local::new(&row),
                    out,
                    |_, _| unreachable!(),
                );
            }
            writeln!(out, "Revision: {}", exact_text(&row.identity)).map_err(io_operation)?;
            local_metadata(out, &row.metadata, kind)
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
            if format == presentation::Format::Json {
                let command =
                    presentation::command_name(&Command::Catalog(CatalogCommand::Mutation {
                        revision: RevisionReference::Exact(revision.clone()),
                        operation: operation.clone(),
                        inspect: inspect.clone(),
                    }));
                return presentation::render(
                    format,
                    command,
                    &catalog_presentation::Mutation::new(&revision, &operation),
                    out,
                    |_, _| unreachable!(),
                );
            }
            writeln!(
                out,
                "Applied (including idempotent retries).\nRevision: {}",
                exact_text(&revision)
            )
            .map_err(io_operation)?;
            match operation {
                RevisionMetadataMutation::CompareAndSetLocalAlias { alias, desired, .. } => {
                    writeln!(
                        out,
                        "Alias: {}\nTarget: {}",
                        safe(alias.as_str()),
                        match desired {
                            CurrentState::Absent => "Absent".into(),
                            CurrentState::Present(id) => exact_text(&id),
                        }
                    )
                    .map_err(io_operation)
                }
                RevisionMetadataMutation::CompareAndSetLocalNote { desired, .. } => writeln!(
                    out,
                    "Local note: {}",
                    match desired {
                        CurrentState::Absent => "Absent".into(),
                        CurrentState::Present(v) => quoted(v.as_str()),
                    }
                )
                .map_err(io_operation),
                RevisionMetadataMutation::CompareAndSetLocalTrust { desired, .. } => writeln!(
                    out,
                    "Trust label: {}",
                    match desired {
                        CurrentState::Absent => "NoDecision".into(),
                        CurrentState::Present(v) => format!("{v:?}"),
                    }
                )
                .map_err(io_operation),
                _ => unreachable!("parser only admits local metadata mutations"),
            }
        }
    }
}

fn history_row(
    out: &mut dyn Write,
    row: &InstanceHistoryEntry,
    full: bool,
    id: &str,
) -> Result<(), CliError> {
    writeln!(
        out,
        "{}  {}  {}  {}  {}",
        id,
        row.current_name
            .as_ref()
            .map(|n| cell(n.as_str(), full))
            .unwrap_or_else(|| "-".into()),
        cell(row.recorded_name.as_str(), full),
        if row.current_name.is_some() {
            "Managed"
        } else if row.retired {
            "Retired"
        } else {
            "Historical"
        },
        row.deletion_phase
            .map(|p| format!("{p:?}"))
            .unwrap_or_else(|| if row.retired {
                "Retired".into()
            } else {
                "-".into()
            })
    )
    .map_err(io_operation)
}
fn declarations(out: &mut dyn Write, core: &RevisionCore) -> Result<(), CliError> {
    writeln!(out, "\nDeclarations").map_err(io_operation)?;
    for action in core.actions() {
        writeln!(
            out,
            "  Action: {} (access={}, Hook protocol={}, parameters={}, outputs={})",
            safe(action.id.as_str()),
            access_name(action.access),
            action.hook.protocol_version,
            action.parameters.len(),
            action.outputs.len()
        )
        .map_err(io_operation)?;
    }
    for input in core.inputs() {
        writeln!(
            out,
            "  Input: {} (required={}, protection={:?})",
            safe(input.id.as_str()),
            input.required,
            input.protection
        )
        .map_err(io_operation)?;
    }
    if let Some(service) = core.service_core() {
        writeln!(
            out,
            "  Service storages: {}\n  Service resources: {}",
            service.storages().len(),
            service.resources().len()
        )
        .map_err(io_operation)?;
    }
    writeln!(
        out,
        "  Snapshot capability: {}\n  Migration edges: {}\n  Cleanup: {}",
        core.snapshot().is_some(),
        core.migrations().len(),
        core.cleanup().is_some()
    )
    .map_err(io_operation)
}
fn local_metadata(
    out: &mut dyn Write,
    view: &RevisionMetadataView,
    kind: LocalKind,
) -> Result<(), CliError> {
    let value = view.items.iter().find_map(|item| match (kind, item) {
        (LocalKind::Note, RevisionMetadataItem::LocalNote { note, .. }) => {
            Some(quoted(note.as_str()))
        }
        (LocalKind::Trust, RevisionMetadataItem::LocalTrust { trust, .. }) => {
            Some(format!("{trust:?}"))
        }
        _ => None,
    });
    match kind {
        LocalKind::Note => writeln!(
            out,
            "Local note: {}",
            value.unwrap_or_else(|| "Absent".into())
        ),
        LocalKind::Trust => writeln!(
            out,
            "Trust label: {}",
            value.unwrap_or_else(|| "NoDecision".into())
        ),
    }
    .map_err(io_operation)
}
fn metadata(out: &mut dyn Write, view: &RevisionMetadataView) -> Result<(), CliError> {
    writeln!(out, "\nMetadata").map_err(io_operation)?;
    for item in &view.items {
        match item {
            RevisionMetadataItem::ReferenceLabel(b) => {
                let source = match &b.source {
                    ReferenceLabelSource::Unattributed => "Unattributed",
                    ReferenceLabelSource::SourceUri(_) => "SourceUri",
                    ReferenceLabelSource::Publisher { .. } => "Publisher",
                    ReferenceLabelSource::PublisherSourceUri { .. } => "PublisherSourceUri",
                };
                writeln!(out, "  Label: {}\n    Source: {source}\n    Publisher: {}\n    Namespace: {}\n    Source URI: {}", safe(b.label.as_str()), b.source.publisher_name().map(|v| quoted(v.as_str())).unwrap_or_else(|| "Absent".into()), b.source.publisher_namespace().map(|v| quoted(v.as_str())).unwrap_or_else(|| "Absent".into()), b.source.source_uri().map(|v| quoted(v.as_str())).unwrap_or_else(|| "Absent".into())).map_err(io_operation)?;
            }
            RevisionMetadataItem::Presentation(p) => writeln!(
                out,
                "  Presentation target: {}\n    Field: {:?}\n    Value: {}",
                presentation_target(&p.target),
                p.field,
                safe(p.value.as_str())
            )
            .map_err(io_operation)?,
            RevisionMetadataItem::Provenance { claim, .. } => match claim {
                ProvenanceClaim::SourceUri(uri) => {
                    writeln!(out, "  Source URI claim: {}", safe(uri.as_str()))
                        .map_err(io_operation)?
                }
                ProvenanceClaim::PublisherAttribution {
                    publisher,
                    namespace,
                    source_uri,
                } => writeln!(
                    out,
                    "  Publisher attribution: {}\n    Namespace: {}\n    Source URI: {}",
                    safe(publisher.as_str()),
                    namespace
                        .as_ref()
                        .map(|v| quoted(v.as_str()))
                        .unwrap_or_else(|| "Absent".into()),
                    source_uri
                        .as_ref()
                        .map(|v| quoted(v.as_str()))
                        .unwrap_or_else(|| "Absent".into())
                )
                .map_err(io_operation)?,
                ProvenanceClaim::Attribution { text, source_uri } => writeln!(
                    out,
                    "  Attribution: {}\n    Source URI: {}",
                    safe(text.as_str()),
                    source_uri
                        .as_ref()
                        .map(|v| quoted(v.as_str()))
                        .unwrap_or_else(|| "Absent".into())
                )
                .map_err(io_operation)?,
            },
            RevisionMetadataItem::LocalAlias { alias, .. } => {
                writeln!(out, "  Local alias: {}", safe(alias.as_str())).map_err(io_operation)?
            }
            RevisionMetadataItem::LocalNote { .. } | RevisionMetadataItem::LocalTrust { .. } => {}
        }
    }
    local_metadata(out, view, LocalKind::Note)?;
    local_metadata(out, view, LocalKind::Trust)
}

pub(super) fn presentation_target(target: &PresentationTargetV1) -> String {
    match target {
        PresentationTargetV1::Revision => "Revision".into(),
        PresentationTargetV1::Input(id) => format!("Input {}", safe(id.as_str())),
        PresentationTargetV1::Action(id) => format!("Action {}", safe(id.as_str())),
        PresentationTargetV1::ActionParameter { action, parameter } => format!(
            "Action {} / Parameter {}",
            safe(action.as_str()),
            safe(parameter.as_str())
        ),
        PresentationTargetV1::ManagedOutput { action, output } => format!(
            "Action {} / Managed output {}",
            safe(action.as_str()),
            safe(output.as_str())
        ),
        PresentationTargetV1::SnapshotCapture => "Snapshot capture".into(),
        PresentationTargetV1::SnapshotRestore => "Snapshot restore".into(),
        PresentationTargetV1::SnapshotCaptureParameter(id) => {
            format!("Snapshot capture / Parameter {}", safe(id.as_str()))
        }
        PresentationTargetV1::SnapshotRestoreParameter(id) => {
            format!("Snapshot restore / Parameter {}", safe(id.as_str()))
        }
        PresentationTargetV1::MigrationEdge(id) => format!("Migration edge {id}"),
        PresentationTargetV1::Cleanup => "Cleanup".into(),
    }
}

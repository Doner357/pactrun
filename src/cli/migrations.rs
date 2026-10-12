//! Exact path discovery, compilation, and managed Migration execution.
use super::*;
use crate::domain::*;

fn path_presentation(
    app: &PactrunApplication,
    page: &MigrationPathPage,
) -> Result<Vec<capability_presentation::Group>, CliError> {
    let requested: Vec<_> = page
        .candidates
        .iter()
        .flat_map(|candidate| {
            candidate.revisions.windows(2).map(|pair| {
                (
                    pair[1].clone(),
                    capability_presentation::Selection::Edge(pair[0].content_digest),
                )
            })
        })
        .collect();
    capability_presentation::load(app, &requested)
}

pub(super) enum MigrationCommand {
    List {
        name: InstanceName,
        target: RevisionReference,
        after: Option<Selector<MigrationPathId>>,
        limit: usize,
        no_trunc: bool,
        resolved_instance: Option<InstanceId>,
    },
    Plan {
        no_retain_hook_text: bool,
        cancel_on_output_close: bool,
        name: InstanceName,
        target: RevisionReference,
        path: Option<Selector<MigrationPathId>>,
        plan: bool,
        authorize: bool,
        override_guard: bool,
        inputs: Vec<(InputTarget, PathBuf)>,
        parameters: Vec<ParameterTarget>,
        policy: HookRuntimePolicy,
        resolved_instance: Option<InstanceId>,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct InputTarget {
    revision: Selector<RevisionContentDigest>,
    input: InputIdentity,
}
pub(super) struct ParameterTarget {
    revision: Selector<RevisionContentDigest>,
    source: ParameterSourceSpec,
}
impl MigrationCommand {
    pub(super) fn resolve_ids(&mut self, r: &mut short_ids::Resolver<'_>) -> Result<(), CliError> {
        let scoped = match self {
            Self::List { after, .. } => matches!(after, Some(Selector::Prefix(_))),
            Self::Plan {
                path,
                inputs,
                parameters,
                ..
            } => {
                matches!(path, Some(Selector::Prefix(_)))
                    || parameters
                        .iter()
                        .any(|p| matches!(p.revision, Selector::Prefix(_)))
                    || inputs
                        .iter()
                        .any(|(i, _)| matches!(i.revision, Selector::Prefix(_)))
            }
        };
        let (name, target, bound) = match self {
            Self::List {
                name,
                target,
                resolved_instance,
                ..
            }
            | Self::Plan {
                name,
                target,
                resolved_instance,
                ..
            } => (name, target, resolved_instance),
        };
        r.revision(target)?;
        if !scoped {
            return Ok(());
        }
        let cancellation = r.cancellation.clone();
        let app = r.app()?;
        let instance = app
            .resolve_instance_name(name)
            .map_err(app_error)?
            .ok_or_else(|| CliError::operation("Instance does not exist"))?;
        let target_id = resolve_revision(app, target.clone())?;
        let observation = app
            .observe_migration_compilation(instance)
            .map_err(|e| CliError::operation(e.to_string()))?;
        *bound = Some(instance);
        *target = RevisionReference::Exact(target_id.clone());
        let path = match self {
            Self::List { after, .. } => after,
            Self::Plan { path, .. } => path,
        };
        if let Some(Selector::Prefix(prefix)) = path {
            let text = format!("mp1-{prefix}");
            let mut candidates = Vec::new();
            visit_paths(&observation, &target_id, &cancellation, |c| {
                let id = c.id.to_string();
                if id.starts_with(&text) {
                    candidates.push(id);
                }
                candidates.len() < 21
            })?;
            let has_more = candidates.len() > 20;
            candidates.truncate(20);
            let selected = short_ids::unique(
                short_ids::SelectorKind::MigrationPath,
                &text,
                IdentityMatches {
                    candidates,
                    has_more,
                },
            )?;
            *path = Some(Selector::Full(selected.parse().map_err(migration_error)?));
        }
        if let Self::Plan {
            inputs, parameters, ..
        } = self
        {
            for revision in inputs
                .iter_mut()
                .map(|(key, _)| &mut key.revision)
                .chain(parameters.iter_mut().map(|p| &mut p.revision))
            {
                if let Selector::Prefix(prefix) = revision {
                    let mut candidates: Vec<_> = observation
                        .revisions
                        .iter()
                        .filter(|r| r.identity.package_id == observation.active_revision.package_id)
                        .map(|r| r.identity.content_digest.to_string())
                        .filter(|s| s[7..].starts_with(prefix.as_str()))
                        .take(21)
                        .collect();
                    let has_more = candidates.len() > 20;
                    candidates.truncate(20);
                    let selected = short_ids::unique(
                        short_ids::SelectorKind::RevisionDigest,
                        &format!("sha256:{prefix}"),
                        IdentityMatches {
                            candidates,
                            has_more,
                        },
                    )?;
                    *revision = Selector::Full(selected.parse().map_err(CliError::usage)?);
                }
            }
            let mut seen = std::collections::BTreeSet::new();
            for (key, _) in inputs {
                if !seen.insert((key.revision.clone().full(), key.input.clone())) {
                    return Err(CliError::usage("duplicate Migration target Input"));
                }
            }
            let mut seen_parameters = BTreeSet::new();
            for parameter in parameters {
                if !seen_parameters.insert((
                    parameter.revision.clone().full(),
                    parameter.source.id().clone(),
                )) {
                    return Err(CliError::usage("duplicate Migration edge parameter"));
                }
            }
        }
        Ok(())
    }
}

fn visit_paths(
    observation: &MigrationCompilationObservation,
    target: &RevisionIdentity,
    cancellation: &ActionCancellation,
    mut visit: impl FnMut(&MigrationPathCandidate) -> bool,
) -> Result<(), CliError> {
    let mut after = None;
    loop {
        if cancellation.is_requested() {
            return Err(CliError::operation("Migration path lookup cancelled"));
        }
        let page = list_migration_paths(
            &observation.revisions,
            &observation.active_revision,
            target,
            after.as_ref(),
            MIGRATION_PATH_PAGE_MAX,
        )
        .map_err(migration_error)?;
        for candidate in &page.candidates {
            if !visit(candidate) {
                return Ok(());
            }
        }
        if !page.has_more {
            return Ok(());
        }
        after = page.candidates.last().map(|c| c.id.clone());
    }
}

pub(super) fn parse(parser: &mut Parser, listing: bool) -> Result<MigrationCommand, CliError> {
    let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
    let (mut target, mut path, mut after, mut limit) = (None, None, None, None);
    let (mut plan, mut authorize, mut override_guard) = (false, false, false);
    let mut no_retain_hook_text = false;
    let mut cancel_on_output_close = false;
    let mut inputs = Vec::new();
    let mut parameters: Vec<ParameterTarget> = Vec::new();
    let mut no_trunc = false;
    let (mut startup, mut execution, mut grace) = (None, None, None);
    while let Some(argument) = parser.next().map_err(lex_error)? {
        match argument {
            Arg::Long(option @ ("param" | "param-file" | "param-stdin")) if !listing => {
                let option = option.to_owned();
                let (target, source) = if option == "param-stdin" {
                    (
                        required_value_string(parser, "Migration parameter target")?,
                        None,
                    )
                } else {
                    let value = required_parser_value(parser, "Migration parameter assignment")?;
                    let (target, value) = split_parameter_assignment(
                        value,
                        &format!("--{option}"),
                        option == "param",
                    )?;
                    (target, Some(value))
                };
                let (revision, id) = target.split_once('/').ok_or_else(|| {
                    CliError::usage("Migration parameters require <target-digest>/<parameter-id>")
                })?;
                let id = parse_parameter_id(id)?;
                let revision = revision.parse()?;
                let source = match option.as_str() {
                    "param" => ParameterSourceSpec::Text(
                        id,
                        os_string(source.expect("text assignment"), "parameter text")?,
                    ),
                    "param-file" => ParameterSourceSpec::File(
                        id,
                        PathBuf::from(source.expect("file assignment")),
                    ),
                    _ => ParameterSourceSpec::Stdin(id),
                };
                if parameters
                    .iter()
                    .any(|p| p.revision == revision && p.source.id() == source.id())
                {
                    return Err(CliError::usage("duplicate Migration edge parameter"));
                }
                if matches!(source, ParameterSourceSpec::Stdin(_))
                    && parameters
                        .iter()
                        .any(|p| matches!(p.source, ParameterSourceSpec::Stdin(_)))
                {
                    return Err(CliError::usage(
                        "only one --param-stdin source is allowed per Migration",
                    ));
                }
                parameters.push(ParameterTarget { revision, source });
            }
            Arg::Long("no-trunc") if listing && !no_trunc => no_trunc = true,
            Arg::Long("cancel-on-output-close") if !listing && !cancel_on_output_close => {
                cancel_on_output_close = true
            }
            Arg::Long("no-retain-hook-text") if !listing && !no_retain_hook_text => {
                no_retain_hook_text = true
            }
            Arg::Long("to") => set_once(
                &mut target,
                parse_revision_reference(value_string(parser, "target Revision")?)?,
                "--to",
            )?,
            Arg::Long("path") if !listing => set_once(&mut path, path_id(parser)?, "--path")?,
            Arg::Long("startup-timeout-ms") if !listing => set_once(
                &mut startup,
                parse_timeout_ms(value_string(parser, "startup timeout")?)?,
                "--startup-timeout-ms",
            )?,
            Arg::Long("execution-timeout-ms") if !listing => set_once(
                &mut execution,
                parse_timeout_ms(value_string(parser, "execution timeout")?)?,
                "--execution-timeout-ms",
            )?,
            Arg::Long("termination-grace-ms") if !listing => set_once(
                &mut grace,
                parse_timeout_ms(value_string(parser, "termination grace")?)?,
                "--termination-grace-ms",
            )?,
            Arg::Long("input-file") if !listing => {
                let value = required_parser_value(parser, "Migration Input file binding")?;
                let (target, file) = split_input_file(value)?;
                let (revision, input) = target.split_once('/').ok_or_else(|| {
                    CliError::usage("--input-file requires <target-digest>/<input-id>=<host-path>")
                })?;
                let target = InputTarget {
                    revision: revision.parse()?,
                    input: parse_input_id(input)?,
                };
                if inputs.iter().any(|(key, _)| key == &target) {
                    return Err(CliError::usage("duplicate Migration target Input"));
                }
                inputs.push((target, file));
            }
            Arg::Long("after") if listing => set_once(&mut after, path_id(parser)?, "--after")?,
            Arg::Long("limit") if listing => {
                let text = value_string(parser, "page size")?;
                let size = text
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=MIGRATION_PATH_PAGE_MAX).contains(n))
                    .filter(|_| text.bytes().all(|b| b.is_ascii_digit()))
                    .ok_or_else(|| CliError::usage("--limit must be between 1 and 100"))?;
                set_once(&mut limit, size, "--limit")?;
            }
            Arg::Long("plan") if !listing && !plan => plan = true,
            Arg::Long("authorize-declassification") if !listing && !authorize => authorize = true,
            Arg::Long("authorize-recovery-override") if !listing && !override_guard => {
                override_guard = true
            }
            other => return Err(CliError::usage(other.unexpected().to_string())),
        }
    }
    let target = target.ok_or_else(|| CliError::usage("missing --to"))?;
    if listing {
        Ok(MigrationCommand::List {
            no_trunc,
            resolved_instance: None,
            name,
            target,
            after,
            limit: limit.unwrap_or(MIGRATION_PATH_PAGE_DEFAULT),
        })
    } else {
        Ok(MigrationCommand::Plan {
            resolved_instance: None,
            no_retain_hook_text,
            cancel_on_output_close,
            name,
            target,
            path,
            plan,
            authorize,
            override_guard,
            inputs,
            parameters,
            policy: HookRuntimePolicy::from_millis(startup, execution, grace.or(Some(5_000)))
                .map_err(CliError::usage)?,
        })
    }
}
fn path_id(parser: &mut Parser) -> Result<Selector<MigrationPathId>, CliError> {
    value_string(parser, "Migration path ID")?.parse()
}
fn migration_error(error: MigrationError) -> CliError {
    CliError::operation(error.to_string())
}

fn read_migration_parameters(
    revisions: &[MigrationRevision],
    selected: &[RevisionIdentity],
    supplied: Vec<ParameterTarget>,
    stdin: &mut dyn Read,
    cancellation: &ActionCancellation,
) -> Result<Vec<MigrationParameterInput>, CliError> {
    let mut groups: std::collections::BTreeMap<MigrationEdgeIdentity, Vec<ParameterSourceSpec>> =
        Default::default();
    let has_stdin = supplied
        .iter()
        .any(|p| matches!(p.source, ParameterSourceSpec::Stdin(_)));
    for input in supplied {
        let target = input.revision.full();
        let pair = selected
            .windows(2)
            .find(|pair| pair[1].content_digest == target)
            .ok_or_else(|| migration_error(MigrationError::InvalidParameterTarget))?;
        groups
            .entry(MigrationEdgeIdentity {
                source: pair[0].content_digest,
                target,
            })
            .or_default()
            .push(input.source);
    }
    let mut contexts = Vec::new();
    for pair in selected.windows(2) {
        let scope = MigrationEdgeIdentity {
            source: pair[0].content_digest,
            target: pair[1].content_digest,
        };
        let target = revisions
            .iter()
            .find(|r| r.identity == pair[1])
            .ok_or_else(|| migration_error(MigrationError::MissingRevision))?;
        let digest = Sha256Digest::from_bytes(*pair[0].content_digest.as_bytes());
        let site = ServiceHookSite::Migration(digest.clone());
        let declarations = target
            .content
            .core
            .parameters(&site)
            .ok_or_else(|| migration_error(MigrationError::InvalidEdge))?;
        let hook = target
            .content
            .core
            .migration_declaration(&digest)
            .ok_or_else(|| migration_error(MigrationError::InvalidEdge))?
            .hook;
        let terminal = hook
            .as_ref()
            .map_or(TerminalContractV1::None, |hook| hook.io.terminal);
        if has_stdin && terminal == TerminalContractV1::Interactive {
            return Err(CliError::usage(
                "Migration paths with interactive Hooks do not accept --param-stdin",
            ));
        }
        let sources = groups.remove(&scope).unwrap_or_default();
        validate_parameter_source_shape(&sources)?;
        for source in &sources {
            if !declarations.iter().any(|p| &p.id == source.id()) {
                return Err(migration_error(MigrationError::Parameters {
                    edge: scope.clone(),
                    error: ActionResolutionError::UnknownParameter(source.id().clone()),
                }));
            }
        }
        for declaration in declarations {
            if declaration.default.is_none() && !sources.iter().any(|p| p.id() == &declaration.id) {
                return Err(migration_error(MigrationError::Parameters {
                    edge: scope.clone(),
                    error: ActionResolutionError::MissingParameter(declaration.id.clone()),
                }));
            }
        }
        contexts.push((scope, declarations, terminal, sources));
    }
    let mut result = Vec::new();
    for (scope, declarations, terminal, sources) in contexts {
        for parameter in
            read_operation_parameter_sources(declarations, terminal, &sources, stdin, cancellation)?
        {
            result.push(MigrationParameterInput {
                edge: scope.clone(),
                parameter,
            });
        }
    }
    Ok(result)
}

pub(super) fn execute(
    command: MigrationCommand,
    root: &Path,
    stdin: &mut dyn Read,
    cancellation: &ActionCancellation,
    format: reply::OutputContext,
) -> Result<(), CliError> {
    if cancellation.is_requested() {
        return Err(CliError::operation(
            "Migration query cancelled; no Run was created",
        ));
    }
    let app = PactrunApplication::open_read_only(root).map_err(app_error)?;
    let (name, target) = match &command {
        MigrationCommand::List { name, target, .. }
        | MigrationCommand::Plan { name, target, .. } => (name, target),
    };
    // All renderers share the same resolved identity and compilation snapshot.
    let target = resolve_revision(&app, target.clone())?;
    let bound = match &command {
        MigrationCommand::List {
            resolved_instance, ..
        }
        | MigrationCommand::Plan {
            resolved_instance, ..
        } => *resolved_instance,
    };
    let instance = match bound {
        Some(id) => id,
        None => app
            .resolve_instance_name(name)
            .map_err(app_error)?
            .ok_or_else(|| CliError::operation("Instance does not exist"))?,
    };
    let observation = app
        .observe_migration_compilation(instance)
        .map_err(|e| CliError::operation(e.to_string()))?;
    match command {
        MigrationCommand::List { after, limit, .. } => {
            let after = after.map(Selector::full);
            let page = list_migration_paths(
                &observation.revisions,
                &observation.active_revision,
                &target,
                after.as_ref(),
                limit,
            )
            .map_err(migration_error)?;
            let ids: Vec<_> = page.candidates.iter().map(|c| c.id.to_string()).collect();
            let mut widths = vec![12; ids.len()];
            if !ids.is_empty() {
                visit_paths(&observation, &target, cancellation, |candidate| {
                    let other = candidate.id.to_string();
                    for (id, width) in ids.iter().zip(&mut widths) {
                        if *id != other {
                            let shared = id[4..68]
                                .bytes()
                                .zip(other[4..68].bytes())
                                .take_while(|(a, b)| a == b)
                                .count();
                            *width = (*width).max((shared + 1).min(64));
                        }
                    }
                    true
                })?;
            }
            {
                presentation::emit_result(
                    format,
                    "instance migration-paths",
                    &capability_presentation::Presented {
                        value: migration_presentation::Paths::new(&page, &widths),
                        presentation: path_presentation(&app, &page)?,
                    },
                )
            }
        }
        MigrationCommand::Plan {
            no_retain_hook_text: _,
            cancel_on_output_close: _,
            path,
            authorize,
            plan: plan_only,
            override_guard,
            inputs,
            parameters,
            policy,
            ..
        } => {
            let inputs: Vec<_> = inputs
                .into_iter()
                .map(|(key, path)| {
                    (
                        MigrationTargetInput {
                            revision: key.revision.full(),
                            input: key.input,
                        },
                        path,
                    )
                })
                .collect();
            let selected = match path {
                Some(id) => id
                    .full()
                    .resolve(
                        &observation.revisions,
                        &observation.active_revision,
                        &target,
                    )
                    .map_err(migration_error)?,
                None => match resolve_migration_path(
                    &observation.revisions,
                    &observation.active_revision,
                    &target,
                    &[],
                ) {
                    Ok(path) => path,
                    Err(MigrationError::AmbiguousPath) => {
                        let page = list_migration_paths(
                            &observation.revisions,
                            &observation.active_revision,
                            &target,
                            None,
                            MIGRATION_PATH_PAGE_DEFAULT,
                        )
                        .map_err(migration_error)?;
                        {
                            let mut error = migration_error(MigrationError::AmbiguousPath);
                            error.partial = Some(presentation::PartialResult::MigrationPaths(
                                Box::new(migration_presentation::Paths::new(
                                    &page,
                                    &vec![64; page.candidates.len()],
                                )),
                            ));
                            return Err(error);
                        }
                    }
                    Err(error) => return Err(migration_error(error)),
                },
            };
            let id = MigrationPathId::for_path(&selected).map_err(migration_error)?;
            for revision in &selected {
                let node = observation
                    .revisions
                    .iter()
                    .find(|node| &node.identity == revision)
                    .ok_or_else(|| migration_error(MigrationError::MissingRevision))?;
                format.version().require_core(&node.content.core)?;
            }
            let mut intent = TransitionRevision {
                parameters: Vec::new(),
                instance,
                expected_state_version: observation.state_version,
                source: observation.active_revision.clone(),
                target,
                path: MigrationPathSelection::Exact(selected.clone()),
                operator_inputs: inputs.iter().map(|(key, _)| key.clone()).collect(),
                authorize_declassification: authorize,
            };
            // Validate the symbolic chain before opening parameter sources.
            build_migration_binding_plan(&intent, &observation.revisions, &observation.bindings)
                .map_err(migration_error)?;
            intent.parameters = read_migration_parameters(
                &observation.revisions,
                &selected,
                parameters,
                stdin,
                cancellation,
            )?;
            let plan = crate::workflow::compile_migration(
                &app,
                &crate::workflow::PlatformHostLauncherLookup,
                &intent,
                &launcher_search_directories(),
            )
            .map_err(app_error)?;
            if !plan_only {
                if format.requires_noninteractive() {
                    for edge in plan.edges() {
                        if let Some(hook) = &edge.bindings.declaration().hook {
                            require_json_terminal_free(hook.io.terminal)?;
                        }
                    }
                }
                let writer = PactrunApplication::open(root).map_err(app_error)?;
                let inputs = inputs
                    .into_iter()
                    .map(|(key, path)| {
                        let file = std::fs::File::open(path).map_err(|error| {
                            use crate::application::migration_input::{Failure, Phase, Reason};
                            app_error(ApplicationError::MigrationInputAcquisition(Box::new(
                                Failure::new(
                                    &plan,
                                    &key,
                                    Phase::OpenSource,
                                    Reason::from_io(&error),
                                ),
                            )))
                        })?;
                        Ok((key, file))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let run = match writer.accept_migration_inputs(
                    plan,
                    override_guard,
                    cancellation.clone(),
                    inputs,
                    policy,
                ) {
                    Ok(run) => run,
                    Err(ApplicationError::Execution(ExecutorError::Persistence {
                        run: Some(run),
                        ..
                    })) => run,
                    Err(error) => return Err(app_error(error)),
                };
                loop {
                    match writer.advance_owner_continuation(run) {
                        Ok(true) => break,
                        Ok(false) => {}
                        Err(_) => {
                            cancellation
                                .delivery
                                .progress(run, crate::hook::delivery::CorePhase::RetryingStorage);
                            thread::sleep(Duration::from_secs(1));
                        }
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                cancellation.diagnostics.finish();
                let inspection=writer.managed_run_inspection(run).map_err(|error|app_error(error).with_run_context(run))?.ok_or_else(||CliError::operation("Migration acceptance was proven absent; no replacement Run was created").with_run_context(run))?;
                {
                    let result = execution_presentation::Inspection::for_version(
                        &inspection,
                        format.version(),
                    )?;
                    if matches!(&inspection.run.state, RunState::Finished(o) if o.outcome == RunOutcome::Succeeded)
                    {
                        let current = writer
                            .load_instance(instance)
                            .map_err(|error| app_error(error).with_run_context(run))?
                            .ok_or_else(|| {
                                CliError::operation("Instance disappeared after Migration")
                                    .with_run_context(run)
                            })?;
                        let definitions = app
                            .inspect_revision_definitions(std::slice::from_ref(
                                &current.active_revision,
                            ))
                            .map_err(|error| app_error(error).with_run_context(run))?;
                        let definition=definitions.first().ok_or_else(||CliError::operation("Revision definition unavailable after Migration; inspect the retained Run").with_run_context(run))?;
                        return presentation::emit_result(
                            format,
                            "instance migrate",
                            &migration_presentation::Completed {
                                inspection: result,
                                current_instance: presentation::Instance::for_version(
                                    &current,
                                    &definition.core,
                                    format.version(),
                                )
                                .map_err(|error| error.with_run_context(run))?,
                            },
                        );
                    }
                    let mut error = CliError::operation(
                        "Migration did not succeed; inspect its last committed boundary before deciding what to do next",
                    );
                    error.partial = Some(presentation::PartialResult::Inspection(Box::new(result)));
                    return Err(error);
                }
            }
            let requested: Vec<_> = plan
                .edges()
                .iter()
                .map(|edge| {
                    (
                        edge.bindings.target().clone(),
                        capability_presentation::Selection::Edge(
                            edge.bindings.source().content_digest,
                        ),
                    )
                })
                .collect();
            let author = capability_presentation::load(&app, &requested)?;
            {
                presentation::emit_result(
                    format,
                    "instance migrate",
                    &capability_presentation::Presented {
                        value: migration_presentation::Plan::new(&id, &plan, &policy)
                            .with_version(&plan, format.version())?,
                        presentation: author,
                    },
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test-ID: PR-TEST-0719
    // Verifies: PR-REQ-0390
    #[test]
    fn migration_parameter_sources_are_edge_scoped_and_stdin_is_single_use() {
        let a = format!("sha256:{}", "1".repeat(64));
        let b = format!("sha256:{}", "2".repeat(64));
        let run = |args: Vec<String>| {
            let mut parser = Parser::from_args(
                ["demo".to_owned(), "--to".to_owned(), "app:next".to_owned()]
                    .into_iter()
                    .chain(args),
            );
            parse(&mut parser, false)
        };
        let MigrationCommand::Plan { parameters, .. } = run(vec![
            "--param".into(),
            format!("{a}/mode=false"),
            "--param-file".into(),
            format!("{b}/mode=directory/private=file"),
        ])
        .unwrap() else {
            panic!("plan");
        };
        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters[0].source.id().as_str(), "mode");
        assert!(
            matches!(&parameters[0].source,ParameterSourceSpec::Text(_,value) if value=="false")
        );
        assert!(
            matches!(&parameters[1].source,ParameterSourceSpec::File(_,path) if path==Path::new("directory/private=file"))
        );
        assert!(
            run(vec![
                "--param".into(),
                format!("{a}/mode=false"),
                "--param".into(),
                format!("{a}/mode=true")
            ])
            .is_err()
        );
        assert!(
            run(vec![
                "--param-stdin".into(),
                format!("{a}/mode"),
                "--param-stdin".into(),
                format!("{b}/mode")
            ])
            .is_err()
        );
        assert!(run(vec!["--param".into(), "mode=false".into()]).is_err());
    }

    // Test-ID: PR-TEST-0324
    // Verifies: PR-REQ-0315, PR-REQ-0307
    #[test]
    fn migration_file_selector_preserves_non_unicode_native_path_units() {
        let prefix = format!("sha256:{}/config=", "ab".repeat(32));
        #[cfg(unix)]
        let (argument, expected) = {
            use std::os::unix::ffi::OsStringExt;
            let path = b"directory/opaque-\xff=file".to_vec();
            let mut argument = prefix.into_bytes();
            argument.extend(&path);
            (OsString::from_vec(argument), OsString::from_vec(path))
        };
        #[cfg(windows)]
        let (argument, expected) = {
            use std::os::windows::ffi::OsStringExt;
            let mut path = "C:\\directory\\opaque-".encode_utf16().collect::<Vec<_>>();
            path.push(0xdcff);
            path.extend("=file".encode_utf16());
            let mut argument = prefix.encode_utf16().collect::<Vec<_>>();
            argument.extend(&path);
            (OsString::from_wide(&argument), OsString::from_wide(&path))
        };
        let mut parser = Parser::from_args(vec![
            OsString::from("demo"),
            "--to".into(),
            "alias:next".into(),
            "--input-file".into(),
            argument,
            "--plan".into(),
        ]);
        let MigrationCommand::Plan { inputs, .. } = parse(&mut parser, false).unwrap() else {
            panic!("Migration plan")
        };
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].0.input.as_str(), "config");
        assert_eq!(inputs[0].1.as_os_str(), expected.as_os_str());
    }
}

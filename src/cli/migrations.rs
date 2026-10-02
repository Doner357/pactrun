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
        policy: HookRuntimePolicy,
        resolved_instance: Option<InstanceId>,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct InputTarget {
    revision: Selector<RevisionContentDigest>,
    input: InputIdentity,
}
impl MigrationCommand {
    pub(super) fn resolve_ids(&mut self, r: &mut short_ids::Resolver<'_>) -> Result<(), CliError> {
        let scoped = match self {
            Self::List { after, .. } => matches!(after, Some(Selector::Prefix(_))),
            Self::Plan { path, inputs, .. } => {
                matches!(path, Some(Selector::Prefix(_)))
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
        if let Self::Plan { inputs, .. } = self {
            for (key, _) in inputs.iter_mut() {
                if let Selector::Prefix(prefix) = &key.revision {
                    let mut candidates: Vec<_> = observation
                        .revisions
                        .iter()
                        .filter(|r| r.identity.package_id == observation.active_revision.package_id)
                        .map(|r| r.identity.content_digest.to_string())
                        .filter(|s| s[7..].starts_with(prefix))
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
                    key.revision = Selector::Full(selected.parse().map_err(CliError::usage)?);
                }
            }
            let mut seen = std::collections::BTreeSet::new();
            for (key, _) in inputs {
                if !seen.insert((key.revision.clone().full(), key.input.clone())) {
                    return Err(CliError::usage("duplicate Migration target Input"));
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
    let mut no_trunc = false;
    let (mut startup, mut execution, mut grace) = (None, None, None);
    while let Some(argument) = parser.next().map_err(lex_error)? {
        match argument {
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

pub(super) fn execute(
    command: MigrationCommand,
    root: &Path,
    output: &mut dyn Write,
    cancellation: &ActionCancellation,
    format: presentation::Format,
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
    // Resolve labels exactly using the same semantics as other human commands.
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
        MigrationCommand::List {
            after,
            limit,
            no_trunc,
            ..
        } => {
            let after = after.map(Selector::full);
            let page = list_migration_paths(
                &observation.revisions,
                &observation.active_revision,
                &target,
                after.as_ref(),
                limit,
            )
            .map_err(migration_error)?;
            if format == presentation::Format::Json {
                return presentation::render(
                    format,
                    "instance migration-paths",
                    &capability_presentation::Presented {
                        value: migration_presentation::Paths::from(&page),
                        presentation: path_presentation(&app, &page)?,
                    },
                    output,
                    |_, _| unreachable!(),
                );
            }
            let labels = path_labels(&observation, &target, &page, no_trunc, cancellation)?;
            write_page(output, &page, &labels)?;
            capability_presentation::write(output, &path_presentation(&app, &page)?, true)
        }
        MigrationCommand::Plan {
            no_retain_hook_text: _,
            cancel_on_output_close: _,
            path,
            authorize,
            plan: plan_only,
            override_guard,
            inputs,
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
                        if format == presentation::Format::Json {
                            let mut error = migration_error(MigrationError::AmbiguousPath);
                            error.partial = Some(presentation::PartialResult::MigrationPaths(
                                Box::new((&page).into()),
                            ));
                            return Err(error);
                        }
                        let labels =
                            path_labels(&observation, &target, &page, false, cancellation)?;
                        write_page(output, &page, &labels)?;
                        return Err(migration_error(MigrationError::AmbiguousPath));
                    }
                    Err(error) => return Err(migration_error(error)),
                },
            };
            let id = MigrationPathId::for_path(&selected).map_err(migration_error)?;
            let intent = TransitionRevision {
                instance,
                expected_state_version: observation.state_version,
                source: observation.active_revision,
                target,
                path: MigrationPathSelection::Exact(selected),
                operator_inputs: inputs.iter().map(|(key, _)| key.clone()).collect(),
                authorize_declassification: authorize,
            };
            let plan = crate::workflow::compile_migration(
                &app,
                &crate::workflow::PlatformHostLauncherLookup,
                &intent,
                &launcher_search_directories(),
            )
            .map_err(app_error)?;
            if !plan_only {
                if format == presentation::Format::Json {
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
                let mut warned = false;
                loop {
                    match writer.advance_owner_continuation(run) {
                        Ok(true) => break,
                        Ok(false) => {}
                        Err(_) => {
                            if !warned && format == presentation::Format::Human {
                                let _ = writeln!(
                                    output,
                                    "Run {run}: owner retained while waiting for durable Migration state"
                                );
                                warned = true;
                            }
                            thread::sleep(Duration::from_secs(1));
                        }
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                let inspection=writer.managed_run_inspection(run).map_err(|error|app_error(error).with_run_context(run))?.ok_or_else(||CliError::operation("Migration acceptance was proven absent; no replacement Run was created").with_run_context(run))?;
                if format == presentation::Format::Json {
                    let result = execution_presentation::Inspection::from(&inspection);
                    if matches!(&inspection.run.state, RunState::Finished(o) if o.outcome == RunOutcome::Succeeded)
                    {
                        let current = writer
                            .load_instance(instance)
                            .map_err(|error| app_error(error).with_run_context(run))?
                            .ok_or_else(|| {
                                CliError::operation("Instance disappeared after Migration")
                                    .with_run_context(run)
                            })?;
                        return presentation::render(
                            format,
                            "instance migrate",
                            &migration_presentation::Completed {
                                inspection: result,
                                current_instance: (&current).into(),
                            },
                            output,
                            |_, _| unreachable!(),
                        );
                    }
                    let mut error = CliError::operation(
                        "Migration did not succeed; inspect its last committed boundary before deciding what to do next",
                    );
                    error.partial = Some(presentation::PartialResult::Inspection(Box::new(result)));
                    return Err(error);
                }
                if matches!(&inspection.run.state, RunState::Finished(o) if o.outcome == RunOutcome::Succeeded)
                {
                    writeln!(output, "run: {run}\noutcome: succeeded").map_err(io_operation)?;
                } else {
                    snapshots::write_run(output, &inspection)?;
                }
                return match &inspection.run.state {
                    RunState::Finished(outcome) if outcome.outcome == RunOutcome::Succeeded => {
                        let current = writer
                            .load_instance(instance)
                            .map_err(app_error)?
                            .ok_or_else(|| CliError::operation("Instance disappeared"))?;
                        writeln!(
                            output,
                            "required_inputs_satisfied: {}",
                            current.required_inputs_satisfied
                        )
                        .map_err(io_operation)
                    }
                    RunState::Finished(outcome) => Err(CliError::operation(format!(
                        "Migration Run {run} finished as {}; last committed boundary retained",
                        format_outcome(outcome.outcome)
                    ))),
                    RunState::Running(_) => Err(CliError::operation(
                        "Migration has not reached a durable terminal state",
                    )),
                };
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
            if format == presentation::Format::Json {
                return presentation::render(
                    format,
                    "instance migrate",
                    &capability_presentation::Presented {
                        value: migration_presentation::Plan::new(&id, &plan, &policy),
                        presentation: author,
                    },
                    output,
                    |_, _| unreachable!(),
                );
            }
            writeln!(
                output,
                "Migration preview\npath_id: {id}\nexpected_state_version: {}",
                plan.expected_state_version()
            )
            .map_err(io_operation)?;
            capability_presentation::write(output, &author, false)?;
            if !plan.operator_inputs().is_empty() {
                writeln!(output, "operator_input_acquisition: not_performed")
                    .map_err(io_operation)?;
            }
            let timeout = |duration: Option<Duration>| {
                duration
                    .map(|d| d.as_millis().to_string())
                    .unwrap_or_else(|| "unlimited".to_owned())
            };
            writeln!(
                output,
                "startup_timeout_ms: {}\nexecution_timeout_ms: {}\ntermination_grace_ms: {}",
                timeout(policy.startup_timeout),
                timeout(policy.action_timeout),
                timeout(policy.termination_grace)
            )
            .map_err(io_operation)?;
            for (index, edge) in plan.edges().iter().enumerate() {
                let bindings = &edge.bindings;
                writeln!(
                    output,
                    "edge {}: {} -> {}",
                    index + 1,
                    format_revision(bindings.source()),
                    format_revision(bindings.target())
                )
                .map_err(io_operation)?;
                if let Some(service) = &edge.service {
                    writeln!(output, "  service_transform: {}", service.transform)
                        .map_err(io_operation)?;
                    writeln!(output, "  live_service_observation: not_performed")
                        .map_err(io_operation)?;
                    if !service.consumed_storages.is_empty()
                        || !service.consumed_resources.is_empty()
                    {
                        writeln!(
                            output,
                            "  Binding consumption does not delete service bytes."
                        )
                        .map_err(io_operation)?;
                    }
                    for id in &service.created_storages {
                        writeln!(output, "  create_storage: {}", id.as_str())
                            .map_err(io_operation)?;
                    }
                    for id in &service.consumed_storages {
                        writeln!(output, "  consume_source_storage_binding: {}", id.as_str())
                            .map_err(io_operation)?;
                    }
                    for id in &service.consumed_resources {
                        writeln!(output, "  consume_source_resource_binding: {}", id.as_str())
                            .map_err(io_operation)?;
                    }
                    for (presence, resource) in &service.create_presence {
                        writeln!(
                            output,
                            "  create_resource: {} presence={}",
                            resource.declaration.id.as_str(),
                            match presence {
                                ServiceCreatePresence::Any => "any",
                                ServiceCreatePresence::Present => "present",
                                ServiceCreatePresence::Absent => "absent",
                            }
                        )
                        .map_err(io_operation)?;
                    }
                    for (requirement, _) in &service.requires {
                        writeln!(
                            output,
                            "  service_requires: {}/{} {}: required {}",
                            match requirement.reference.view {
                                ServiceView::Current => "current",
                                ServiceView::Source => "source",
                                ServiceView::Target => "target",
                            },
                            match requirement.reference.role {
                                ServiceRole::Active => "active",
                                ServiceRole::Retained => "retained",
                            },
                            match &requirement.reference.scope {
                                ServiceScope::Resource(id) => format!("resource {}", id.as_str()),
                                ServiceScope::Storage(id) => format!("storage {}", id.as_str()),
                            },
                            match requirement.presence {
                                ServicePresenceRequirement::Present => "present",
                                ServicePresenceRequirement::Absent => "absent",
                            }
                        )
                        .map_err(io_operation)?;
                    }
                }
                for input in plan
                    .operator_inputs()
                    .iter()
                    .filter(|i| i.revision == bindings.target().content_digest)
                {
                    writeln!(output, "  input file: {}", input.input.as_str())
                        .map_err(io_operation)?;
                }
                for source in &bindings.declaration().requires_source {
                    writeln!(
                        output,
                        "  requires_source: {}/{}",
                        match source.role {
                            InputBindingRoleV1::Active => "active",
                            InputBindingRoleV1::Retained => "retained",
                        },
                        source.input_id.as_str()
                    )
                    .map_err(io_operation)?;
                }
                for input in &bindings.declaration().requires_target {
                    writeln!(output, "  requires_target: {}", input.as_str())
                        .map_err(io_operation)?;
                }
                for transition in &bindings.declaration().transitions {
                    let kind = match transition {
                        MigrationTransitionV1::Carry { .. } => "carry",
                        MigrationTransitionV1::Declassify { .. } => "declassify",
                        MigrationTransitionV1::Keep { .. } => "keep",
                        MigrationTransitionV1::Discard { .. } => "discard",
                    };
                    writeln!(
                        output,
                        "  transition: {kind} {}/{} -> {}",
                        match transition.source().role {
                            InputBindingRoleV1::Active => "active",
                            InputBindingRoleV1::Retained => "retained",
                        },
                        transition.source().input_id.as_str(),
                        transition
                            .target()
                            .map(InputIdentity::as_str)
                            .unwrap_or("none")
                    )
                    .map_err(io_operation)?;
                }
                for input in &bindings.declaration().produces_target {
                    writeln!(output, "  mandatory_hook_output: {}", input.as_str())
                        .map_err(io_operation)?;
                }
                writeln!(
                    output,
                    "  hook: {}\n  predicted_required_inputs_satisfied: {}",
                    if edge.launch.is_some() { "yes" } else { "none" },
                    bindings.required_inputs_satisfied()
                )
                .map_err(io_operation)?;
            }
            writeln!(output, "Mode: preview").map_err(io_operation)
        }
    }
}

fn path_labels(
    observation: &MigrationCompilationObservation,
    target: &RevisionIdentity,
    page: &MigrationPathPage,
    full: bool,
    cancellation: &ActionCancellation,
) -> Result<Vec<String>, CliError> {
    let ids: Vec<_> = page.candidates.iter().map(|c| c.id.to_string()).collect();
    if full {
        return Ok(ids);
    }
    let mut sizes = vec![16usize; ids.len()];
    visit_paths(observation, target, cancellation, |candidate| {
        let other = candidate.id.to_string();
        for (id, size) in ids.iter().zip(&mut sizes) {
            if id != &other {
                *size = (*size).max(
                    id.bytes()
                        .zip(other.bytes())
                        .take_while(|(a, b)| a == b)
                        .count()
                        + 1,
                );
            }
        }
        true
    })?;
    Ok(ids
        .into_iter()
        .zip(sizes)
        .map(|(id, n)| {
            if n >= 68 || n >= id.len() {
                id
            } else {
                id[..n].into()
            }
        })
        .collect())
}

fn write_page(
    output: &mut dyn Write,
    page: &MigrationPathPage,
    labels: &[String],
) -> Result<(), CliError> {
    writeln!(output, "Migration paths").map_err(io_operation)?;
    for (candidate, label) in page.candidates.iter().zip(labels) {
        writeln!(
            output,
            "path_id: {}\n  route: {}\n  edge_count: {}",
            label,
            candidate
                .revisions
                .iter()
                .map(format_revision)
                .collect::<Vec<_>>()
                .join(" -> "),
            candidate.revisions.len() - 1
        )
        .map_err(io_operation)?;
    }
    writeln!(
        output,
        "shown: {}\nmore_paths: {}",
        page.candidates.len(),
        page.has_more
    )
    .map_err(io_operation)?;
    if page.has_more {
        writeln!(
            output,
            "next_after: {}",
            page.candidates.last().expect("nonempty limited page").id
        )
        .map_err(io_operation)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

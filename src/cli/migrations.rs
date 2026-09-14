//! Exact path discovery, compilation, and managed Migration execution.
use super::*;
use crate::domain::*;

pub(super) enum MigrationCommand {
    List {
        name: InstanceName,
        target: RevisionReference,
        after: Option<MigrationPathId>,
        limit: usize,
    },
    Plan {
        name: InstanceName,
        target: RevisionReference,
        path: Option<MigrationPathId>,
        plan: bool,
        authorize: bool,
        override_guard: bool,
        inputs: Vec<(MigrationTargetInput, PathBuf)>,
        policy: HookRuntimePolicy,
    },
}

pub(super) fn parse(parser: &mut Parser, listing: bool) -> Result<MigrationCommand, CliError> {
    let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
    let (mut target, mut path, mut after, mut limit) = (None, None, None, None);
    let (mut plan, mut authorize, mut override_guard) = (false, false, false);
    let mut inputs = Vec::new();
    let (mut startup, mut execution, mut grace) = (None, None, None);
    while let Some(argument) = parser.next().map_err(lex_error)? {
        match argument {
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
                let target = MigrationTargetInput {
                    revision: revision.parse().map_err(CliError::usage)?,
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
            name,
            target,
            after,
            limit: limit.unwrap_or(MIGRATION_PATH_PAGE_DEFAULT),
        })
    } else {
        Ok(MigrationCommand::Plan {
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
fn path_id(parser: &mut Parser) -> Result<MigrationPathId, CliError> {
    value_string(parser, "Migration path ID")?
        .parse()
        .map_err(|error: MigrationError| CliError::usage(error.to_string()))
}
fn migration_error(error: MigrationError) -> CliError {
    CliError::operation(error.to_string())
}

pub(super) fn execute(
    command: MigrationCommand,
    root: &Path,
    output: &mut dyn Write,
    cancellation: &ActionCancellation,
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
    let instance = app
        .resolve_instance_name(name)
        .map_err(app_error)?
        .ok_or_else(|| CliError::operation("Instance does not exist"))?;
    let observation = app
        .observe_migration_compilation(instance)
        .map_err(|e| CliError::operation(e.to_string()))?;
    match command {
        MigrationCommand::List { after, limit, .. } => {
            let page = list_migration_paths(
                &observation.revisions,
                &observation.active_revision,
                &target,
                after.as_ref(),
                limit,
            )
            .map_err(migration_error)?;
            write_page(output, &page)
        }
        MigrationCommand::Plan {
            path,
            authorize,
            plan: plan_only,
            override_guard,
            inputs,
            policy,
            ..
        } => {
            let selected = match path {
                Some(id) => id
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
                        write_page(output, &page)?;
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
                let writer = PactrunApplication::open(root).map_err(app_error)?;
                let inputs = inputs
                    .into_iter()
                    .map(|(key, path)| {
                        std::fs::File::open(path)
                            .map(|file| (key, file))
                            .map_err(|_| CliError::operation("Migration Input acquisition failed"))
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
                            if !warned {
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
                let inspection=writer.managed_run_inspection(run).map_err(app_error)?.ok_or_else(||CliError::operation("Migration acceptance was proven absent; no replacement Run was created"))?;
                snapshots::write_run(output, &inspection)?;
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
            writeln!(output, "Migration plan (read-only; no Run created)\npath_id: {id}\nexpected_state_version: {}", plan.expected_state_version()).map_err(io_operation)?;
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
                for input in plan
                    .operator_inputs()
                    .iter()
                    .filter(|i| i.revision == bindings.target().content_digest)
                {
                    writeln!(
                        output,
                        "  operator_input: {} (acquisition not performed)",
                        input.input.as_str()
                    )
                    .map_err(io_operation)?;
                }
                for source in &bindings.declaration().requires_source {
                    writeln!(
                        output,
                        "  requires_source: {:?}/{}",
                        source.role,
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
                        "  transition: {kind} {:?}/{} -> {}",
                        transition.source().role,
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
            writeln!(
                output,
                "Admission not performed; plan references do not reserve resources."
            )
            .map_err(io_operation)
        }
    }
}

fn write_page(output: &mut dyn Write, page: &MigrationPathPage) -> Result<(), CliError> {
    writeln!(output, "Migration path candidates (relationally valid declared edges; requirements, runtime and authorization not evaluated)").map_err(io_operation)?;
    for candidate in &page.candidates {
        writeln!(
            output,
            "path_id: {}\n  route: {}\n  edge_count: {}",
            candidate.id,
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
    writeln!(
        output,
        "No Run, pin, reservation, or persistent path registry was created."
    )
    .map_err(io_operation)
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

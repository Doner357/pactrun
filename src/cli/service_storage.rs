//! Closed human ServiceStorage queries. No write/cleanup/content-command route.
use super::*;
use crate::domain::{
    ObservedServiceObjectKind, ServiceAccessError, ServiceAccessIntent, ServiceObservation,
    ServiceObservationCause, ServiceReadExposure, ServiceResourceAssociation,
    ServiceResourceIdentity, ServiceResourceKind, ServiceRole, ServiceUserMutation,
};

pub(super) struct ServiceCommand {
    name: InstanceName,
    role: ServiceRole,
    operation: ServiceOperation,
}
enum ServiceOperation {
    Storages,
    Resources,
    Show(ServiceResourceIdentity),
    Observe(ServiceResourceIdentity),
    Locate(ServiceResourceIdentity, ServiceAccessIntent),
}

pub(super) fn parse(parser: &mut Parser, storage: bool) -> Result<Command, CliError> {
    let operation = required_value_string(parser, "service command")?;
    if storage && operation == "detached" {
        return retirements::parse_detached(parser).map(Command::Retirement);
    }
    if (storage && operation != "list")
        || (!storage && !matches!(operation.as_str(), "list" | "show" | "observe" | "locate"))
    {
        return Err(CliError::usage("unsupported ServiceStorage command"));
    }
    let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
    let resource = if operation == "list" {
        None
    } else {
        Some(
            ServiceResourceIdentity::parse(required_value_string(parser, "resource identity")?)
                .map_err(|_| CliError::usage("invalid resource identity"))?,
        )
    };
    let mut retained = false;
    let mut intent = None;
    while let Some(arg) = parser.next().map_err(lex_error)? {
        match arg {
            Arg::Long("retained") if !retained => retained = true,
            Arg::Long("intent") if operation == "locate" => {
                let value = match value_string(parser, "access intent")?.as_str() {
                    "read" => ServiceAccessIntent::Read,
                    "write" => ServiceAccessIntent::Write,
                    _ => return Err(CliError::usage("--intent must be read or write")),
                };
                set_once(&mut intent, value, "--intent")?;
            }
            _ => {
                return Err(CliError::usage(
                    "unsupported or duplicate ServiceStorage argument",
                ));
            }
        }
    }
    let operation = match operation.as_str() {
        "list" if storage => ServiceOperation::Storages,
        "list" => ServiceOperation::Resources,
        "show" => ServiceOperation::Show(resource.expect("resource parsed")),
        "observe" => ServiceOperation::Observe(resource.expect("resource parsed")),
        "locate" => ServiceOperation::Locate(
            resource.expect("resource parsed"),
            intent.ok_or_else(|| CliError::usage("locate requires --intent read or write"))?,
        ),
        _ => unreachable!("closed operation"),
    };
    Ok(Command::ServiceStorage(ServiceCommand {
        name,
        role: if retained {
            ServiceRole::Retained
        } else {
            ServiceRole::Active
        },
        operation,
    }))
}

pub(super) fn execute(
    command: ServiceCommand,
    root: &Path,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let app = PactrunApplication::open_read_only(root).map_err(safe_error)?;
    let ServiceCommand {
        name,
        role,
        operation,
    } = command;
    match operation {
        ServiceOperation::Storages => {
            let state = app.load_instance_services(&name).map_err(safe_error)?;
            for storage in state.storages.iter().filter(|s| s.role == role) {
                writeln!(
                    out,
                    "{} role={} revision={}",
                    storage.declaration.id.as_str(),
                    role_name(role),
                    format_revision(&storage.declaration_revision)
                )
                .map_err(io_operation)?;
            }
            if role == ServiceRole::Retained {
                for allocation in &state.preserved {
                    writeln!(out, "preserved allocation={} origin_storage={} origin_revision={} origin_run={}",
                        allocation.allocation, allocation.origin_storage.as_str(), format_revision(&allocation.origin_revision),
                        allocation.origin_run.map(|r| r.to_string()).unwrap_or_else(|| "none".into())).map_err(io_operation)?;
                }
            }
        }
        ServiceOperation::Resources => {
            let state = app.load_instance_services(&name).map_err(safe_error)?;
            for resource in state.resources.iter().filter(|r| r.role == role) {
                summary(out, resource)?;
            }
        }
        ServiceOperation::Show(id) => {
            let resource = app
                .load_service_resource(&name, &id, role)
                .map_err(safe_error)?;
            summary(out, &resource)?;
            writeln!(out, "locator: {}\ndeclaration_revision: {}\nstored_mutation: {}\neffective_mutation: {}",
                resource.declaration.locator.as_str(), format_revision(&resource.declaration_revision),
                mutation(&resource.declaration.user_mutation),
                if role == ServiceRole::Retained { "unavailable_until_reattachment".into() } else { mutation(&resource.declaration.user_mutation) }).map_err(io_operation)?;
        }
        ServiceOperation::Observe(id) => {
            match app
                .observe_service_resource(&name, &id, role)
                .map_err(safe_error)?
            {
                ServiceObservation::Present { kind, kind_matches } => writeln!(
                    out,
                    "Present kind={} kind_matches={kind_matches}",
                    observed_kind(kind)
                )
                .map_err(io_operation)?,
                ServiceObservation::Absent => writeln!(out, "Absent").map_err(io_operation)?,
                ServiceObservation::Unknown(cause) => {
                    writeln!(out, "Unknown cause={}", cause_name(cause)).map_err(io_operation)?;
                    return Err(CliError::operation(
                        ServiceAccessError::Unknown(cause).to_string(),
                    ));
                }
            }
        }
        ServiceOperation::Locate(id, intent) => {
            let path = app
                .locate_service_resource(&name, &id, role, intent)
                .map_err(safe_error)?;
            writeln!(out, "{}", format_path(&path)).map_err(io_operation)?;
        }
    }
    Ok(())
}
fn role_name(role: ServiceRole) -> &'static str {
    match role {
        ServiceRole::Active => "active",
        ServiceRole::Retained => "retained",
    }
}
fn observed_kind(kind: ObservedServiceObjectKind) -> &'static str {
    match kind {
        ObservedServiceObjectKind::File => "file",
        ObservedServiceObjectKind::Directory => "directory",
        ObservedServiceObjectKind::Other => "other",
    }
}
fn cause_name(cause: ServiceObservationCause) -> &'static str {
    match cause {
        ServiceObservationCause::AllocationUnavailable => "allocation_unavailable",
        ServiceObservationCause::PermissionDenied => "permission_denied",
        ServiceObservationCause::UnsafePath => "unsafe_path",
        ServiceObservationCause::IoFailure => "io_failure",
    }
}
fn mutation(route: &ServiceUserMutation) -> String {
    match route {
        ServiceUserMutation::Unavailable => "unavailable".into(),
        ServiceUserMutation::Direct => "direct".into(),
        ServiceUserMutation::Operation { action_id } => format!("operation:{}", action_id.as_str()),
    }
}
fn summary(out: &mut dyn Write, resource: &ServiceResourceAssociation) -> Result<(), CliError> {
    writeln!(
        out,
        "{} storage={} kind={} role={} read={} mutation={}",
        resource.declaration.id.as_str(),
        resource.declaration.storage_id.as_str(),
        match resource.declaration.kind {
            ServiceResourceKind::File => "file",
            ServiceResourceKind::Directory => "directory",
        },
        role_name(resource.role),
        match resource.declaration.read_exposure {
            ServiceReadExposure::Readable => "readable",
            ServiceReadExposure::Hidden => "hidden",
        },
        if resource.role == ServiceRole::Retained {
            "unavailable_until_reattachment".into()
        } else {
            mutation(&resource.declaration.user_mutation)
        }
    )
    .map_err(io_operation)
}
fn safe_error(error: ApplicationError) -> CliError {
    let message = match error {
        ApplicationError::ServiceStorage(error) => error.to_string(),
        ApplicationError::ActionResolution(
            crate::domain::ActionResolutionError::InstanceNotFound,
        ) => "Instance not found".into(),
        ApplicationError::Persistence(
            crate::persistence::PersistenceError::CorruptServiceStorage(_),
        ) => "service_storage.corrupt_storage_state".into(),
        _ => "ServiceStorage query failed; verify the selected store and its schema".into(),
    };
    CliError::operation(message)
}

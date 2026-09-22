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

impl ServiceCommand {
    pub(super) fn presentation_name(&self) -> &'static str {
        match self.operation {
            ServiceOperation::Storages => "service-storage list",
            ServiceOperation::Resources => "resource list",
            ServiceOperation::Show(_) => "resource show",
            ServiceOperation::Observe(_) => "resource observe",
            ServiceOperation::Locate(..) => "resource locate",
        }
    }
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
    format: presentation::Format,
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
            if format == presentation::Format::Json {
                let result = Storages {
                    items: state
                        .storages
                        .iter()
                        .filter(|s| s.role == role)
                        .map(|s| Storage {
                            storage_id: s.declaration.id.as_str().into(),
                            role: role_name(role),
                            revision: (&s.declaration_revision).into(),
                        })
                        .collect(),
                    preserved: if role == ServiceRole::Retained {
                        state
                            .preserved
                            .iter()
                            .map(|p| Preserved {
                                allocation_id: p.allocation.to_string(),
                                origin_storage_id: p.origin_storage.as_str().into(),
                                origin_revision: (&p.origin_revision).into(),
                                origin_run_id: p.origin_run.map(|id| id.to_string()),
                            })
                            .collect()
                    } else {
                        Vec::new()
                    },
                };
                return presentation::render(
                    format,
                    "service-storage list",
                    &result,
                    out,
                    |_, _| unreachable!(),
                );
            }
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
            if format == presentation::Format::Json {
                let result = Resources {
                    items: state
                        .resources
                        .iter()
                        .filter(|r| r.role == role)
                        .map(|r| Resource::new(r, false))
                        .collect(),
                };
                return presentation::render(
                    format,
                    "resource list",
                    &result,
                    out,
                    |_, _| unreachable!(),
                );
            }
            for resource in state.resources.iter().filter(|r| r.role == role) {
                summary(out, resource)?;
            }
        }
        ServiceOperation::Show(id) => {
            let resource = app
                .load_service_resource(&name, &id, role)
                .map_err(safe_error)?;
            if format == presentation::Format::Json {
                return presentation::render(
                    format,
                    "resource show",
                    &Resource::new(&resource, true),
                    out,
                    |_, _| unreachable!(),
                );
            }
            summary(out, &resource)?;
            writeln!(out, "locator: {}\ndeclaration_revision: {}\nstored_mutation: {}\neffective_mutation: {}",
                resource.declaration.locator.as_str(), format_revision(&resource.declaration_revision),
                mutation(&resource.declaration.user_mutation),
                if role == ServiceRole::Retained { "unavailable_until_reattachment".into() } else { mutation(&resource.declaration.user_mutation) }).map_err(io_operation)?;
        }
        ServiceOperation::Observe(id) => {
            let observed = app
                .observe_service_resource(&name, &id, role)
                .map_err(safe_error)?;
            if format == presentation::Format::Json {
                let result = Observation::from(&observed);
                if let ServiceObservation::Unknown(cause) = observed {
                    let mut error =
                        CliError::operation(ServiceAccessError::Unknown(cause).to_string());
                    error.partial = Some(presentation::PartialResult::Observation(result));
                    return Err(error);
                }
                return presentation::render(
                    format,
                    "resource observe",
                    &result,
                    out,
                    |_, _| unreachable!(),
                );
            }
            match observed {
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
            if format == presentation::Format::Json {
                return presentation::render(
                    format,
                    "resource locate",
                    &Location {
                        path: path.as_path().into(),
                    },
                    out,
                    |_, _| unreachable!(),
                );
            }
            writeln!(out, "{}", format_path(&path)).map_err(io_operation)?;
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Location {
    path: presentation::NativePath,
}

#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Storage {
    storage_id: String,
    role: &'static str,
    revision: presentation::Revision,
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Preserved {
    allocation_id: String,
    origin_storage_id: String,
    origin_revision: presentation::Revision,
    origin_run_id: Option<String>,
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Storages {
    items: Vec<Storage>,
    preserved: Vec<Preserved>,
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Resources {
    items: Vec<Resource>,
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Resource {
    resource_id: String,
    storage_id: String,
    kind: &'static str,
    role: &'static str,
    read_exposure: &'static str,
    effective_mutation: Mutation,
    details: Option<ResourceDetails>,
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct ResourceDetails {
    locator: String,
    declaration_revision: presentation::Revision,
    stored_mutation: Mutation,
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Mutation {
    Unavailable,
    UnavailableUntilReattachment,
    Direct,
    Operation { action_id: String },
}
impl From<&ServiceUserMutation> for Mutation {
    fn from(m: &ServiceUserMutation) -> Self {
        match m {
            ServiceUserMutation::Unavailable => Self::Unavailable,
            ServiceUserMutation::Direct => Self::Direct,
            ServiceUserMutation::Operation { action_id } => Self::Operation {
                action_id: action_id.as_str().into(),
            },
        }
    }
}
impl Resource {
    fn new(r: &ServiceResourceAssociation, details: bool) -> Self {
        Self {
            resource_id: r.declaration.id.as_str().into(),
            storage_id: r.declaration.storage_id.as_str().into(),
            kind: match r.declaration.kind {
                ServiceResourceKind::File => "file",
                ServiceResourceKind::Directory => "directory",
            },
            role: role_name(r.role),
            read_exposure: match r.declaration.read_exposure {
                ServiceReadExposure::Readable => "readable",
                ServiceReadExposure::Hidden => "hidden",
            },
            effective_mutation: if r.role == ServiceRole::Retained {
                Mutation::UnavailableUntilReattachment
            } else {
                (&r.declaration.user_mutation).into()
            },
            details: details.then(|| ResourceDetails {
                locator: r.declaration.locator.as_str().into(),
                declaration_revision: (&r.declaration_revision).into(),
                stored_mutation: (&r.declaration.user_mutation).into(),
            }),
        }
    }
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "state", rename_all = "snake_case")]
pub(super) enum Observation {
    Present {
        kind: &'static str,
        kind_matches: bool,
    },
    Absent,
    Unknown {
        cause: &'static str,
    },
}
impl From<&ServiceObservation> for Observation {
    fn from(value: &ServiceObservation) -> Self {
        match value {
            ServiceObservation::Present { kind, kind_matches } => Self::Present {
                kind: observed_kind(*kind),
                kind_matches: *kind_matches,
            },
            ServiceObservation::Absent => Self::Absent,
            ServiceObservation::Unknown(cause) => Self::Unknown {
                cause: cause_name(*cause),
            },
        }
    }
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
    let mut result = CliError::operation("");
    preserve_error_facts(&error, &mut result);
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
    result.message = message;
    result
}

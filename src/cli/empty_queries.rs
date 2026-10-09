//! Missing Stores are empty only for global collection queries, not lookups.
use super::reply::OutputContext;
use super::*;

pub(super) fn collect_missing(
    command: &Command,
    root: &Path,
    format: OutputContext,
) -> Result<bool, CliError> {
    use catalog::{CatalogCommand as C, RunSelector};
    use catalog_presentation::Page;
    if !root.is_absolute() {
        return Err(CliError::operation(
            "PACTRUN_STORAGE_ROOT must be a non-empty absolute path",
        ));
    }
    match std::fs::symlink_metadata(root) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        _ => return Ok(false), // Preserve permission, corruption and existing-path errors.
    }
    let label = presentation::command_name(command);
    fn empty<T: serde::Serialize + Clone + Send + 'static>(
        format: OutputContext,
        label: &str,
        value: &T,
    ) -> Result<(), CliError> {
        presentation::emit_result(format, label, value)
    }
    match command {
        Command::ListInstances => empty(
            format,
            label,
            &definitions::Related::new(definitions::Instances { items: vec![] }, &[], &[]),
        )?,
        Command::Catalog(C::Revisions(_)) => empty(
            format,
            label,
            &Page::<catalog_presentation::RevisionEntry> {
                items: vec![],
                next: None,
            },
        )?,
        Command::Catalog(C::Runs(RunSelector::All, _)) => empty(
            format,
            label,
            &definitions::Related::new(
                Page::<definitions::InspectedRun> {
                    items: vec![],
                    next: None,
                },
                &[],
                &[],
            ),
        )?,
        Command::Catalog(C::History(_, false)) => empty(
            format,
            label,
            &Page::<catalog_presentation::History> {
                items: vec![],
                next: None,
            },
        )?,
        Command::Catalog(C::History(_, true)) => empty(
            format,
            label,
            &Page::<catalog_presentation::Retirement> {
                items: vec![],
                next: None,
            },
        )?,
        Command::Snapshot(snapshots::SnapshotCommand::List { name: None, .. }) => empty(
            format,
            label,
            &definitions::Related::new(
                transport_presentation::Snapshots { items: vec![] },
                &[],
                &[],
            ),
        )?,
        Command::Retirement(retirements::RetirementCommand::DetachedList { .. }) => {
            empty(format, label, &retirements::DetachedList::empty())?
        }
        _ => return Ok(false),
    }
    Ok(true)
}

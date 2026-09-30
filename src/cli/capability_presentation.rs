//! Current author explanations, projected beside (never inside) immutable plans.
use super::*;
use crate::domain::*;
use serde::Serialize;

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Presented<T> {
    #[serde(flatten)]
    pub(super) value: T,
    pub(super) presentation: Vec<Group>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Group {
    revision: presentation::Revision,
    entries: Vec<Entry>,
    #[serde(skip)]
    revision_label: String,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Entry {
    target: catalog_presentation::Target,
    display_name: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    help: Option<String>,
    #[serde(skip)]
    label: String,
}

pub(super) enum Selection {
    All,
    Actions,
    Action(ActionIdentity),
    Capture,
    Restore,
    Cleanup,
    Edge(RevisionContentDigest),
}
impl Selection {
    fn includes(&self, target: &PresentationTargetV1) -> bool {
        use PresentationTargetV1 as T;
        match (self, target) {
            (Self::All, _) => true,
            (Self::Actions, T::Action(_)) => true,
            (
                Self::Action(wanted),
                T::Action(id)
                | T::ActionParameter { action: id, .. }
                | T::ManagedOutput { action: id, .. },
            ) => wanted == id,
            (Self::Capture, T::SnapshotCapture | T::SnapshotCaptureParameter(_)) => true,
            (Self::Restore, T::SnapshotRestore | T::SnapshotRestoreParameter(_)) => true,
            (Self::Cleanup, T::Cleanup) => true,
            (Self::Edge(wanted), T::MigrationEdge(id)) => wanted == id,
            _ => false,
        }
    }
}

pub(super) fn project(row: &RevisionCatalogEntry, selections: &[&Selection]) -> Vec<Group> {
    let mut entries = Vec::<Entry>::new();
    let mut targets = Vec::<PresentationTargetV1>::new();
    for item in &row.metadata.items {
        let RevisionMetadataItem::Presentation(value) = item else {
            continue;
        };
        if !selections.iter().any(|s| s.includes(&value.target)) {
            continue;
        }
        let index = targets
            .iter()
            .position(|target| target == &value.target)
            .unwrap_or_else(|| {
                targets.push(value.target.clone());
                entries.push(Entry {
                    target: (&value.target).into(),
                    display_name: None,
                    summary: None,
                    description: None,
                    help: None,
                    label: catalog::presentation_target(&value.target),
                });
                entries.len() - 1
            });
        let slot = match value.field {
            PresentationField::DisplayName => &mut entries[index].display_name,
            PresentationField::Summary => &mut entries[index].summary,
            PresentationField::Description => &mut entries[index].description,
            PresentationField::Help => &mut entries[index].help,
        };
        *slot = Some(value.value.as_str().to_owned());
    }
    // Keep parameters and outputs next to their capability, rather than the
    // persistence catalog's global target-type ordering.
    let mut ordered: Vec<_> = targets.into_iter().zip(entries).collect();
    ordered.sort_by_key(|(target, _)| capability_order(target));
    let entries: Vec<_> = ordered.into_iter().map(|(_, entry)| entry).collect();
    if entries.is_empty() {
        Vec::new()
    } else {
        vec![Group {
            revision: (&row.identity).into(),
            entries,
            revision_label: format_revision(&row.identity),
        }]
    }
}

fn capability_order(target: &PresentationTargetV1) -> (u8, String, u8, String) {
    use PresentationTargetV1 as T;
    match target {
        T::Revision => (0, String::new(), 0, String::new()),
        T::Input(id) => (1, id.as_str().into(), 0, String::new()),
        T::Action(id) => (2, id.as_str().into(), 0, String::new()),
        T::ActionParameter { action, parameter } => {
            (2, action.as_str().into(), 1, parameter.as_str().into())
        }
        T::ManagedOutput { action, output } => {
            (2, action.as_str().into(), 2, output.as_str().into())
        }
        T::SnapshotCapture => (3, String::new(), 0, String::new()),
        T::SnapshotCaptureParameter(id) => (3, String::new(), 1, id.as_str().into()),
        T::SnapshotRestore => (4, String::new(), 0, String::new()),
        T::SnapshotRestoreParameter(id) => (4, String::new(), 1, id.as_str().into()),
        T::MigrationEdge(id) => (5, id.to_string(), 0, String::new()),
        T::Cleanup => (6, String::new(), 0, String::new()),
    }
}

pub(super) fn load(
    app: &PactrunApplication,
    requested: &[(RevisionIdentity, Selection)],
) -> Result<Vec<Group>, CliError> {
    let ids: Vec<_> = requested.iter().map(|(id, _)| id.clone()).collect();
    let rows = app.inspect_revision_definitions(&ids).map_err(app_error)?;
    Ok(rows
        .iter()
        .flat_map(|row| {
            let selections: Vec<_> = requested
                .iter()
                .filter(|(id, _)| id == &row.identity)
                .map(|(_, s)| s)
                .collect();
            project(row, &selections)
        })
        .collect())
}

pub(super) fn write(out: &mut dyn Write, groups: &[Group], compact: bool) -> Result<(), CliError> {
    for group in groups {
        if compact
            && group
                .entries
                .iter()
                .all(|e| e.display_name.is_none() && e.summary.is_none())
        {
            continue;
        }
        if groups.len() > 1 {
            writeln!(out, "\nRevision: {}", group.revision_label).map_err(io_operation)?;
        }
        for entry in &group.entries {
            if compact && entry.display_name.is_none() && entry.summary.is_none() {
                continue;
            }
            if compact {
                let brief = |value: &Option<String>| {
                    value
                        .as_ref()
                        .map(|s| {
                            let text: String = s.chars().take(160).collect();
                            format!(
                                "{}{}",
                                catalog::safe(&text),
                                if s.chars().count() > 160 { "..." } else { "" }
                            )
                        })
                        .unwrap_or_default()
                };
                writeln!(
                    out,
                    "  {}: {} {}",
                    entry.label,
                    brief(&entry.display_name),
                    brief(&entry.summary)
                )
                .map_err(io_operation)?;
                continue;
            }
            writeln!(out, "  {}", entry.label).map_err(io_operation)?;
            for (field, value) in [
                ("Name", &entry.display_name),
                ("Summary", &entry.summary),
                ("Description", &entry.description),
                ("Help", &entry.help),
            ] {
                if compact && matches!(field, "Description" | "Help") {
                    continue;
                }
                if let Some(value) = value {
                    writeln!(out, "    {field}:").map_err(io_operation)?;
                    for line in value.lines() {
                        writeln!(out, "      {}", catalog::safe(line)).map_err(io_operation)?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0573
    // Verifies: PR-REQ-0363
    #[test]
    fn selection_preserves_capability_and_edge_ownership() {
        let action = ActionIdentity::parse("deploy").unwrap();
        assert!(Selection::Action(action.clone()).includes(&PresentationTargetV1::Action(action)));
        assert!(!Selection::Restore.includes(&PresentationTargetV1::SnapshotCapture));
        assert!(Selection::Restore.includes(&PresentationTargetV1::SnapshotRestore));
        assert!(!Selection::Actions.includes(&PresentationTargetV1::Cleanup));
        let first = RevisionContentDigest::from_bytes([1; 32]);
        let second = RevisionContentDigest::from_bytes([2; 32]);
        assert!(Selection::Edge(first).includes(&PresentationTargetV1::MigrationEdge(first)));
        assert!(!Selection::Edge(first).includes(&PresentationTargetV1::MigrationEdge(second)));
    }
}

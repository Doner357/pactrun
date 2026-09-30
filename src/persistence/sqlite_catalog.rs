//! Bounded catalog queries. Every projection is loaded in the same read transaction.
use super::sqlite_revision_metadata::load_revision_metadata_from;
use super::sqlite_revision_store::load_revision_from;
use super::sqlite_runs::load_managed_run_from;
use super::{PactrunPersistence, PersistenceError};
use crate::domain::*;
use rusqlite::{Connection, OptionalExtension, params};

pub(crate) struct SnapshotCatalog {
    pub(crate) items: Vec<super::SnapshotInspection>,
    pub(crate) revisions: Vec<RevisionCatalogEntry>,
    pub(crate) unavailable_revisions: Vec<RevisionIdentity>,
}

fn sql(error: rusqlite::Error) -> PersistenceError {
    PersistenceError::sqlite("read catalog", error)
}
fn corrupt() -> PersistenceError {
    PersistenceError::CorruptInstance("invalid catalog identity or historical state".into())
}
fn bytes<const N: usize>(value: Vec<u8>) -> Result<[u8; N], PersistenceError> {
    value.try_into().map_err(|_| corrupt())
}
fn bound(limit: usize) -> Result<i64, PersistenceError> {
    if !(1..=500).contains(&limit) {
        return Err(PersistenceError::InvalidMetadata(
            "catalog limit must be 1..500".into(),
        ));
    }
    Ok((limit + 1) as i64)
}
fn page<T, K: Clone>(mut items: Vec<T>, limit: usize, key: impl Fn(&T) -> K) -> CatalogPage<T, K> {
    let more = items.len() > limit;
    items.truncate(limit);
    let next = if more { items.last().map(key) } else { None };
    CatalogPage { items, next }
}
fn revision(
    db: &Connection,
    id: &RevisionIdentity,
) -> Result<RevisionCatalogEntry, PersistenceError> {
    let stored =
        load_revision_from(db, id)?.ok_or_else(|| PersistenceError::MissingRevision(id.clone()))?;
    let metadata = load_revision_metadata_from(db, id, stored.content.core.common())?;
    Ok(RevisionCatalogEntry {
        identity: id.clone(),
        core: stored.content.core,
        metadata,
    })
}

impl PactrunPersistence {
    pub(crate) fn revision_abbreviations(
        &self,
        ids: &[RevisionIdentity],
    ) -> Result<Vec<(String, String)>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        ids.iter().map(|id| {
            let package=id.package_id.to_string();let digest=hex::encode(id.content_digest.as_bytes());
            let mut p=Vec::new();let mut d=Vec::new();
            for (comparison,order) in [("<","DESC"),(">","ASC")] {
                if let Some(v)=tx.query_row(&format!("SELECT lower(hex(package_id)) FROM revisions WHERE package_id {comparison} ?1 ORDER BY package_id {order} LIMIT 1"),[id.package_id.as_bytes().as_slice()],|r|r.get::<_,String>(0)).optional().map_err(sql)?{p.push(v);}
                if let Some(v)=tx.query_row(&format!("SELECT lower(hex(revision_content_digest)) FROM revisions WHERE package_id=?1 AND revision_content_digest {comparison} ?2 ORDER BY revision_content_digest {order} LIMIT 1"),params![id.package_id.as_bytes().as_slice(),id.content_digest.as_bytes().as_slice()],|r|r.get::<_,String>(0)).optional().map_err(sql)?{d.push(v);}
            }
            let short=|s:&str,others:Vec<String>|{let n=others.iter().map(|v|s.bytes().zip(v.bytes()).take_while(|(a,b)|a==b).count()+1).max().unwrap_or(12).max(12).min(s.len());s[..n].to_string()};
            Ok((short(&package,p),format!("sha256:{}",short(&digest,d))))
        }).collect()
    }
    pub(crate) fn identity_prefix(
        &self,
        kind: CatalogIdentityKind,
        prefix: &str,
    ) -> Result<IdentityMatches, PersistenceError> {
        let (table, column) = identity_table(kind);
        let db = self.open_read_connection()?;
        let (low, high) = prefix_bounds(prefix, 32)?;
        let mut stmt = db.prepare(&format!("SELECT lower(hex({column})) FROM {table} WHERE {column}>=?1 AND (?2 IS NULL OR {column}<?2) ORDER BY {column} LIMIT 21")).map_err(sql)?;
        let mut candidates = stmt
            .query_map(params![low, high], |r| r.get::<_, String>(0))
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        let has_more = candidates.len() > 20;
        candidates.truncate(20);
        Ok(IdentityMatches {
            candidates,
            has_more,
        })
    }

    pub(crate) fn revision_prefix(
        &self,
        package: &str,
        digest: &str,
    ) -> Result<IdentityMatches, PersistenceError> {
        let db = self.open_read_connection()?;
        let (pl, ph) = prefix_bounds(package, 32)?;
        let (dl, dh) = prefix_bounds(digest, 64)?;
        let mut stmt = db.prepare("SELECT 'exact:'||lower(hex(package_id))||'/sha256:'||lower(hex(revision_content_digest)) FROM revisions WHERE package_id>=?1 AND (?2 IS NULL OR package_id<?2) AND revision_content_digest>=?3 AND (?4 IS NULL OR revision_content_digest<?4) ORDER BY package_id,revision_content_digest LIMIT 21").map_err(sql)?;
        let mut candidates = stmt
            .query_map(params![pl, ph, dl, dh], |r| r.get::<_, String>(0))
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        let has_more = candidates.len() > 20;
        candidates.truncate(20);
        Ok(IdentityMatches {
            candidates,
            has_more,
        })
    }

    pub(crate) fn identity_abbreviations(
        &self,
        kind: CatalogIdentityKind,
        values: &[String],
    ) -> Result<Vec<String>, PersistenceError> {
        let (table, column) = identity_table(kind);
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let mut before=tx.prepare(&format!("SELECT lower(hex({column})) FROM {table} WHERE {column}<?1 ORDER BY {column} DESC LIMIT 1")).map_err(sql)?;
        let mut after=tx.prepare(&format!("SELECT lower(hex({column})) FROM {table} WHERE {column}>?1 ORDER BY {column} LIMIT 1")).map_err(sql)?;
        values
            .iter()
            .map(|id| {
                let bytes = hex::decode(id).map_err(|_| corrupt())?;
                let previous = before
                    .query_row([&bytes], |r| r.get::<_, String>(0))
                    .optional()
                    .map_err(sql)?;
                let next = after
                    .query_row([&bytes], |r| r.get::<_, String>(0))
                    .optional()
                    .map_err(sql)?;
                let n = previous
                    .iter()
                    .chain(next.iter())
                    .map(|other| {
                        id.bytes()
                            .zip(other.bytes())
                            .take_while(|(a, b)| a == b)
                            .count()
                            + 1
                    })
                    .max()
                    .unwrap_or(12)
                    .max(12)
                    .min(id.len());
                Ok(id[..n].to_owned())
            })
            .collect()
    }
    pub(crate) fn catalog_retirements(
        &self,
        limit: usize,
        after: Option<InstanceId>,
    ) -> Result<CatalogPage<RetirementCatalogEntry, InstanceId>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let mut stmt = tx.prepare("SELECT h.instance_id FROM instance_history_identities h WHERE (?1 IS NULL OR h.instance_id>?1) AND (EXISTS(SELECT 1 FROM instance_deletion_obligations d WHERE d.instance_id=h.instance_id) OR EXISTS(SELECT 1 FROM instance_retirement_receipts t WHERE t.instance_id=h.instance_id)) ORDER BY h.instance_id LIMIT ?2").map_err(sql)?;
        let ids = stmt
            .query_map(
                params![
                    after.as_ref().map(|id| id.as_bytes().as_slice()),
                    bound(limit)?
                ],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        let entries = ids
            .into_iter()
            .map(|id| retirement(&tx, InstanceId::from_bytes(bytes(id)?)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(page(entries, limit, |entry| entry.history.id))
    }

    pub(crate) fn catalog_retirement(
        &self,
        id: InstanceId,
    ) -> Result<RetirementCatalogEntry, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        retirement(&tx, id)
    }
    pub(crate) fn catalog_run_complete(&self, id: RunId) -> Result<RunCatalog, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let run = load_managed_run_from(&tx, id)?;
        let mut result = RunCatalog {
            page: CatalogPage {
                items: vec![run],
                next: None,
            },
            inspections: Vec::new(),
            revisions: Vec::new(),
            unavailable_revisions: Vec::new(),
        };
        complete_runs(&tx, &mut result)?;
        Ok(result)
    }
    pub(crate) fn catalog_snapshots_complete(
        &self,
        name: Option<&InstanceName>,
        id: Option<SnapshotId>,
    ) -> Result<SnapshotCatalog, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let origin = name
            .map(|n| {
                super::sqlite_instances::resolve_instance_name_from(&tx, n)?
                    .ok_or_else(|| PersistenceError::MissingInstance(n.as_str().into()))
            })
            .transpose()?;
        let ids = if let Some(id) = id {
            vec![id]
        } else {
            let mut stmt = tx
                .prepare("SELECT snapshot_id FROM snapshots ORDER BY snapshot_id")
                .map_err(sql)?;
            stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))
                .map_err(sql)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql)?
                .into_iter()
                .map(|v| bytes(v).map(SnapshotId::from_bytes))
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut full = SnapshotCatalog {
            items: Vec::new(),
            revisions: Vec::new(),
            unavailable_revisions: Vec::new(),
        };
        for id in ids {
            let item = super::sqlite_snapshots::inspection_from(&tx, id)?;
            if origin.is_some_and(|i| i != item.origin) {
                continue;
            }
            let revision_id = &item.producer;
            if !full.revisions.iter().any(|r| &r.identity == revision_id)
                && !full.unavailable_revisions.contains(revision_id)
            {
                match revision(&tx, revision_id) {
                    Ok(r) => full.revisions.push(r),
                    Err(PersistenceError::MissingRevision(_)) => {
                        full.unavailable_revisions.push(revision_id.clone())
                    }
                    Err(e) => return Err(e),
                }
            }
            full.items.push(item);
        }
        Ok(full)
    }
    pub(crate) fn catalog_instances_complete(&self) -> Result<InstanceCatalog, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let mut stmt = tx
            .prepare("SELECT instance_id FROM instances ORDER BY instance_name,instance_id")
            .map_err(sql)?;
        let ids = stmt
            .query_map([], |r| r.get::<_, Vec<u8>>(0))
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        let mut items = Vec::new();
        let mut revisions = Vec::<RevisionCatalogEntry>::new();
        for id in ids {
            let view = super::sqlite_instances::load_instance_view_from(
                &tx,
                InstanceId::from_bytes(bytes(id)?),
            )?
            .ok_or_else(corrupt)?;
            if !revisions.iter().any(|r| r.identity == view.active_revision) {
                revisions.push(revision(&tx, &view.active_revision)?);
            }
            items.push(view);
        }
        Ok(InstanceCatalog { items, revisions })
    }
    pub(crate) fn catalog_instance_definition(
        &self,
        name: &InstanceName,
    ) -> Result<Option<(InstanceView, RevisionCatalogEntry)>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let Some(id) = super::sqlite_instances::resolve_instance_name_from(&tx, name)? else {
            return Ok(None);
        };
        let view =
            super::sqlite_instances::load_instance_view_from(&tx, id)?.ok_or_else(corrupt)?;
        let definition = revision(&tx, &view.active_revision)?;
        Ok(Some((view, definition)))
    }

    pub(crate) fn catalog_revision_set(
        &self,
        ids: &[RevisionIdentity],
    ) -> Result<Vec<RevisionCatalogEntry>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let mut rows = Vec::<RevisionCatalogEntry>::new();
        for id in ids {
            if !rows.iter().any(|row| &row.identity == id) {
                rows.push(revision(&tx, id)?);
            }
        }
        Ok(rows)
    }

    pub(crate) fn catalog_alias(
        &self,
        alias: &LocalAlias,
    ) -> Result<Option<RevisionIdentity>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let tuple = tx.query_row("SELECT package_id,revision_content_digest FROM revision_local_aliases WHERE alias_utf8=?1", [alias.as_bytes()], |r| Ok((r.get::<_,Vec<u8>>(0)?, r.get::<_,Vec<u8>>(1)?))).optional().map_err(sql)?;
        let Some((package, digest)) = tuple else {
            return Ok(None);
        };
        let identity = RevisionIdentity::new(
            PackageId::from_bytes(bytes(package)?),
            RevisionContentDigest::from_bytes(bytes(digest)?),
        );
        revision(&tx, &identity)?;
        Ok(Some(identity))
    }
    pub(crate) fn catalog_resolve_revision(
        &self,
        selector: &CatalogRevisionSelector,
    ) -> Result<RevisionCatalogEntry, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let identity = match selector {
            CatalogRevisionSelector::Exact(id) => id.clone(),
            CatalogRevisionSelector::Alias(alias) => {
                let tuple = tx.query_row("SELECT package_id,revision_content_digest FROM revision_local_aliases WHERE alias_utf8=?1", [alias.as_bytes()], |r| Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,Vec<u8>>(1)?))).optional().map_err(sql)?.ok_or_else(|| PersistenceError::InvalidMetadata("local alias does not resolve".into()))?;
                RevisionIdentity::new(
                    PackageId::from_bytes(bytes(tuple.0)?),
                    RevisionContentDigest::from_bytes(bytes(tuple.1)?),
                )
            }
            CatalogRevisionSelector::Label(label) => {
                super::sqlite_revision_metadata::validate_reference_label_lookup_rows(&tx, label)?;
                let mut statement = tx.prepare("SELECT DISTINCT package_id,revision_content_digest FROM revision_reference_label_bindings WHERE label_utf8=?1 ORDER BY package_id,revision_content_digest LIMIT 2").map_err(sql)?;
                let tuples = statement
                    .query_map([label.as_bytes()], |r| {
                        Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
                    })
                    .map_err(sql)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(sql)?;
                if tuples.len() > 1 {
                    return Err(PersistenceError::InvalidMetadata(
                        "resolution.ambiguous_reference".into(),
                    ));
                }
                let (package, digest) = tuples.into_iter().next().ok_or_else(|| {
                    PersistenceError::InvalidMetadata("reference label does not resolve".into())
                })?;
                RevisionIdentity::new(
                    PackageId::from_bytes(bytes(package)?),
                    RevisionContentDigest::from_bytes(bytes(digest)?),
                )
            }
        };
        revision(&tx, &identity)
    }

    pub(crate) fn catalog_revisions(
        &self,
        limit: usize,
        after: Option<&RevisionIdentity>,
    ) -> Result<CatalogPage<RevisionCatalogEntry, RevisionIdentity>, PersistenceError> {
        let take = bound(limit)?;
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let mut stmt = tx.prepare("SELECT package_id,revision_content_digest FROM revisions WHERE ?1 IS NULL OR (package_id,revision_content_digest)>(?1,?2) ORDER BY package_id,revision_content_digest LIMIT ?3").map_err(sql)?;
        let ids = stmt
            .query_map(
                params![
                    after.map(|v| v.package_id.as_bytes().as_slice()),
                    after.map(|v| v.content_digest.as_bytes().as_slice()),
                    take
                ],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?)),
            )
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        let mut entries = Vec::new();
        for (package, digest) in ids {
            let id = RevisionIdentity::new(
                PackageId::from_bytes(bytes(package)?),
                RevisionContentDigest::from_bytes(bytes(digest)?),
            );
            entries.push(revision(&tx, &id)?);
        }
        Ok(page(entries, limit, |v| v.identity.clone()))
    }

    pub(crate) fn catalog_history(
        &self,
        limit: usize,
        after: Option<InstanceId>,
        deletions: bool,
    ) -> Result<CatalogPage<InstanceHistoryEntry, InstanceId>, PersistenceError> {
        let take = bound(limit)?;
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let mut stmt = tx.prepare("SELECT h.instance_id FROM instance_history_identities h WHERE (?1 IS NULL OR h.instance_id>?1) AND (?2=0 OR EXISTS(SELECT 1 FROM instance_deletion_obligations d WHERE d.instance_id=h.instance_id) OR EXISTS(SELECT 1 FROM instance_retirement_receipts t WHERE t.instance_id=h.instance_id)) ORDER BY h.instance_id LIMIT ?3").map_err(sql)?;
        let ids = stmt
            .query_map(
                params![
                    after.as_ref().map(|v| v.as_bytes().as_slice()),
                    deletions,
                    take
                ],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        let entries = ids
            .into_iter()
            .map(|id| history(&tx, InstanceId::from_bytes(bytes(id)?)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(page(entries, limit, |v| v.id))
    }

    pub(crate) fn catalog_instance(
        &self,
        id: InstanceId,
    ) -> Result<InstanceHistoryEntry, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        history(&tx, id)
    }

    pub(crate) fn catalog_runs(
        &self,
        selector: &CatalogRunSelector,
        limit: usize,
        after: Option<RunId>,
    ) -> Result<CatalogPage<ManagedRunView, RunId>, PersistenceError> {
        Ok(self
            .catalog_runs_complete(selector, limit, after, false)?
            .page)
    }
    pub(crate) fn catalog_runs_complete(
        &self,
        selector: &CatalogRunSelector,
        limit: usize,
        after: Option<RunId>,
        details: bool,
    ) -> Result<RunCatalog, PersistenceError> {
        let take = bound(limit)?;
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let instance = match selector {
            CatalogRunSelector::All => None,
            CatalogRunSelector::Identity(id) => {
                history(&tx, *id)?;
                Some(*id)
            }
            CatalogRunSelector::Name(name) => {
                let id: Option<Vec<u8>> = tx
                    .query_row(
                        "SELECT instance_id FROM instances WHERE instance_name=?1",
                        [name.as_str().as_bytes()],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(sql)?;
                Some(InstanceId::from_bytes(bytes(id.ok_or_else(|| {
                    PersistenceError::MissingInstance(name.as_str().into())
                })?)?))
            }
        };
        let mut stmt = tx.prepare("SELECT run_id FROM runs WHERE (?1 IS NULL OR instance_id=?1) AND (?2 IS NULL OR run_id>?2) ORDER BY run_id LIMIT ?3").map_err(sql)?;
        let ids = stmt
            .query_map(
                params![
                    instance.as_ref().map(|v| v.as_bytes().as_slice()),
                    after.as_ref().map(|v| v.as_bytes().as_slice()),
                    take
                ],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        let entries = ids
            .into_iter()
            .map(|id| load_managed_run_from(&tx, RunId::from_bytes(bytes(id)?)))
            .collect::<Result<Vec<_>, _>>()?;
        let page = page(entries, limit, |v| v.id);
        let mut result = RunCatalog {
            page,
            inspections: Vec::new(),
            revisions: Vec::new(),
            unavailable_revisions: Vec::new(),
        };
        if details {
            complete_runs(&tx, &mut result)?;
        }
        Ok(result)
    }
}

fn identity_table(kind: CatalogIdentityKind) -> (&'static str, &'static str) {
    match kind {
        CatalogIdentityKind::Run => ("runs", "run_id"),
        CatalogIdentityKind::Snapshot => ("snapshots", "snapshot_id"),
        CatalogIdentityKind::Instance => ("instance_history_identities", "instance_id"),
        CatalogIdentityKind::Allocation => ("service_storage_allocations", "allocation_id"),
    }
}

fn prefix_bounds(
    prefix: &str,
    width: usize,
) -> Result<(Vec<u8>, Option<Vec<u8>>), PersistenceError> {
    if prefix.is_empty()
        || prefix.len() > width
        || !prefix
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(corrupt());
    }
    let low = hex::decode(format!("{prefix:0<width$}")).map_err(|_| corrupt())?;
    let mut high = low.clone();
    let mut carry = if prefix.len() % 2 == 1 { 16u16 } else { 1 };
    for index in (0..prefix.len().div_ceil(2)).rev() {
        let next = u16::from(high[index]) + carry;
        high[index] = next as u8;
        carry = next >> 8;
        if carry == 0 {
            break;
        }
    }
    Ok((low, if carry == 0 { Some(high) } else { None }))
}

fn retirement(db: &Connection, id: InstanceId) -> Result<RetirementCatalogEntry, PersistenceError> {
    let history = history(db, id)?;
    let live = super::sqlite_instances::load_instance_view_from(db, id)?;
    let obligation = super::sqlite_deletions::obligation_from(db, id)?;
    let mut stmt = db.prepare("SELECT r.run_id FROM runs r JOIN run_deletion_invocations d ON d.run_id=r.run_id WHERE r.instance_id=?1 ORDER BY r.run_id").map_err(sql)?;
    let ids = stmt
        .query_map([id.as_bytes().as_slice()], |r| r.get::<_, Vec<u8>>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let runs = ids
        .into_iter()
        .map(|id| load_managed_run_from(db, RunId::from_bytes(bytes(id)?)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RetirementCatalogEntry {
        history,
        live,
        obligation,
        runs,
    })
}

fn complete_runs(db: &Connection, result: &mut RunCatalog) -> Result<(), PersistenceError> {
    for run in &result.page.items {
        result
            .inspections
            .push(super::sqlite_runs::inspect_run_from(db, run.clone())?);
        let ids = match &run.operation {
            ManagedRunIdentity::Migration(m) => m.path(),
            operation => std::slice::from_ref(operation.revision()),
        };
        for id in ids {
            if result.revisions.iter().any(|r| &r.identity == id)
                || result.unavailable_revisions.contains(id)
            {
                continue;
            }
            match revision(db, id) {
                Ok(row) => result.revisions.push(row),
                Err(PersistenceError::MissingRevision(_)) => {
                    result.unavailable_revisions.push(id.clone())
                }
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}

fn history(db: &Connection, id: InstanceId) -> Result<InstanceHistoryEntry, PersistenceError> {
    let row = db.query_row("SELECT h.instance_name,i.instance_name,d.phase,t.instance_id IS NOT NULL FROM instance_history_identities h LEFT JOIN instances i ON i.instance_id=h.instance_id LEFT JOIN instance_deletion_obligations d ON d.instance_id=h.instance_id LEFT JOIN instance_retirement_receipts t ON t.instance_id=h.instance_id WHERE h.instance_id=?1", [id.as_bytes().as_slice()], |r| Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,Option<Vec<u8>>>(1)?,r.get::<_,Option<i64>>(2)?,r.get::<_,bool>(3)?))).optional().map_err(sql)?.ok_or_else(|| PersistenceError::MissingInstance(id.to_string()))?;
    let name = |v| {
        InstanceName::parse(String::from_utf8(v).map_err(|_| corrupt())?).map_err(|_| corrupt())
    };
    if row.3 && (row.1.is_some() || row.2.is_some()) {
        return Err(corrupt());
    }
    Ok(InstanceHistoryEntry {
        id,
        recorded_name: name(row.0)?,
        current_name: row.1.map(name).transpose()?,
        deletion_phase: row
            .2
            .map(|v| DeletionPhase::from_rank(v).map_err(|_| corrupt()))
            .transpose()?,
        retired: row.3,
    })
}

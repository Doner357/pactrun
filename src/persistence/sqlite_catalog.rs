//! Bounded catalog queries. Every projection is loaded in the same read transaction.
use super::sqlite_revision_metadata::load_revision_metadata_from;
use super::sqlite_revision_store::load_revision_from;
use super::sqlite_runs::load_managed_run_from;
use super::{PactrunPersistence, PersistenceError};
use crate::domain::*;
use rusqlite::{Connection, OptionalExtension, params};

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
        Ok(page(entries, limit, |v| v.id))
    }
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

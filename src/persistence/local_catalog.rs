//! Local management names and installation facts, never portable Pack metadata.
use super::{PactrunPersistence, PersistenceError};
use crate::domain::{
    InstallNames, LocalName, LocalRevisionFacts, LocalRevisionReference, LocalSelector, PackageId,
    RevisionContentDigest, RevisionIdentity,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

pub(super) const SCHEMA: &str = include_str!("local_catalog.sql");

pub(super) fn install_receipt(
    db: &Connection,
    id: &RevisionIdentity,
    newly_installed: bool,
) -> Result<crate::domain::RevisionInstallReceipt, PersistenceError> {
    let local = read_facts(db, id)?;
    let reference = readable_reference(db, id, &local)?;
    Ok(crate::domain::RevisionInstallReceipt {
        identity: id.clone(),
        local,
        reference,
        newly_installed,
    })
}

fn sql(error: rusqlite::Error) -> PersistenceError {
    PersistenceError::sqlite("access local management catalog", error)
}
fn conflict() -> PersistenceError {
    PersistenceError::MetadataConflict(
        "name is already assigned; use rename or unname explicitly".into(),
    )
}
fn name(text: Option<String>) -> Result<Option<LocalName>, PersistenceError> {
    text.map(|s| {
        LocalName::parse(s)
            .map_err(|_| PersistenceError::CorruptMetadata("invalid local name".into()))
    })
    .transpose()
}
fn package(bytes: Vec<u8>) -> Result<PackageId, PersistenceError> {
    bytes
        .try_into()
        .map(PackageId::from_bytes)
        .map_err(|_| PersistenceError::CorruptMetadata("invalid Package identity".into()))
}
fn identity(package_bytes: Vec<u8>, digest: Vec<u8>) -> Result<RevisionIdentity, PersistenceError> {
    let digest = digest
        .try_into()
        .map(RevisionContentDigest::from_bytes)
        .map_err(|_| PersistenceError::CorruptMetadata("invalid Revision identity".into()))?;
    Ok(RevisionIdentity::new(package(package_bytes)?, digest))
}

/// Called inside the Revision installation transaction, before its single commit.
pub(super) fn install_names(
    tx: &Transaction<'_>,
    id: &RevisionIdentity,
    names: &InstallNames,
) -> Result<(), PersistenceError> {
    if let Some(name) = &names.package {
        set_package_name(tx, id.package_id, Some(name), false)?;
    }
    if let Some(name) = &names.revision {
        set_revision_name(tx, id, Some(name), false)?;
    }
    Ok(())
}

pub(super) fn record_install(
    tx: &Transaction<'_>,
    id: &RevisionIdentity,
    millis: i64,
) -> Result<(), PersistenceError> {
    if millis < 0 {
        return Err(PersistenceError::InvalidMetadata(
            "invalid installation time".into(),
        ));
    }
    tx.execute("INSERT INTO revision_installations(package_id,revision_content_digest,installed_at_unix_ms) VALUES (?1,?2,?3)",
        params![id.package_id.as_bytes().as_slice(), id.content_digest.as_bytes().as_slice(), millis]).map_err(sql)?;
    Ok(())
}

fn set_package_name(
    db: &Connection,
    id: PackageId,
    desired: Option<&LocalName>,
    rename: bool,
) -> Result<(), PersistenceError> {
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM packages WHERE package_id=?1)",
            [id.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if !exists {
        return Err(PersistenceError::InvalidMetadata(
            "Package does not exist".into(),
        ));
    }
    let current: Option<String> = db
        .query_row(
            "SELECT name FROM package_local_names WHERE package_id=?1",
            [id.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    if !rename
        && current
            .as_deref()
            .is_some_and(|v| Some(v) != desired.map(LocalName::as_str))
    {
        return Err(conflict());
    }
    if let Some(name) = desired {
        let occupied: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM package_local_names WHERE name=?1 AND package_id<>?2)",
                params![name.as_str(), id.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if occupied {
            return Err(conflict());
        }
        db.execute("INSERT INTO package_local_names(package_id,name) VALUES (?1,?2) ON CONFLICT(package_id) DO UPDATE SET name=excluded.name", params![id.as_bytes().as_slice(), name.as_str()]).map_err(sql)?;
    } else {
        db.execute(
            "DELETE FROM package_local_names WHERE package_id=?1",
            [id.as_bytes().as_slice()],
        )
        .map_err(sql)?;
    }
    Ok(())
}

fn set_revision_name(
    db: &Connection,
    id: &RevisionIdentity,
    desired: Option<&LocalName>,
    rename: bool,
) -> Result<(), PersistenceError> {
    let keys = params![
        id.package_id.as_bytes().as_slice(),
        id.content_digest.as_bytes().as_slice()
    ];
    let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM revisions WHERE package_id=?1 AND revision_content_digest=?2)", keys, |r| r.get(0)).map_err(sql)?;
    if !exists {
        return Err(PersistenceError::MissingRevision(id.clone()));
    }
    let current: Option<String> = db.query_row("SELECT name FROM revision_local_names WHERE package_id=?1 AND revision_content_digest=?2", keys, |r| r.get(0)).optional().map_err(sql)?;
    if !rename
        && current
            .as_deref()
            .is_some_and(|v| Some(v) != desired.map(LocalName::as_str))
    {
        return Err(conflict());
    }
    if let Some(name) = desired {
        let occupied: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM revision_local_names WHERE package_id=?1 AND name=?2 AND revision_content_digest<>?3)", params![id.package_id.as_bytes().as_slice(), name.as_str(), id.content_digest.as_bytes().as_slice()], |r| r.get(0)).map_err(sql)?;
        if occupied {
            return Err(conflict());
        }
        db.execute("INSERT INTO revision_local_names(package_id,revision_content_digest,name) VALUES (?1,?2,?3) ON CONFLICT(package_id,revision_content_digest) DO UPDATE SET name=excluded.name", params![id.package_id.as_bytes().as_slice(), id.content_digest.as_bytes().as_slice(), name.as_str()]).map_err(sql)?;
    } else {
        db.execute(
            "DELETE FROM revision_local_names WHERE package_id=?1 AND revision_content_digest=?2",
            keys,
        )
        .map_err(sql)?;
    }
    Ok(())
}

pub(super) fn resolve_revision(
    db: &Connection,
    reference: &LocalRevisionReference,
) -> Result<RevisionIdentity, PersistenceError> {
    let (p, r) = reference.components();
    // A single snapshot and at most two rows suffice to prove uniqueness.
    let mut stmt = db.prepare("SELECT r.package_id,r.revision_content_digest FROM revisions r LEFT JOIN package_local_names p ON p.package_id=r.package_id LEFT JOIN revision_local_names n ON n.package_id=r.package_id AND n.revision_content_digest=r.revision_content_digest WHERE (p.name=?1 OR (?2 IS NOT NULL AND lower(hex(r.package_id)) LIKE ?2 || '%')) AND (n.name=?3 OR (?4 IS NOT NULL AND lower(hex(r.revision_content_digest)) LIKE ?4 || '%')) LIMIT 2").map_err(sql)?;
    let rows = stmt
        .query_map(
            params![p.as_str(), p.id_prefix(32), r.as_str(), r.id_prefix(64)],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .map_err(sql)?;
    let mut found = None;
    for row in rows {
        let (p, r) = row.map_err(sql)?;
        let id = identity(p, r)?;
        if found.is_some() {
            return Err(PersistenceError::InvalidMetadata(
                "resolution.ambiguous_reference".into(),
            ));
        }
        found = Some(id);
    }
    found.ok_or_else(|| {
        PersistenceError::InvalidMetadata("Revision reference does not resolve".into())
    })
}

impl PactrunPersistence {
    pub(crate) fn resolve_named_revision(
        &self,
        reference: &LocalRevisionReference,
    ) -> Result<RevisionIdentity, PersistenceError> {
        let db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        resolve_revision(&db, reference)
    }

    pub(crate) fn resolve_named_package(
        &self,
        reference: &LocalSelector,
    ) -> Result<PackageId, PersistenceError> {
        let db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let mut statement = db.prepare("SELECT p.package_id FROM packages p LEFT JOIN package_local_names n ON n.package_id=p.package_id WHERE n.name=?1 OR (?2 IS NOT NULL AND lower(hex(p.package_id)) LIKE ?2 || '%') LIMIT 2").map_err(sql)?;
        let rows = statement
            .query_map(params![reference.as_str(), reference.id_prefix(32)], |r| {
                r.get::<_, Vec<u8>>(0)
            })
            .map_err(sql)?;
        let mut found = None;
        for row in rows {
            let id = package(row.map_err(sql)?)?;
            if found.is_some() {
                return Err(PersistenceError::InvalidMetadata(
                    "resolution.ambiguous_reference".into(),
                ));
            }
            found = Some(id);
        }
        found.ok_or_else(|| {
            PersistenceError::InvalidMetadata("Package reference does not resolve".into())
        })
    }

    pub(crate) fn rename_package(
        &self,
        id: PackageId,
        desired: Option<&LocalName>,
    ) -> Result<(), PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql)?;
        self.check_write_admission(&tx)?;
        set_package_name(&tx, id, desired, true)?;
        tx.commit().map_err(sql)
    }

    pub(crate) fn rename_revision(
        &self,
        id: &RevisionIdentity,
        desired: Option<&LocalName>,
    ) -> Result<(), PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql)?;
        self.check_write_admission(&tx)?;
        set_revision_name(&tx, id, desired, true)?;
        tx.commit().map_err(sql)
    }

    pub(crate) fn local_revision_facts(
        &self,
        id: &RevisionIdentity,
    ) -> Result<LocalRevisionFacts, PersistenceError> {
        let db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        read_facts(&db, id)
    }
}

pub(super) fn read_facts(
    db: &Connection,
    id: &RevisionIdentity,
) -> Result<LocalRevisionFacts, PersistenceError> {
    let (p,r,t,present): (Option<String>,Option<String>,Option<i64>,bool) = db.query_row("SELECT p.name,n.name,i.installed_at_unix_ms,i.package_id IS NOT NULL FROM revisions r LEFT JOIN package_local_names p ON p.package_id=r.package_id LEFT JOIN revision_local_names n ON n.package_id=r.package_id AND n.revision_content_digest=r.revision_content_digest LEFT JOIN revision_installations i ON i.package_id=r.package_id AND i.revision_content_digest=r.revision_content_digest WHERE r.package_id=?1 AND r.revision_content_digest=?2", params![id.package_id.as_bytes().as_slice(), id.content_digest.as_bytes().as_slice()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(sql)?.ok_or_else(||PersistenceError::MissingRevision(id.clone()))?;
    if !present {
        return Err(PersistenceError::CorruptMetadata(
            "missing local installation record".into(),
        ));
    }
    Ok(LocalRevisionFacts {
        package_name: name(p)?,
        revision_name: name(r)?,
        installed_at_unix_ms: t,
    })
}

pub(super) fn readable_reference(
    db: &Connection,
    id: &RevisionIdentity,
    facts: &LocalRevisionFacts,
) -> Result<String, PersistenceError> {
    let package = id.package_id.to_string();
    let digest = hex::encode(id.content_digest.as_bytes());
    let text = format!(
        "{}:{}",
        facts
            .package_name
            .as_ref()
            .map_or(&package[..12], LocalName::as_str),
        facts
            .revision_name
            .as_ref()
            .map_or(&digest[..12], LocalName::as_str)
    );
    let reference = LocalRevisionReference::parse(&text)
        .map_err(|e| PersistenceError::CorruptMetadata(e.to_string()))?;
    match resolve_revision(db, &reference) {
        Ok(resolved) if resolved == *id => Ok(text),
        Err(PersistenceError::InvalidMetadata(reason))
            if reason == "resolution.ambiguous_reference" =>
        {
            Ok(format!("{package}:{digest}"))
        }
        Ok(_) => Err(PersistenceError::CorruptMetadata(
            "display reference resolved to another Revision".into(),
        )),
        Err(error) => Err(error),
    }
}

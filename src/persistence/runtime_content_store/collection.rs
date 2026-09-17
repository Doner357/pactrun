//! Verified store-local collection. Never traverses service or workspace roots.
use super::*;
use crate::domain::CollectionReport;
use std::ffi::OsString;

#[derive(Clone, Debug)]
pub(super) struct Candidate {
    pub(super) name: String,
    pub(super) claim: Option<String>,
}

fn blob_name(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn claim_name(name: &str) -> bool {
    name.strip_prefix(".gc-").is_some_and(|s| {
        s.len() == 32
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

impl RuntimeContentStore {
    pub(crate) fn collect_unreferenced(
        &self,
        roots: &BTreeSet<Sha256Digest>,
        execute: bool,
    ) -> Result<CollectionReport, RuntimeContentStoreError> {
        if execute && !self.instance.collection {
            return Err(RuntimeContentStoreError::UnsupportedStorageProfile(
                "collection requires exclusive content coordination".to_owned(),
            ));
        }
        let mut report = CollectionReport::default();
        let names = platform::collection_names(&self.root_file)
            .map_err(|e| RuntimeContentStoreError::io("enumerate runtime content", e))?;
        for raw in names {
            let Some(name) = raw.to_str() else {
                report.unsupported += 1;
                continue;
            };
            if matches!(name, ".publish.lock" | ".collection.lock") {
                continue;
            }
            if blob_name(name) {
                self.collect_one(
                    roots,
                    execute,
                    Candidate {
                        name: name.to_owned(),
                        claim: None,
                    },
                    &mut report,
                );
            } else if claim_name(name) {
                // A Linux interrupted claim is exactly one verified blob in a private
                // directory. Empty or foreign-shaped directories are retained.
                match platform::open_collection_claim(&self.root_file, name)
                    .and_then(|dir| platform::collection_names(&dir))
                {
                    Ok(children) if children.len() == 1 => {
                        let child = &children[0];
                        if let Some(child) = child.to_str().filter(|n| blob_name(n)) {
                            self.collect_one(
                                roots,
                                execute,
                                Candidate {
                                    name: child.to_owned(),
                                    claim: Some(name.to_owned()),
                                },
                                &mut report,
                            );
                        } else {
                            report.unsupported += 1;
                        }
                    }
                    Ok(_) => report.unsupported += 1,
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::Unsupported | io::ErrorKind::InvalidData
                        ) =>
                    {
                        report.unsupported += 1
                    }
                    Err(_) => report.failed += 1,
                }
            } else {
                report.unsupported += 1;
            }
        }
        Ok(report)
    }

    fn collect_one(
        &self,
        roots: &BTreeSet<Sha256Digest>,
        execute: bool,
        candidate: Candidate,
        report: &mut CollectionReport,
    ) {
        let digest = Sha256Digest::parse(format!("sha256:{}", candidate.name))
            .expect("qualified digest spelling");
        if roots.contains(&digest) {
            report.retained += 1;
            return;
        }
        let result = (|| -> Result<(), RuntimeContentStoreError> {
            let mut file = platform::open_collection_blob(&self.root_file, &candidate, execute)
                .map_err(|e| RuntimeContentStoreError::entry("qualify collection candidate", e))?;
            self.verify_open_file(&digest, &mut file)?;
            report.candidates += 1;
            if execute {
                crate::persistence::fault(crate::persistence::FaultPoint::BeforeCollectionRemoval);
                platform::remove_collection_blob(&self.root_file, &candidate, &file).map_err(
                    |e| RuntimeContentStoreError::io("remove qualified runtime blob", e),
                )?;
                report.removed += 1;
                crate::persistence::fault(crate::persistence::FaultPoint::AfterCollectionRemoval);
            }
            Ok(())
        })();
        if result.is_err() {
            report.failed += 1;
        }
    }
}

// Backends enumerate native components; no lossy-name conversion is authoritative.
pub(super) type NativeNames = Vec<OsString>;

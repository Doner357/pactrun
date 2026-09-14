//! Stateless CLI path selectors. Not Revision identity, authority, or a pin.
use super::migration::{migration_successors, validate_exact_path};
use super::*;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    str::FromStr,
};

pub(crate) const MIGRATION_PATH_PAGE_DEFAULT: usize = 20;
pub(crate) const MIGRATION_PATH_PAGE_MAX: usize = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationPathId {
    fingerprint: [u8; 32],
    intermediates: Vec<RevisionContentDigest>,
}

impl fmt::Display for MigrationPathId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mp1-{}", hex::encode(self.fingerprint))?;
        for digest in &self.intermediates {
            write!(f, "-{}", hex::encode(digest.as_bytes()))?;
        }
        Ok(())
    }
}
impl FromStr for MigrationPathId {
    type Err = MigrationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut segments = value
            .strip_prefix("mp1-")
            .ok_or(MigrationError::InvalidPathId)?
            .split('-');
        fn digest(text: &str) -> Result<[u8; 32], MigrationError> {
            if text.len() != 64
                || !text
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                return Err(MigrationError::InvalidPathId);
            }
            let mut bytes = [0; 32];
            hex::decode_to_slice(text, &mut bytes).map_err(|_| MigrationError::InvalidPathId)?;
            Ok(bytes)
        }
        let fingerprint = digest(segments.next().ok_or(MigrationError::InvalidPathId)?)?;
        let intermediates = segments
            .map(|s| digest(s).map(RevisionContentDigest::from_bytes))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            fingerprint,
            intermediates,
        })
    }
}

fn path_fingerprint(path: &[RevisionIdentity]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"pactrun.migration-path.v1\0");
    hash.update(path[0].package_id.as_bytes());
    hash.update((path.len() as u64).to_be_bytes());
    for node in path {
        hash.update(node.content_digest.as_bytes());
    }
    hash.finalize().into()
}

impl MigrationPathId {
    /// A selector is not proof of availability. Resolution checks actual edges.
    pub(crate) fn for_path(path: &[RevisionIdentity]) -> Result<Self, MigrationError> {
        if path.len() < 2 {
            return Err(MigrationError::ZeroEdges);
        }
        if path
            .iter()
            .any(|node| node.package_id != path[0].package_id)
        {
            return Err(MigrationError::CrossLineage);
        }
        if path.iter().collect::<BTreeSet<_>>().len() != path.len() {
            return Err(MigrationError::RepeatedRevision);
        }
        Ok(Self::from_path(path))
    }
    /// Only a validated path can obtain a selector through production listing.
    fn from_path(path: &[RevisionIdentity]) -> Self {
        Self {
            fingerprint: path_fingerprint(path),
            intermediates: path[1..path.len() - 1]
                .iter()
                .map(|n| n.content_digest)
                .collect(),
        }
    }
    pub(crate) fn resolve(
        &self,
        revisions: &[MigrationRevision],
        source: &RevisionIdentity,
        target: &RevisionIdentity,
    ) -> Result<Vec<RevisionIdentity>, MigrationError> {
        let graph = migration_successors(revisions, source, target)?;
        self.resolve_in_graph(&graph, source, target)
    }
    fn resolve_in_graph(
        &self,
        graph: &BTreeMap<RevisionIdentity, Vec<RevisionIdentity>>,
        source: &RevisionIdentity,
        target: &RevisionIdentity,
    ) -> Result<Vec<RevisionIdentity>, MigrationError> {
        let path: Vec<_> = std::iter::once(source.clone())
            .chain(
                self.intermediates
                    .iter()
                    .map(|digest| RevisionIdentity::new(source.package_id, *digest)),
            )
            .chain(std::iter::once(target.clone()))
            .collect();
        if path_fingerprint(&path) != self.fingerprint {
            return Err(MigrationError::PathContextMismatch);
        }
        validate_exact_path(graph, source, target, &path)?;
        Ok(path)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationPathCandidate {
    pub(crate) id: MigrationPathId,
    pub(crate) revisions: Vec<RevisionIdentity>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationPathPage {
    pub(crate) candidates: Vec<MigrationPathCandidate>,
    pub(crate) has_more: bool,
}

pub(crate) fn list_migration_paths(
    revisions: &[MigrationRevision],
    source: &RevisionIdentity,
    target: &RevisionIdentity,
    after: Option<&MigrationPathId>,
    limit: usize,
) -> Result<MigrationPathPage, MigrationError> {
    if !(1..=MIGRATION_PATH_PAGE_MAX).contains(&limit) {
        return Err(MigrationError::InvalidPageSize);
    }
    let graph = migration_successors(revisions, source, target)?;
    let after = after
        .map(|id| id.resolve_in_graph(&graph, source, target))
        .transpose()?;
    let mut candidates = Vec::new();
    let mut path = vec![source.clone()];
    let mut positions = vec![0];
    while let Some(current) = path.last() {
        if current == target {
            if after.as_ref().is_none_or(|cursor| path > *cursor) {
                if candidates.len() == limit {
                    return Ok(MigrationPathPage {
                        candidates,
                        has_more: true,
                    });
                }
                candidates.push(MigrationPathCandidate {
                    id: MigrationPathId::from_path(&path),
                    revisions: path.clone(),
                });
            }
            path.pop();
            positions.pop();
            continue;
        }
        let next = graph.get(current).map(Vec::as_slice).unwrap_or(&[]);
        let position = positions.last_mut().expect("path cursor");
        if *position == next.len() {
            path.pop();
            positions.pop();
            continue;
        }
        let candidate = &next[*position];
        *position += 1;
        if path.contains(candidate) {
            continue;
        }
        let blocked: BTreeSet<_> = path.iter().cloned().collect();
        path.push(candidate.clone());
        let before_cursor = after
            .as_ref()
            .is_some_and(|cursor| path < *cursor && !cursor.starts_with(&path));
        if before_cursor || !can_reach(&graph, candidate, target, blocked) {
            path.pop();
            continue;
        }
        positions.push(0);
    }
    Ok(MigrationPathPage {
        candidates,
        has_more: false,
    })
}

// Prefix-aware reachability prevents factorial exploration of cyclic dead ends.
// Every expanded branch can complete a simple path without revisiting a prefix.
fn can_reach(
    graph: &BTreeMap<RevisionIdentity, Vec<RevisionIdentity>>,
    start: &RevisionIdentity,
    target: &RevisionIdentity,
    mut seen: BTreeSet<RevisionIdentity>,
) -> bool {
    let mut queue = VecDeque::from([start.clone()]);
    seen.insert(start.clone());
    while let Some(current) = queue.pop_front() {
        if &current == target {
            return true;
        }
        for next in graph.get(&current).map(Vec::as_slice).unwrap_or(&[]) {
            if seen.insert(next.clone()) {
                queue.push_back(next.clone());
            }
        }
    }
    false
}

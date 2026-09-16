//! Linux unlink has no expected-inode argument. Before any traversal/removal,
//! atomically claim each object into a fresh private parent and validate what
//! was actually moved. Service directory handles cannot rename that private
//! binding. The journal is physical progress, never authority to run Cleanup.
use super::*;
use rustix::fs::{
    AtFlags, Dir, FileType, Mode, OFlags, RenameFlags, ResolveFlags, StatxFlags, fstat, fsync,
    mkdirat, openat2, renameat, renameat_with, statx, unlinkat,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString, OsString},
    io::{Read, Write},
    os::unix::ffi::OsStringExt,
};

const AREA: &str = "retirement-v1";
const ROOT: &str = "root";
const RECORD_LIMIT: u64 = 4096;
type Key = [u8; 40];

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "retirement identity or physical progress is inconsistent",
    )
}
fn token() -> io::Result<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(hex::encode(bytes))
}
fn valid_token(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_node(s: &str) -> bool {
    s == ROOT || valid_token(s)
}
fn component(bytes: &[u8]) -> io::Result<CString> {
    if bytes.is_empty()
        || bytes.len() > 255
        || bytes == b"."
        || bytes == b".."
        || bytes.contains(&b'/')
    {
        return Err(invalid());
    }
    CString::new(bytes).map_err(|_| invalid())
}
fn decode_key(s: &str) -> io::Result<Key> {
    if s.len() != 80
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    let bytes: Key = hex::decode(s)
        .map_err(|_| invalid())?
        .try_into()
        .map_err(|_| invalid())?;
    if u64::from_le_bytes(bytes[..8].try_into().unwrap()) != 3
        || u64::from_le_bytes(bytes[32..].try_into().unwrap()) >= 1_000_000_000
    {
        return Err(invalid());
    }
    Ok(bytes)
}
fn validate_expected(key: &Key) -> io::Result<()> {
    let mut marked = *key;
    if u64::from_le_bytes(marked[..8].try_into().expect("tag")) == 2 {
        marked[..8].copy_from_slice(&3u64.to_le_bytes());
    }
    decode_key(&hex::encode(marked)).map(|_| ())
}
pub(super) fn identity(file: &File) -> io::Result<Key> {
    let stat = statx(
        file,
        "",
        AtFlags::EMPTY_PATH,
        StatxFlags::BTIME | StatxFlags::INO,
    )?;
    let required = (StatxFlags::BTIME | StatxFlags::INO).bits();
    if stat.stx_mask & required != required {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "retirement requires physical incarnation evidence",
        ));
    }
    let words = [
        3,
        (u64::from(stat.stx_dev_major) << 32) | u64::from(stat.stx_dev_minor),
        stat.stx_ino,
        stat.stx_btime.tv_sec as u64,
        u64::from(stat.stx_btime.tv_nsec),
    ];
    let mut key = [0; 40];
    for (out, word) in key.as_chunks_mut::<8>().0.iter_mut().zip(words) {
        out.copy_from_slice(&word.to_le_bytes());
    }
    Ok(key)
}
fn open_at(parent: &File, name: &CStr, flags: OFlags) -> io::Result<File> {
    let nonblock = if flags.contains(OFlags::PATH) {
        OFlags::empty()
    } else {
        OFlags::NONBLOCK
    };
    let mode = if flags.contains(OFlags::CREATE) {
        Mode::RUSR | Mode::WUSR
    } else {
        Mode::empty()
    };
    Ok(openat2(
        parent,
        name,
        flags | OFlags::NOFOLLOW | OFlags::CLOEXEC | nonblock,
        mode,
        ResolveFlags::BENEATH
            | ResolveFlags::NO_SYMLINKS
            | ResolveFlags::NO_MAGICLINKS
            | ResolveFlags::NO_XDEV,
    )?
    .into())
}
fn dir(parent: &File, name: &str) -> io::Result<File> {
    open_at(
        parent,
        &component(name.as_bytes())?,
        OFlags::RDONLY | OFlags::DIRECTORY,
    )
}
fn ensure_dir(parent: &File, name: &str) -> io::Result<File> {
    match mkdirat(parent, name, Mode::RWXU) {
        Ok(()) => {
            fsync(parent)?;
        }
        Err(rustix::io::Errno::EXIST) => {}
        Err(e) => return Err(e.into()),
    }
    dir(parent, name)
}
fn maybe<T>(value: io::Result<T>) -> io::Result<Option<T>> {
    match value {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn names(file: &File) -> io::Result<Vec<CString>> {
    // A fresh open-file description avoids sharing directory iteration offsets.
    let fresh = open_at(file, c".", OFlags::RDONLY | OFlags::DIRECTORY)?;
    let mut d = Dir::read_from(&fresh)?;
    let mut names = vec![];
    while let Some(entry) = d.read() {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            names.push(name.to_owned());
        }
    }
    names.sort();
    Ok(names)
}
fn qualified(parent: &File, name: &CStr, key: &Key, directory: bool) -> io::Result<Option<File>> {
    let flags = if directory {
        OFlags::RDONLY | OFlags::DIRECTORY
    } else {
        OFlags::PATH
    };
    let Some(file) = maybe(open_at(parent, name, flags))? else {
        return Ok(None);
    };
    let kind = FileType::from_raw_mode(fstat(&file)?.st_mode);
    if &identity(&file)? != key
        || kind
            != if directory {
                FileType::Directory
            } else {
                FileType::RegularFile
            }
    {
        return Err(invalid());
    }
    Ok(Some(file))
}
fn lock(file: &File, shared: bool) -> io::Result<()> {
    let result = if shared {
        file.try_lock_shared()
    } else {
        file.try_lock()
    };
    result.map_err(|e| match e {
        std::fs::TryLockError::WouldBlock => io::Error::new(
            io::ErrorKind::WouldBlock,
            "allocation retirement is owned by another operation",
        ),
        std::fs::TryLockError::Error(e) => e,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Place {
    Live,
    Child,
    Frame { id: String },
    Gone,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    anchor: String,
    parent: Option<String>,
    name: Vec<u8>,
    key: String,
    directory: bool,
    place: Place,
    claim: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Seal {
    version: u8,
    anchor: String,
    key: String,
    moved_to: Option<String>,
}

struct Book {
    store: File,
    slot: File,
    records_dir: File,
    frames: File,
    _lock: File,
    live_name: String,
    anchor: String,
    nodes: BTreeMap<String, Record>,
    // Only bindings created while THIS lock was held may be unlinked. A retry
    // reclaims every surviving object: an earlier handoff may have exposed it.
    held: BTreeSet<String>,
}
impl Book {
    fn load(store: &File, live_name: &str, key: &Key, write: bool) -> io::Result<Option<Self>> {
        let area = if write {
            ensure_dir(store, AREA)?
        } else {
            let Some(d) = maybe(dir(store, AREA))? else {
                return Ok(None);
            };
            d
        };
        let (slot, initialization) = match maybe(dir(&area, live_name))? {
            Some(slot) => (slot, None),
            None if !write => return Ok(None),
            None => {
                let name = format!("init-{}", token()?);
                mkdirat(&area, name.as_str(), Mode::RWXU)?;
                (dir(&area, &name)?, Some(name))
            }
        };
        let lease = open_at(
            &slot,
            &component(b"lock")?,
            if initialization.is_some() {
                OFlags::RDWR | OFlags::CREATE | OFlags::EXCL
            } else if write {
                OFlags::RDWR
            } else {
                OFlags::RDONLY
            },
        )?;
        if FileType::from_raw_mode(fstat(&lease)?.st_mode) != FileType::RegularFile
            || fstat(&lease)?.st_nlink != 1
        {
            return Err(invalid());
        }
        lock(&lease, !write)?;
        let records_dir = if initialization.is_some() {
            ensure_dir(&slot, "records")?
        } else {
            dir(&slot, "records")?
        };
        let frames = if initialization.is_some() {
            ensure_dir(&slot, "frames")?
        } else {
            dir(&slot, "frames")?
        };
        let mut book = Self {
            store: store.try_clone()?,
            slot,
            records_dir,
            frames,
            _lock: lease,
            live_name: live_name.to_owned(),
            anchor: hex::encode(key),
            nodes: BTreeMap::new(),
            held: BTreeSet::new(),
        };
        for name in names(&book.records_dir)? {
            let text = name.to_str().map_err(|_| invalid())?;
            if text.starts_with("tmp-") && valid_token(&text[4..]) {
                continue;
            }
            let id = text
                .strip_suffix(".json")
                .filter(|s| valid_node(s))
                .ok_or_else(invalid)?;
            let file = open_at(&book.records_dir, &name, OFlags::RDONLY)?;
            let stat = fstat(&file)?;
            if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile || stat.st_nlink != 1
            {
                return Err(invalid());
            }
            let mut raw = vec![];
            file.take(RECORD_LIMIT + 1).read_to_end(&mut raw)?;
            if raw.len() as u64 > RECORD_LIMIT {
                return Err(invalid());
            }
            let record: Record = serde_json::from_slice(&raw).map_err(|_| invalid())?;
            if book.nodes.insert(id.to_owned(), record).is_some() {
                return Err(invalid());
            }
        }
        if !book.nodes.contains_key(ROOT) {
            // No object may have moved before its complete root record exists.
            if !book.nodes.is_empty() || !names(&book.frames)?.is_empty() {
                return Err(invalid());
            }
            if initialization.is_none() {
                return Err(invalid());
            }
            book.save(
                ROOT,
                Record {
                    version: 1,
                    anchor: book.anchor.clone(),
                    parent: None,
                    name: live_name.as_bytes().to_vec(),
                    key: book.anchor.clone(),
                    directory: true,
                    place: Place::Live,
                    claim: None,
                },
            )?;
        }
        book.validate()?;
        if let Some(name) = initialization {
            book._lock.sync_all()?;
            fsync(&book.frames)?;
            fsync(&book.slot)?;
            checkpoint("retirement_before_journal_publish");
            renameat_with(
                &area,
                name.as_str(),
                &area,
                live_name,
                RenameFlags::NOREPLACE,
            )?;
            fsync(&area)?;
            checkpoint("retirement_after_journal_publish");
        }
        Ok(Some(book))
    }
    fn validate(&self) -> io::Result<()> {
        let mut used = BTreeSet::new();
        for (id, r) in &self.nodes {
            if r.version != 1 || r.anchor != self.anchor {
                return Err(invalid());
            }
            decode_key(&r.key)?;
            component(&r.name)?;
            if id == ROOT {
                if r.parent.is_some()
                    || r.name != self.live_name.as_bytes()
                    || r.key != self.anchor
                    || !r.directory
                    || matches!(r.place, Place::Child)
                {
                    return Err(invalid());
                }
            } else if !valid_token(id)
                || r.parent.as_ref().is_none_or(|p| !valid_node(p))
                || matches!(r.place, Place::Live)
            {
                return Err(invalid());
            }
            if let Place::Frame { id } = &r.place {
                if !valid_token(id) || !used.insert(id.clone()) {
                    return Err(invalid());
                }
                if let Some(seal) = self.seal(id)?
                    && (seal.key != r.key || seal.moved_to != r.claim)
                {
                    return Err(invalid());
                }
            }
            if let Some(frame) = &r.claim {
                if !valid_token(frame)
                    || !used.insert(frame.clone())
                    || matches!(r.place, Place::Gone)
                {
                    return Err(invalid());
                }
                if self.seal(frame)?.is_some() {
                    return Err(invalid());
                }
            }
            let mut current = id.as_str();
            let mut seen = BTreeSet::new();
            while current != ROOT {
                if !seen.insert(current) {
                    return Err(invalid());
                }
                let parent = self
                    .nodes
                    .get(current)
                    .and_then(|r| r.parent.as_deref())
                    .ok_or_else(invalid)?;
                let r = self.nodes.get(parent).ok_or_else(invalid)?;
                if !r.directory || !matches!(r.place, Place::Frame { .. }) {
                    return Err(invalid());
                }
                current = parent;
            }
        }
        for name in names(&self.frames)? {
            let frame = name
                .to_str()
                .ok()
                .filter(|s| valid_token(s))
                .ok_or_else(invalid)?;
            let parent = self.frame(frame)?;
            let entries = names(&parent)?;
            if entries.iter().any(|n| {
                n.to_bytes() != b"object"
                    && n.to_bytes() != b"seal.json"
                    && !n.to_bytes().starts_with(b"tmp-")
            }) {
                return Err(invalid());
            }
            if !used.contains(frame)
                && entries.iter().any(|n| n.to_bytes() == b"object")
                && self.seal(frame)?.is_none()
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn seal(&self, frame: &str) -> io::Result<Option<Seal>> {
        let parent = self.frame(frame)?;
        let Some(file) = maybe(open_at(&parent, c"seal.json", OFlags::RDONLY))? else {
            return Ok(None);
        };
        if FileType::from_raw_mode(fstat(&file)?.st_mode) != FileType::RegularFile
            || fstat(&file)?.st_nlink != 1
        {
            return Err(invalid());
        }
        let mut raw = vec![];
        file.take(RECORD_LIMIT + 1).read_to_end(&mut raw)?;
        if raw.len() as u64 > RECORD_LIMIT {
            return Err(invalid());
        }
        let seal: Seal = serde_json::from_slice(&raw).map_err(|_| invalid())?;
        if seal.version != 1
            || seal.anchor != self.anchor
            || seal
                .moved_to
                .as_ref()
                .is_some_and(|id| !valid_token(id) || id == frame)
        {
            return Err(invalid());
        }
        decode_key(&seal.key)?;
        Ok(Some(seal))
    }
    fn seal_frame(&self, frame: &str, key: &str, moved_to: Option<String>) -> io::Result<()> {
        if let Some(seal) = self.seal(frame)? {
            if seal.key != key || seal.moved_to != moved_to {
                return Err(invalid());
            }
            return Ok(());
        }
        let parent = self.frame(frame)?;
        let temp = format!("tmp-{}", token()?);
        let mut file = open_at(
            &parent,
            &component(temp.as_bytes())?,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
        )?;
        file.write_all(
            &serde_json::to_vec(&Seal {
                version: 1,
                anchor: self.anchor.clone(),
                key: key.to_owned(),
                moved_to,
            })
            .map_err(io::Error::other)?,
        )?;
        file.sync_all()?;
        renameat_with(
            &parent,
            temp.as_str(),
            &parent,
            "seal.json",
            RenameFlags::NOREPLACE,
        )?;
        fsync(&parent)?;
        Ok(())
    }
    fn save(&mut self, id: &str, r: Record) -> io::Result<()> {
        let raw = serde_json::to_vec(&r).map_err(io::Error::other)?;
        if raw.len() as u64 > RECORD_LIMIT {
            return Err(invalid());
        }
        let tmp = format!("tmp-{}", token()?);
        let mut file = open_at(
            &self.records_dir,
            &component(tmp.as_bytes())?,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
        )?;
        file.write_all(&raw)?;
        file.sync_all()?;
        renameat(
            &self.records_dir,
            tmp.as_str(),
            &self.records_dir,
            format!("{id}.json").as_str(),
        )?;
        fsync(&self.records_dir)?;
        self.nodes.insert(id.to_owned(), r);
        Ok(())
    }
    fn frame(&self, id: &str) -> io::Result<File> {
        if !valid_token(id) {
            return Err(invalid());
        }
        dir(&self.frames, id)
    }
    fn object(&self, id: &str) -> io::Result<Option<File>> {
        let r = self.nodes.get(id).ok_or_else(invalid)?;
        let key = decode_key(&r.key)?;
        if let Some(frame) = &r.claim {
            if self.seal(frame)?.is_some() {
                return Err(invalid());
            }
            if let Some(file) = qualified(
                &self.frame(frame)?,
                &component(b"object")?,
                &key,
                r.directory,
            )? {
                return Ok(Some(file));
            }
        }
        if let Place::Frame { id } = &r.place
            && let Some(seal) = self.seal(id)?
        {
            if seal.key != r.key {
                return Err(invalid());
            }
            if seal.moved_to.is_none() {
                return Ok(None);
            }
            if r.claim.as_ref() != seal.moved_to.as_ref() {
                return Err(invalid());
            }
            // The durable move receipt identifies the destination, even when
            // its data was subsequently taken away through an explicit handoff.
            // Never return to a replacement at the former source position.
            return Ok(None);
        }
        let Some((parent, name)) = self.source(r)? else {
            return Ok(None);
        };
        qualified(&parent, &name, &key, r.directory)
    }
    fn source(&self, r: &Record) -> io::Result<Option<(File, CString)>> {
        match &r.place {
            Place::Live => Ok(maybe(dir(&self.store, "service-storage"))?.map(|p| {
                (
                    p,
                    component(self.live_name.as_bytes()).expect("fixed allocation name"),
                )
            })),
            Place::Child => Ok(self
                .object(r.parent.as_deref().ok_or_else(invalid)?)?
                .map(|p| (p, component(&r.name).expect("validated native name")))),
            Place::Frame { id } => {
                if let Some(seal) = self.seal(id)? {
                    if seal.key != r.key || seal.moved_to.is_some() {
                        return Err(invalid());
                    }
                    return Ok(None);
                }
                Ok(Some((self.frame(id)?, component(b"object")?)))
            }
            Place::Gone => Ok(None),
        }
    }
    fn recover(&mut self, id: &str) -> io::Result<()> {
        let mut r = self.nodes.get(id).ok_or_else(invalid)?.clone();
        let Some(frame) = r.claim.clone() else {
            return Ok(());
        };
        let dest = self.frame(&frame)?;
        let moved = if let Place::Frame { id: old } = &r.place {
            self.seal(old)?
                .is_some_and(|seal| seal.key == r.key && seal.moved_to.as_ref() == Some(&frame))
        } else {
            false
        };
        if qualified(
            &dest,
            &component(b"object")?,
            &decode_key(&r.key)?,
            r.directory,
        )?
        .is_some()
            || moved
        {
            fsync(&dest)?;
            if let Place::Frame { id: old } = &r.place {
                self.seal_frame(old, &r.key, Some(frame.clone()))?;
            }
            r.place = Place::Frame { id: frame };
        } else if self.object(id)?.is_none() {
            // Retain an absent private position, not a fallback to a later
            // object at the original public pathname. Children finish first.
            if let Place::Frame { id: old } = &r.place {
                self.seal_frame(old, &r.key, None)?;
            }
            r.place = Place::Frame { id: frame };
        }
        r.claim = None;
        self.save(id, r)
    }
    fn take(&mut self, id: &str, observer: &mut impl FnMut(&CStr, bool)) -> io::Result<()> {
        if self.held.contains(id) {
            return Ok(());
        }
        self.recover(id)?;
        let mut r = self.nodes.get(id).ok_or_else(invalid)?.clone();
        let Some((parent, name)) = self.source(&r)? else {
            self.held.insert(id.to_owned());
            return Ok(());
        };
        if qualified(&parent, &name, &decode_key(&r.key)?, r.directory)?.is_none() {
            self.held.insert(id.to_owned());
            return Ok(());
        }
        let frame = token()?;
        mkdirat(&self.frames, frame.as_str(), Mode::RWXU)?;
        fsync(&self.frames)?;
        let dest = self.frame(&frame)?;
        r.claim = Some(frame.clone());
        self.save(id, r.clone())?;
        checkpoint_named("retirement_before_claim", &r.name);
        observer(&component(&r.name)?, r.directory);
        renameat_with(&parent, &name, &dest, "object", RenameFlags::NOREPLACE)?;
        fsync(&parent)?;
        fsync(&dest)?;
        checkpoint_named("retirement_after_claim", &r.name);
        if !matches!(
            qualified(
                &dest,
                &component(b"object")?,
                &decode_key(&r.key)?,
                r.directory
            ),
            Ok(Some(_))
        ) {
            // A concurrent substitution may have been captured. Never erase
            // it. Restore its directory entry if that can be done no-clobber;
            // otherwise retain the conflict for explicit repair/handoff.
            if renameat_with(&dest, "object", &parent, &name, RenameFlags::NOREPLACE).is_ok() {
                fsync(&dest)?;
                fsync(&parent)?;
            }
            return Err(invalid());
        }
        if let Place::Frame { id: old } = &r.place {
            self.seal_frame(old, &r.key, Some(frame.clone()))?;
            checkpoint_named("retirement_source_sealed", &r.name);
        }
        r.place = Place::Frame { id: frame };
        r.claim = None;
        self.save(id, r)?;
        self.held.insert(id.to_owned());
        checkpoint_named("retirement_claim_committed", &self.nodes[id].name);
        Ok(())
    }
    fn finish_node(&mut self, id: &str) -> io::Result<()> {
        let mut r = self.nodes.get(id).ok_or_else(invalid)?.clone();
        if let Place::Frame { id: frame } = &r.place {
            self.seal_frame(frame, &r.key, None)?;
        }
        r.place = Place::Gone;
        r.claim = None;
        self.save(id, r)?;
        if id != ROOT {
            unlinkat(
                &self.records_dir,
                format!("{id}.json").as_str(),
                AtFlags::empty(),
            )?;
            fsync(&self.records_dir)?;
            self.nodes.remove(id);
        }
        self.held.remove(id);
        Ok(())
    }
    fn run(&mut self, mut observer: impl FnMut(&CStr, bool)) -> io::Result<()> {
        let mut stack = vec![ROOT.to_owned()];
        while let Some(id) = stack.last().cloned() {
            self.take(&id, &mut observer)?;
            if let Some(child) = self
                .nodes
                .iter()
                .find(|(_, r)| r.parent.as_deref() == Some(id.as_str()))
                .map(|(id, _)| id.clone())
            {
                stack.push(child);
                continue;
            }
            let r = self.nodes.get(&id).ok_or_else(invalid)?.clone();
            let Some(file) = self.object(&id)? else {
                self.finish_node(&id)?;
                stack.pop();
                continue;
            };
            if !self.held.contains(&id) {
                return Err(invalid());
            }
            let Place::Frame { id: frame } = &r.place else {
                return Err(invalid());
            };
            if r.directory
                && let Some(name) = names(&file)?.into_iter().next()
            {
                let child = open_at(&file, &name, OFlags::PATH)?;
                let kind = FileType::from_raw_mode(fstat(&child)?.st_mode);
                if !matches!(kind, FileType::Directory | FileType::RegularFile) {
                    return Err(invalid());
                }
                let child_id = token()?;
                self.save(
                    &child_id,
                    Record {
                        version: 1,
                        anchor: self.anchor.clone(),
                        parent: Some(id),
                        name: name.to_bytes().to_vec(),
                        key: hex::encode(identity(&child)?),
                        directory: kind == FileType::Directory,
                        place: Place::Child,
                        claim: None,
                    },
                )?;
                stack.push(child_id);
                continue;
            }
            // Only this invocation's fresh, unexposed parent is used here.
            // Neither original pathnames nor service-held parent handles can
            // substitute its object binding. A shared inspector cannot publish
            // new frame paths while this allocation's exclusive lease is held.
            let parent = self.frame(frame)?;
            if qualified(
                &parent,
                &component(b"object")?,
                &decode_key(&r.key)?,
                r.directory,
            )?
            .is_none()
            {
                return Err(invalid());
            }
            checkpoint_named("retirement_before_unlink", &r.name);
            unlinkat(
                &parent,
                "object",
                if r.directory {
                    AtFlags::REMOVEDIR
                } else {
                    AtFlags::empty()
                },
            )?;
            fsync(&parent)?;
            checkpoint_named("retirement_after_unlink", &r.name);
            self.finish_node(&id)?;
            stack.pop();
        }
        fsync(&self.slot)?;
        Ok(())
    }
}

#[cfg(not(test))]
fn checkpoint(_: &str) {}
#[cfg(test)]
fn checkpoint(point: &str) {
    checkpoint_named(point, b"root");
}
#[cfg(not(test))]
fn checkpoint_named(_: &str, _: &[u8]) {}
#[cfg(test)]
fn checkpoint_named(point: &str, name: &[u8]) {
    if std::env::var_os("PACTRUN_RETIREMENT_FAULT").is_some_and(|p| p == point)
        && std::env::var_os("PACTRUN_RETIREMENT_FAULT_NODE")
            .is_none_or(|n| n.as_encoded_bytes() == name)
    {
        std::process::exit(87);
    }
}

fn existing_slot(store: &File, name: &str) -> io::Result<bool> {
    let Some(area) = maybe(dir(store, AREA))? else {
        return Ok(false);
    };
    Ok(maybe(dir(&area, name))?.is_some())
}
pub(super) fn open(
    root: &Path,
    allocation: ServiceAllocationId,
    expected: Option<&Key>,
) -> io::Result<Option<AllocationRoot>> {
    if let Some(key) = expected {
        validate_expected(key)?;
    }
    let store = service_storage::open_root(root)?;
    let name = format!("alloc-{allocation}");
    let identity = if existing_slot(&store, &name)? {
        let expected = expected.ok_or_else(invalid)?;
        decode_key(&hex::encode(expected))?;
        let book = Book::load(&store, &name, expected, false)?.ok_or_else(invalid)?;
        if matches!(book.nodes[ROOT].place, Place::Gone) {
            return Ok(None);
        }
        // Inspection does not recover or infer another location from absence.
        for id in book.nodes.keys() {
            book.object(id)?;
        }
        *expected
    } else {
        let Some(parent) = maybe(dir(&store, "service-storage"))? else {
            return Ok(None);
        };
        let Some(file) = maybe(dir(&parent, &name))? else {
            return Ok(None);
        };
        let key = identity(&file)?;
        if expected.is_some_and(|expected| !super::same_incarnation(expected, &key)) {
            return Err(invalid());
        }
        key
    };
    Ok(Some(AllocationRoot {
        parent: store,
        name,
        identity,
    }))
}
pub(super) fn remove(store: &File, name: &str, expected: &Key) -> io::Result<()> {
    remove_observed(store, name, expected, |_, _| {})
}
pub(super) fn remove_observed(
    store: &File,
    name: &str,
    expected: &Key,
    observer: impl FnMut(&CStr, bool),
) -> io::Result<()> {
    let mut book = Book::load(store, name, expected, true)?.ok_or_else(invalid)?;
    book.run(observer)
}
pub(super) fn handoff(
    root: &Path,
    allocation: ServiceAllocationId,
    expected: Option<&Key>,
) -> io::Result<Vec<HandoffLocation>> {
    if let Some(key) = expected {
        validate_expected(key)?;
    }
    let store = service_storage::open_root(root)?;
    let name = format!("alloc-{allocation}");
    if !existing_slot(&store, &name)? {
        let Some(parent) = maybe(dir(&store, "service-storage"))? else {
            return Ok(vec![]);
        };
        let Some(file) = maybe(dir(&parent, &name))? else {
            return Ok(vec![]);
        };
        let actual = identity(&file)?;
        if expected.is_some_and(|key| !super::same_incarnation(key, &actual)) {
            return Err(invalid());
        }
        return Ok(vec![HandoffLocation {
            path: root.join("service-storage").join(name),
            original_relative_path: PathBuf::from("."),
        }]);
    }
    decode_key(&hex::encode(expected.ok_or_else(invalid)?))?;
    let book =
        Book::load(&store, &name, expected.ok_or_else(invalid)?, false)?.ok_or_else(invalid)?;
    let base = root.join(AREA).join(&name).join("frames");
    let mut result = vec![];
    for id in book.nodes.keys() {
        if book.object(id)?.is_none() {
            continue;
        }
        let mut relative = vec![];
        let mut current = id.as_str();
        while current != ROOT {
            let record = &book.nodes[current];
            relative.push(OsString::from_vec(record.name.clone()));
            current = record.parent.as_deref().ok_or_else(invalid)?;
        }
        let mut original_relative_path = PathBuf::new();
        for part in relative.into_iter().rev() {
            original_relative_path.push(part);
        }
        if original_relative_path.as_os_str().is_empty() {
            original_relative_path.push(".");
        }
        let path = location_path(&book, root, &base, id)?;
        result.push(HandoffLocation {
            path,
            original_relative_path,
        });
    }
    Ok(result)
}
fn location_path(book: &Book, root: &Path, base: &Path, id: &str) -> io::Result<PathBuf> {
    let r = book.nodes.get(id).ok_or_else(invalid)?;
    if let Some(frame) = &r.claim
        && qualified(
            &book.frame(frame)?,
            c"object",
            &decode_key(&r.key)?,
            r.directory,
        )?
        .is_some()
    {
        return Ok(base.join(frame).join("object"));
    }
    match &r.place {
        Place::Live => Ok(root.join("service-storage").join(&book.live_name)),
        Place::Frame { id } => Ok(base.join(id).join("object")),
        Place::Child => {
            Ok(
                location_path(book, root, base, r.parent.as_deref().ok_or_else(invalid)?)?
                    .join(OsString::from_vec(r.name.clone())),
            )
        }
        Place::Gone => Err(invalid()),
    }
}

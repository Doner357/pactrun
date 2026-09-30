//! Bounded Stored-ZIP profile. Preflight precedes the general ZIP reader.
#![allow(dead_code)] // Application integration follows the adapter/store tests.

use crate::{
    domain::{
        CapabilityRefusal, Sha256Digest, SnapshotCapability, SnapshotIntegrityVersion,
        SnapshotValidationError,
    },
    managed_data::{StagedFile, StagingSession},
    snapshot_integrity::{self, SnapshotCodecError, VerifiedSnapshotManifest},
    strict_json::{self, RawJsonValue},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BundleError {
    UnsupportedVersion(crate::domain::VersionDomain, crate::domain::FormatVersion),
    Profile(&'static str),
    Capability(CapabilityRefusal),
    Integrity(SnapshotCodecError),
    Io(io::ErrorKind),
}
impl From<CapabilityRefusal> for BundleError {
    fn from(e: CapabilityRefusal) -> Self {
        Self::Capability(e)
    }
}
impl From<SnapshotCodecError> for BundleError {
    fn from(e: SnapshotCodecError) -> Self {
        Self::Integrity(e)
    }
}
impl From<io::Error> for BundleError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::InvalidData {
            Self::Profile("invalid ZIP member data or CRC")
        } else {
            Self::Io(e.kind())
        }
    }
}
impl From<zip::result::ZipError> for BundleError {
    fn from(e: zip::result::ZipError) -> Self {
        match e {
            zip::result::ZipError::Io(e) => e.into(),
            _ => Self::Profile("invalid or unsupported ZIP archive"),
        }
    }
}
impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(domain, version) => {
                f.write_str(&domain.unsupported_message(*version))
            }
            Self::Profile(reason) => write!(f, "Snapshot bundle: {reason}"),
            Self::Capability(e) => e.fmt(f),
            Self::Integrity(e) => e.fmt(f),
            Self::Io(_) => f.write_str("Snapshot bundle I/O failed"),
        }
    }
}
impl std::error::Error for BundleError {}

#[derive(Clone, Debug)]
struct Entry {
    name: String,
    size: u64,
    crc: u32,
    flags: u16,
    header: u64,
    data: u64,
}
fn invalid<T>(reason: &'static str) -> Result<T, BundleError> {
    Err(BundleError::Profile(reason))
}
fn add(a: u64, b: u64) -> Result<u64, BundleError> {
    a.checked_add(b)
        .ok_or(BundleError::Profile("ZIP offset overflow"))
}
fn u16le(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes(b[p..p + 2].try_into().unwrap())
}
fn u32le(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
fn u64le(b: &[u8], p: usize) -> u64 {
    u64::from_le_bytes(b[p..p + 8].try_into().unwrap())
}
fn at<const N: usize>(file: &mut File, offset: u64, end: u64) -> Result<[u8; N], BundleError> {
    if add(offset, N as u64)? > end {
        return invalid("ZIP record outside its section");
    };
    file.seek(SeekFrom::Start(offset))?;
    let mut b = [0; N];
    file.read_exact(&mut b)?;
    Ok(b)
}
fn variable(file: &mut File, offset: u64, len: usize, end: u64) -> Result<Vec<u8>, BundleError> {
    if add(offset, len as u64)? > end {
        return invalid("ZIP variable record outside its section");
    };
    file.seek(SeekFrom::Start(offset))?;
    let mut b = vec![0; len];
    file.read_exact(&mut b)?;
    Ok(b)
}
fn zip64(
    extra: &[u8],
    size: u32,
    compressed: u32,
    offset: u32,
    disk: u16,
) -> Result<(u64, u64, u64, u32), BundleError> {
    let mut cursor = 0;
    let mut record = None;
    while cursor < extra.len() {
        if extra.len() - cursor < 4 {
            return invalid("truncated ZIP extra field");
        };
        let tag = u16le(extra, cursor);
        let length = u16le(extra, cursor + 2) as usize;
        cursor += 4;
        if length > extra.len() - cursor {
            return invalid("truncated ZIP extra payload");
        };
        if tag == 1 {
            if record.is_some() {
                return invalid("duplicate ZIP64 field");
            };
            record = Some(&extra[cursor..cursor + length]);
        }
        if matches!(tag, 0x0017 | 0x9901) {
            return invalid("encrypted ZIP extra field");
        };
        cursor += length;
    }
    let needs =
        size == u32::MAX || compressed == u32::MAX || offset == u32::MAX || disk == u16::MAX;
    if !needs {
        return Ok((size.into(), compressed.into(), offset.into(), disk.into()));
    };
    let b = record.ok_or(BundleError::Profile("missing ZIP64 extra field"))?;
    let mut p = 0;
    let mut wide = || -> Result<u64, BundleError> {
        if b.len() - p < 8 {
            return invalid("truncated ZIP64 value");
        };
        let v = u64le(b, p);
        p += 8;
        Ok(v)
    };
    let size = if size == u32::MAX {
        wide()?
    } else {
        size.into()
    };
    let compressed = if compressed == u32::MAX {
        wide()?
    } else {
        compressed.into()
    };
    let offset = if offset == u32::MAX {
        wide()?
    } else {
        offset.into()
    };
    let disk = if disk == u16::MAX {
        if b.len() - p < 4 {
            return invalid("truncated ZIP64 disk");
        };
        u32le(b, p)
    } else {
        disk.into()
    };
    Ok((size, compressed, offset, disk))
}
fn member_name(bytes: &[u8]) -> Result<String, BundleError> {
    if bytes == b"bundle.json" || bytes == b"manifest.json" {
        return Ok(String::from_utf8(bytes.to_vec()).unwrap());
    };
    let Some(digest) = bytes.strip_prefix(b"blobs/sha256/") else {
        return invalid("unrecognized ZIP member");
    };
    if digest.len() != 64
        || !digest
            .iter()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return invalid("invalid blob member path");
    };
    Ok(String::from_utf8(bytes.to_vec()).unwrap())
}

fn preflight(file: &mut File) -> Result<(Vec<Entry>, u64), BundleError> {
    let length = file.metadata()?.len();
    SnapshotCapability::BundleBytes.check(length)?;
    if length < 22 {
        return invalid("missing ZIP end record");
    };
    let tail_size = length.min(65_557) as usize;
    let start = length - tail_size as u64;
    let tail = variable(file, start, tail_size, length)?;
    let position = (0..=tail.len() - 22)
        .rev()
        .find(|&p| {
            u32le(&tail, p) == 0x06054b50 && p + 22 + u16le(&tail, p + 20) as usize == tail.len()
        })
        .ok_or(BundleError::Profile("invalid ZIP end record"))?;
    let end_record = start + position as u64;
    let e = &tail[position..];
    if u16le(e, 4) != 0 || u16le(e, 6) != 0 || u16le(e, 8) != u16le(e, 10) {
        return invalid("multi-volume ZIP is not supported");
    };
    let (mut count, mut directory_size, mut directory) = (
        u16le(e, 10) as u64,
        u32le(e, 12) as u64,
        u32le(e, 16) as u64,
    );
    let mut directory_end = end_record;
    let locator = if end_record >= 20 {
        Some(at::<20>(file, end_record - 20, length)?)
    } else {
        None
    };
    if let Some(locator) = locator.filter(|b| u32le(b, 0) == 0x07064b50) {
        if u32le(&locator, 4) != 0 || u32le(&locator, 16) != 1 {
            return invalid("multi-volume ZIP64");
        };
        let offset = u64le(&locator, 8);
        let z = at::<56>(file, offset, end_record - 20)?;
        let record_size = u64le(&z, 4);
        if u32le(&z, 0) != 0x06064b50
            || record_size < 44
            || add(offset, add(12, record_size)?)? != end_record - 20
            || u32le(&z, 16) != 0
            || u32le(&z, 20) != 0
            || u64le(&z, 24) != u64le(&z, 32)
        {
            return invalid("invalid ZIP64 end record");
        };
        let (c, s, o) = (u64le(&z, 32), u64le(&z, 40), u64le(&z, 48));
        if (count != u16::MAX as u64 && count != c)
            || (directory_size != u32::MAX as u64 && directory_size != s)
            || (directory != u32::MAX as u64 && directory != o)
        {
            return invalid("inconsistent ZIP64 directory");
        };
        (count, directory_size, directory) = (c, s, o);
        directory_end = offset;
    } else if count == u16::MAX as u64
        || directory_size == u32::MAX as u64
        || directory == u32::MAX as u64
    {
        return invalid("missing ZIP64 locator");
    };
    SnapshotCapability::BundleEntries.check(count)?;
    if count < 2 || add(directory, directory_size)? != directory_end || directory_end > length {
        return invalid("invalid central directory extent");
    };
    let mut metadata = length - directory;
    SnapshotCapability::BundleMetadata.check(metadata)?;
    let mut entries = Vec::with_capacity(count as usize);
    let mut names = BTreeSet::new();
    let mut cursor = directory;
    let mut blobs = 0_u64;
    for _ in 0..count {
        let h = at::<46>(file, cursor, directory_end)?;
        if u32le(&h, 0) != 0x02014b50 || u16le(&h, 10) != 0 || u16le(&h, 8) & !0x0808 != 0 {
            return invalid("unsupported central ZIP record");
        };
        let (name_len, extra_len, comment_len) = (
            u16le(&h, 28) as usize,
            u16le(&h, 30) as usize,
            u16le(&h, 32) as usize,
        );
        if name_len > 77 {
            return invalid("invalid ZIP member name");
        };
        let name_bytes = variable(file, add(cursor, 46)?, name_len, directory_end)?;
        let name = member_name(&name_bytes)?;
        if !names.insert(name.clone()) {
            return invalid("duplicate ZIP member");
        };
        let extra = variable(
            file,
            add(cursor, 46 + name_len as u64)?,
            extra_len,
            directory_end,
        )?;
        let (size, compressed, header, disk) = zip64(
            &extra,
            u32le(&h, 24),
            u32le(&h, 20),
            u32le(&h, 42),
            u16le(&h, 34),
        )?;
        if disk != 0 || size != compressed {
            return invalid("non-Stored or split ZIP member");
        };
        let attr = u32le(&h, 38);
        let kind = (attr >> 16) & 0xf000;
        if attr & 0x18 != 0 || (kind != 0 && kind != 0x8000) {
            return invalid("non-regular ZIP member");
        };
        let cap = match name.as_str() {
            "bundle.json" => SnapshotCapability::BundleEnvelope,
            "manifest.json" => SnapshotCapability::RawManifest,
            _ => SnapshotCapability::StoredBlob,
        };
        cap.check(size)?;
        if name.starts_with("blobs/") {
            blobs = add(blobs, size)?;
            SnapshotCapability::StoredClosure.check(blobs)?;
        }
        entries.push(Entry {
            name,
            size,
            crc: u32le(&h, 16),
            flags: u16le(&h, 8),
            header,
            data: 0,
        });
        cursor = add(
            cursor,
            46 + name_len as u64 + extra_len as u64 + comment_len as u64,
        )?;
        if cursor > directory_end {
            return invalid("central ZIP member exceeds directory");
        };
    }
    if cursor != directory_end || !names.contains("bundle.json") || !names.contains("manifest.json")
    {
        return invalid("incomplete central directory");
    };
    let mut order: Vec<_> = (0..entries.len()).collect();
    order.sort_by_key(|&i| entries[i].header);
    let mut next = 0;
    for i in order {
        let entry = &mut entries[i];
        if entry.header != next {
            return invalid("ZIP prefix, overlap, hidden entry, or gap");
        };
        let h = at::<30>(file, entry.header, directory)?;
        if u32le(&h, 0) != 0x04034b50 || u16le(&h, 6) != entry.flags || u16le(&h, 8) != 0 {
            return invalid("local/central ZIP disagreement");
        };
        let (n, x) = (u16le(&h, 26) as usize, u16le(&h, 28) as usize);
        if n != entry.name.len() {
            return invalid("local ZIP name mismatch");
        };
        if variable(file, add(entry.header, 30)?, n, directory)? != entry.name.as_bytes() {
            return invalid("local ZIP name mismatch");
        };
        let extra = variable(file, add(entry.header, 30 + n as u64)?, x, directory)?;
        let (s, c, _, _) = zip64(&extra, u32le(&h, 22), u32le(&h, 18), 0, 0)?;
        let descriptor = entry.flags & 8 != 0;
        if (!descriptor && (s != entry.size || c != entry.size || u32le(&h, 14) != entry.crc))
            || (descriptor
                && ((s != 0 && s != entry.size)
                    || (c != 0 && c != entry.size)
                    || (u32le(&h, 14) != 0 && u32le(&h, 14) != entry.crc)))
        {
            return invalid("local ZIP sizes or CRC disagree");
        };
        metadata = add(metadata, 30 + n as u64 + x as u64)?;
        SnapshotCapability::BundleMetadata.check(metadata)?;
        entry.data = add(entry.header, 30 + n as u64 + x as u64)?;
        next = add(entry.data, entry.size)?;
        if next > directory {
            return invalid("ZIP payload extends into metadata");
        };
        if descriptor {
            let wide = u32le(&h, 18) == u32::MAX
                || u32le(&h, 22) == u32::MAX
                || entry.size > u32::MAX as u64;
            let total = descriptor_length(file, next, directory, wide, entry.crc, entry.size)?;
            metadata = add(metadata, total)?;
            SnapshotCapability::BundleMetadata.check(metadata)?;
            next = add(next, total)?;
        }
    }
    if next != directory {
        return invalid("unaccounted local ZIP data");
    };
    Ok((entries, directory))
}

fn descriptor_length(
    file: &mut File,
    offset: u64,
    end: u64,
    wide: bool,
    crc: u32,
    length: u64,
) -> Result<u64, BundleError> {
    let first = at::<4>(file, offset, end)?;
    for signed in [true, false] {
        if signed && u32le(&first, 0) != 0x08074b50 {
            continue;
        };
        let prefix = if signed { 4 } else { 0 };
        let body_size = if wide { 20 } else { 12 };
        let total = prefix + body_size;
        if add(offset, total as u64)? > end {
            continue;
        };
        let b = variable(file, offset + prefix as u64, body_size, end)?;
        let (a, c) = if wide {
            (u64le(&b, 4), u64le(&b, 12))
        } else {
            (u32le(&b, 4) as u64, u32le(&b, 8) as u64)
        };
        if u32le(&b, 0) == crc && a == length && c == length {
            return Ok(total as u64);
        }
    }
    invalid("invalid ZIP data descriptor")
}

/// The library sees only trusted bounded directory metadata, never a second
/// interpretation of attacker-controlled EOCD/extra-field entry counts.
struct ValidatedZipView {
    file: File,
    metadata: Vec<u8>,
    local_end: u64,
    position: u64,
}
impl ValidatedZipView {
    fn new(file: File, entries: &[Entry], directory: u64) -> Result<Self, BundleError> {
        let mut metadata = Vec::with_capacity(entries.len() * 160 + 100);
        for e in entries {
            let mut h = [0u8; 46];
            h[0..4].copy_from_slice(&0x02014b50u32.to_le_bytes());
            h[4..6].copy_from_slice(&((3u16 << 8) | 45).to_le_bytes());
            h[6..8].copy_from_slice(&45u16.to_le_bytes());
            h[8..10].copy_from_slice(&e.flags.to_le_bytes());
            h[16..20].copy_from_slice(&e.crc.to_le_bytes());
            h[20..28].fill(255);
            h[28..30].copy_from_slice(&(e.name.len() as u16).to_le_bytes());
            h[30..32].copy_from_slice(&28u16.to_le_bytes());
            h[38..42].copy_from_slice(&(0o100600u32 << 16).to_le_bytes());
            h[42..46].fill(255);
            metadata.extend(h);
            metadata.extend(e.name.as_bytes());
            metadata.extend(1u16.to_le_bytes());
            metadata.extend(24u16.to_le_bytes());
            metadata.extend(e.size.to_le_bytes());
            metadata.extend(e.size.to_le_bytes());
            metadata.extend(e.header.to_le_bytes());
        }
        let cd_size = metadata.len() as u64;
        let zip64 = add(directory, cd_size)?;
        metadata.extend(0x06064b50u32.to_le_bytes());
        metadata.extend(44u64.to_le_bytes());
        metadata.extend(45u16.to_le_bytes());
        metadata.extend(45u16.to_le_bytes());
        metadata.extend(0u64.to_le_bytes());
        metadata.extend((entries.len() as u64).to_le_bytes());
        metadata.extend((entries.len() as u64).to_le_bytes());
        metadata.extend(cd_size.to_le_bytes());
        metadata.extend(directory.to_le_bytes());
        metadata.extend(0x07064b50u32.to_le_bytes());
        metadata.extend(0u32.to_le_bytes());
        metadata.extend(zip64.to_le_bytes());
        metadata.extend(1u32.to_le_bytes());
        metadata.extend(0x06054b50u32.to_le_bytes());
        metadata.extend(0u32.to_le_bytes());
        metadata.extend([255; 12]);
        metadata.extend(0u16.to_le_bytes());
        Ok(Self {
            file,
            metadata,
            local_end: directory,
            position: 0,
        })
    }
}
impl Read for ValidatedZipView {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        };
        if self.position < self.local_end {
            self.file.seek(SeekFrom::Start(self.position))?;
            let n = buffer
                .len()
                .min((self.local_end - self.position).min(usize::MAX as u64) as usize);
            let read = self.file.read(&mut buffer[..n])?;
            self.position += read as u64;
            Ok(read)
        } else {
            let offset = self.position - self.local_end;
            if offset >= self.metadata.len() as u64 {
                return Ok(0);
            };
            let offset = offset as usize;
            let n = buffer.len().min(self.metadata.len() - offset);
            buffer[..n].copy_from_slice(&self.metadata[offset..offset + n]);
            self.position += n as u64;
            Ok(n)
        }
    }
}
impl Seek for ValidatedZipView {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
            SeekFrom::End(n) => {
                i128::from(self.local_end) + self.metadata.len() as i128 + i128::from(n)
            }
        };
        self.position = u64::try_from(next)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid archive seek"))?;
        Ok(self.position)
    }
}

pub(crate) struct ValidatedSnapshotBundle {
    archive: ZipArchive<ValidatedZipView>,
    manifest: VerifiedSnapshotManifest,
    blobs: BTreeMap<Sha256Digest, usize>,
    lengths: BTreeMap<Sha256Digest, u64>,
    _stage: StagedFile,
}
impl std::fmt::Debug for ValidatedSnapshotBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ValidatedSnapshotBundle([private])")
    }
}

pub(crate) fn stage_bundle(
    session: &StagingSession,
    source: &mut dyn Read,
) -> Result<StagedFile, BundleError> {
    let mut stage = session
        .create_snapshot_stage()
        .map_err(|_| BundleError::Io(io::ErrorKind::Other))?;
    copy_bounded(source, stage.writer(), SnapshotCapability::BundleBytes)?;
    stage.finish_operation_file()?;
    Ok(stage)
}
fn copy_bounded(
    source: &mut dyn Read,
    target: &mut dyn Write,
    cap: SnapshotCapability,
) -> Result<u64, BundleError> {
    let mut total = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = match source.read(&mut buffer) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            other => other?,
        };
        if n == 0 {
            break;
        };
        total = add(total, n as u64)?;
        cap.check(total)?;
        target.write_all(&buffer[..n])?;
    }
    Ok(total)
}

impl ValidatedSnapshotBundle {
    pub(crate) fn read(stage: StagedFile) -> Result<Self, BundleError> {
        let mut file = stage
            .try_clone_reader()
            .map_err(|_| BundleError::Io(io::ErrorKind::Other))?;
        let (entries, directory) = preflight(&mut file)?;
        file.seek(SeekFrom::Start(0))?;
        let view = ValidatedZipView::new(file, &entries, directory)?;
        let mut archive = ZipArchive::with_config(
            zip::read::Config {
                archive_offset: zip::read::ArchiveOffset::Known(0),
            },
            view,
        )?;
        if archive.len() != entries.len() {
            return invalid("ZIP adapter changed the member set");
        };
        for (i, expected) in entries.iter().enumerate() {
            let actual = archive.by_index(i)?;
            if actual.name_raw() != expected.name.as_bytes()
                || actual.header_start() != expected.header
                || actual.data_start() != Some(expected.data)
                || actual.size() != expected.size
                || actual.crc32() != expected.crc
                || actual.compression() != CompressionMethod::Stored
                || actual.encrypted()
                || actual.is_symlink()
            {
                return invalid("ZIP adapter disagrees with bounded preflight");
            };
        }
        let envelope_index = entries
            .iter()
            .position(|e| e.name == "bundle.json")
            .unwrap();
        let manifest_index = entries
            .iter()
            .position(|e| e.name == "manifest.json")
            .unwrap();
        let mut envelope = Vec::new();
        archive
            .by_index(envelope_index)?
            .read_to_end(&mut envelope)?;
        let (version, digest) = decode_envelope(&envelope)?;
        let mut raw = Vec::new();
        archive.by_index(manifest_index)?.read_to_end(&mut raw)?;
        let manifest = snapshot_integrity::decode_snapshot_manifest(version, &raw, None)?;
        if manifest.integrity_digest().as_str() != digest {
            return invalid("envelope integrity disagrees with manifest");
        };
        let references = references(manifest.manifest());
        let mut blobs = BTreeMap::new();
        let mut lengths = BTreeMap::new();
        for (i, entry) in entries.iter().enumerate() {
            if let Some(hex) = entry.name.strip_prefix("blobs/sha256/") {
                let key = Sha256Digest::parse(format!("sha256:{hex}"))
                    .map_err(|_| BundleError::Profile("invalid blob identity"))?;
                blobs.insert(key.clone(), i);
                lengths.insert(key, entry.size);
            }
        }
        if references != blobs.keys().cloned().collect() {
            return invalid("bundle payload closure is not exact");
        };
        for (digest, index) in &blobs {
            let mut reader = archive.by_index(*index)?;
            let mut hash = Sha256::new();
            let mut buffer = [0; 64 * 1024];
            let mut length = 0;
            loop {
                let n = reader.read(&mut buffer)?;
                if n == 0 {
                    break;
                };
                length = add(length, n as u64)?;
                SnapshotCapability::StoredBlob.check(length)?;
                hash.update(&buffer[..n]);
            }
            if length != lengths[digest] || <[u8; 32]>::from(hash.finalize()) != digest.to_bytes() {
                return Err(BundleError::Integrity(
                    SnapshotValidationError::ContentDigestMismatch.into(),
                ));
            }
        }
        Ok(Self {
            archive,
            manifest,
            blobs,
            lengths,
            _stage: stage,
        })
    }
    pub(crate) fn manifest(&self) -> &VerifiedSnapshotManifest {
        &self.manifest
    }
    pub(crate) fn lengths(&self) -> &BTreeMap<Sha256Digest, u64> {
        &self.lengths
    }
    pub(crate) fn open_blob(
        &mut self,
        digest: &Sha256Digest,
    ) -> Result<impl Read + '_, BundleError> {
        let index = *self
            .blobs
            .get(digest)
            .ok_or(BundleError::Profile("missing verified blob"))?;
        Ok(self.archive.by_index(index)?)
    }
}
pub(crate) fn references(manifest: &crate::domain::SnapshotManifest) -> BTreeSet<Sha256Digest> {
    manifest
        .managed_bindings()
        .iter()
        .filter_map(|b| match &b.state {
            crate::domain::SnapshotBindingState::Absent => None,
            crate::domain::SnapshotBindingState::Bound(d) => Some(d.clone()),
        })
        .chain(
            manifest
                .service_content()
                .iter()
                .map(|c| c.blob_digest.clone()),
        )
        .collect()
}
fn decode_envelope(raw: &[u8]) -> Result<(SnapshotIntegrityVersion, String), BundleError> {
    SnapshotCapability::BundleEnvelope.check(raw.len() as u64)?;
    snapshot_integrity::check_json_depth(raw)?;
    let value = strict_json::parse_json(
        raw,
        SnapshotCapability::BundleEnvelope
            .maximum()
            .expect("metadata bound") as usize,
    )
    .map_err(|_| BundleError::Profile("invalid closed bundle envelope"))?;
    let RawJsonValue::Object(fields) = value else {
        return invalid("envelope must be an object");
    };
    let mut fields: BTreeMap<_, _> = fields.into_iter().collect();
    if fields.remove("kind") != Some(RawJsonValue::String("pactrun_snapshot_bundle".to_owned())) {
        return invalid("invalid bundle kind");
    };
    let Some(RawJsonValue::String(bundle)) = fields.remove("bundle_version") else {
        return invalid("bundle version must be a string");
    };
    let required = bundle
        .parse::<crate::domain::FormatVersion>()
        .map_err(|_| BundleError::Profile("invalid bundle version identifier"))?;
    if !crate::domain::VersionDomain::SnapshotBundle.supports(required) {
        return Err(BundleError::UnsupportedVersion(
            crate::domain::VersionDomain::SnapshotBundle,
            required,
        ));
    }
    let Some(RawJsonValue::String(integrity)) = fields.remove("integrity_format") else {
        return invalid("integrity version must be a string");
    };
    let required = integrity
        .parse::<crate::domain::FormatVersion>()
        .map_err(|_| BundleError::Profile("invalid integrity version identifier"))?;
    if !crate::domain::VersionDomain::Snapshot.supports(required) {
        return Err(BundleError::UnsupportedVersion(
            crate::domain::VersionDomain::Snapshot,
            required,
        ));
    }
    let version = SnapshotIntegrityVersion::from_text(&integrity)
        .map_err(|_| BundleError::Profile("unsupported Snapshot integrity version"))?;
    if fields.len() != 1 {
        return invalid("invalid envelope fields");
    }
    let Some(RawJsonValue::String(digest)) = fields.remove("integrity_digest") else {
        return invalid("invalid envelope digest");
    };
    crate::domain::SnapshotIntegrityDigest::parse(&digest)
        .map_err(|_| BundleError::Profile("invalid envelope digest"))?;
    Ok((version, digest))
}

use crate::zip_output::DropSafeWriter;
pub(crate) struct SnapshotBundleWriter<'a> {
    writer: ZipWriter<DropSafeWriter<&'a mut File>>,
    failed: std::rc::Rc<std::cell::Cell<Option<io::ErrorKind>>>,
}
impl Write for SnapshotBundleWriter<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if let Some(kind) = self.failed.get() {
            return Err(kind.into());
        };
        self.writer.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        if let Some(kind) = self.failed.get() {
            return Err(kind.into());
        };
        self.writer.flush()
    }
}
impl SnapshotBundleWriter<'_> {
    pub(crate) fn start_blob(
        &mut self,
        digest: &Sha256Digest,
        length: u64,
    ) -> Result<(), BundleError> {
        if let Some(kind) = self.failed.get() {
            return Err(BundleError::Io(kind));
        };
        start_blob(&mut self.writer, digest, length)
    }
    pub(crate) fn finish(self) -> Result<(), BundleError> {
        let failed = self.failed.clone();
        let result = self.writer.finish();
        if let Some(kind) = failed.get() {
            return Err(BundleError::Io(kind));
        };
        result.map(|_| ()).map_err(BundleError::from)
    }
}
pub(crate) fn start_bundle<'a>(
    stage: &'a mut StagedFile,
    manifest: &VerifiedSnapshotManifest,
) -> Result<SnapshotBundleWriter<'a>, BundleError> {
    let failed = std::rc::Rc::new(std::cell::Cell::new(None));
    let position = stage.writer().stream_position()?;
    let length = stage.writer().metadata()?.len();
    let mut writer = ZipWriter::new(DropSafeWriter {
        inner: stage.writer(),
        failed: failed.clone(),
        position,
        length,
    });
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o600);
    writer.start_file("bundle.json", options)?;
    let envelope = serde_json::json!({"kind":"pactrun_snapshot_bundle","bundle_version":"1.0-alpha.1","integrity_format":manifest.manifest().version().as_str(),"integrity_digest":manifest.integrity_digest().as_str()});
    writer.write_all(
        &serde_json::to_vec(&envelope)
            .map_err(|_| BundleError::Profile("envelope serialization failed"))?,
    )?;
    writer.start_file("manifest.json", options)?;
    writer.write_all(manifest.canonical_bytes())?;
    Ok(SnapshotBundleWriter { writer, failed })
}
pub(crate) fn start_blob<W: Write + Seek>(
    writer: &mut ZipWriter<W>,
    digest: &Sha256Digest,
    length: u64,
) -> Result<(), BundleError> {
    let name = format!(
        "blobs/sha256/{}",
        digest.as_str().strip_prefix("sha256:").unwrap()
    );
    writer.start_file(
        name,
        SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o600)
            .large_file(length > u32::MAX as u64),
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "snapshot_bundle_tests.rs"]
mod tests;

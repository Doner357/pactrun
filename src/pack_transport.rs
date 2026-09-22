//! Pack container acquisition and the independently versioned distribution codec.
use crate::{
    domain::*,
    hook::ActionCancellation,
    managed_data::{StagedFile, StagedRuntimeSource, StagingSession},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

#[cfg(test)]
#[path = "pack_transport_tests.rs"]
mod tests;

pub(crate) const DESCRIPTOR: &str = "pactrun-distribution.json";
const CORE: &str = "revision-core.json";
const RUNTIME: &str = "runtime-content.json";
const JSON_LIMIT: u64 = 16 * 1024 * 1024;
pub(crate) type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub(crate) fn check_cancel(c: &ActionCancellation) -> io::Result<()> {
    // Interrupted is retried by write_all and must not represent cancellation.
    if c.is_requested() {
        Err(io::Error::other("Pack operation cancelled"))
    } else {
        Ok(())
    }
}
pub(crate) struct CancelWriter<'a, W> {
    pub(crate) inner: W,
    pub(crate) cancellation: &'a ActionCancellation,
}
impl<W: Write> Write for CancelWriter<'_, W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        check_cancel(self.cancellation)?;
        self.inner.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        check_cancel(self.cancellation)?;
        self.inner.flush()
    }
}
pub(crate) struct CancelReader<'a, R> {
    pub(crate) inner: R,
    pub(crate) cancellation: &'a ActionCancellation,
}
impl<R: Read> Read for CancelReader<'_, R> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        check_cancel(self.cancellation)?;
        self.inner.read(b)
    }
}
use crate::revision_installation::PreparedRevision;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    kind: String,
    format_version: u8,
    package_id: String,
    revision_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    portable_metadata: Option<crate::pack_metadata::Metadata>,
}
enum InputFiles {
    Directory {
        root: crate::authoring::SecureSourceRoot,
    },
    Archive(BTreeMap<String, StagedRuntimeSource>),
}
fn open_path(path: &Path) -> io::Result<File> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::other("invalid Pack path"))?;
    crate::opened_files::open_file(&crate::opened_files::open_root(parent)?, &[name])
}
impl InputFiles {
    fn open(path: &Path, session: &StagingSession, c: &ActionCancellation) -> Result<Self> {
        check_cancel(c).map_err(err)?;
        let info = std::fs::symlink_metadata(path).map_err(err)?;
        if info.is_dir() {
            return Ok(Self::Directory {
                root: crate::authoring::SecureSourceRoot::open(path).map_err(err)?,
            });
        }
        let mut reader = CancelReader {
            inner: open_path(path).map_err(err)?,
            cancellation: c,
        };
        let archive = session.stage_runtime_source(&mut reader).map_err(err)?;
        let mut file = archive.bytes.try_clone_reader().map_err(err)?;
        let (entries, _) = crate::pack_zip::preflight(&mut file).map_err(err)?;
        let mut names = BTreeMap::new();
        let mut folded = BTreeMap::new();
        for entry in &entries {
            let name = entry.name.trim_end_matches('/');
            if folded.insert(name.to_ascii_lowercase(), name).is_some() {
                return Err("conflicting Pack member names".into());
            }
            names.insert(name.to_owned(), entry.name.ends_with('/'));
        }
        let mut previous: Option<&str> = None;
        for name in folded.values() {
            if let Some(previous) = previous {
                for (left, right) in previous.split('/').zip(name.split('/')) {
                    if !left.eq_ignore_ascii_case(right) {
                        break;
                    }
                    if left != right {
                        return Err("conflicting Pack member ancestor names".into());
                    }
                }
            }
            previous = Some(name);
        }
        for name in names.keys() {
            let mut ancestor = name.as_str();
            while let Some((parent, _)) = ancestor.rsplit_once('/') {
                if names.get(parent) == Some(&false) {
                    return Err("Pack file/directory conflict".into());
                }
                ancestor = parent;
            }
        }
        if names.get("pactrun.yaml") == Some(&true) || names.get(DESCRIPTOR) == Some(&true) {
            return Err("Pack entry marker must be a regular file".into());
        }
        let source = names.contains_key("pactrun.yaml");
        let distribution = names.contains_key(DESCRIPTOR);
        if source == distribution {
            return Err("Pack requires exactly one source or distribution marker".into());
        }
        for entry in &entries {
            let manifest = (source && entry.name == "pactrun.yaml")
                || (distribution && matches!(entry.name.as_str(), DESCRIPTOR | CORE | RUNTIME));
            if manifest && entry.size > JSON_LIMIT {
                return Err("Pack manifest exceeds codec limit before decompression".into());
            }
        }
        let mut files = BTreeMap::new();
        for entry in entries {
            check_cancel(c).map_err(err)?;
            let data = expand_entry(&mut file, &entry, session, c)?;
            if !entry.name.ends_with('/') {
                files.insert(entry.name, data);
            }
        }
        Ok(Self::Archive(files))
    }
    fn read(&self, name: &str) -> Result<Option<File>> {
        crate::authoring::SourceRelativePathV1::parse(name).map_err(err)?;
        match self {
            Self::Archive(files) => files
                .get(name)
                .map(|s| s.bytes.try_clone_reader().map_err(err))
                .transpose(),
            Self::Directory { root, .. } => match if name == "pactrun.yaml" {
                root.open_manifest()
            } else {
                root.open_runtime_source(
                    &crate::authoring::SourceRelativePathV1::parse(name).map_err(err)?,
                )
            } {
                Ok(file) => Ok(Some(file)),
                Err(crate::authoring::SourceAcquisitionError::Io { source, .. })
                    if source.kind() == io::ErrorKind::NotFound =>
                {
                    Ok(None)
                }
                Err(e) => Err(err(e)),
            },
        }
    }
    fn required(&self, name: &str) -> Result<File> {
        self.read(name)?
            .ok_or_else(|| format!("missing Pack member {name}"))
    }
    fn names(&self) -> Result<BTreeSet<String>> {
        match self {
            Self::Archive(files) => Ok(files.keys().cloned().collect()),
            Self::Directory { root, .. } => root.file_names(65_539).map_err(err),
        }
    }
}

fn read_json(mut file: File) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(JSON_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    crate::strict_json::parse_json(&bytes, JSON_LIMIT as usize).map_err(err)?;
    Ok(bytes)
}

pub(crate) fn acquire(
    path: &Path,
    session: &StagingSession,
    c: &ActionCancellation,
) -> Result<PreparedRevision> {
    let input = InputFiles::open(path, session, c)?;
    let source = input.read("pactrun.yaml")?;
    let distribution = input.read(DESCRIPTOR)?;
    match (source, distribution) {
        (Some(_), Some(_)) => Err("Pack has both source and distribution markers".into()),
        (None, None) => {
            Err("Pack root has neither pactrun.yaml nor pactrun-distribution.json".into())
        }
        (Some(mut source), None) => {
            let mut bytes = Vec::new();
            Read::by_ref(&mut source)
                .take(JSON_LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(err)?;
            if bytes.len() as u64 > JSON_LIMIT {
                return Err("Pack source manifest exceeds codec limit".into());
            }
            let candidate = crate::authoring::parse_pack_source_yaml(&bytes).map_err(err)?;
            let mut blobs = BTreeMap::new();
            let mut files = Vec::new();
            let mut acquired =
                BTreeMap::<crate::authoring::SourceRelativePathV1, Sha256Digest>::new();
            for record in candidate.runtime_sources {
                let digest = if let Some(digest) = acquired.get(&record.source) {
                    digest.clone()
                } else {
                    let mut reader = CancelReader {
                        inner: input.required(record.source.as_str())?,
                        cancellation: c,
                    };
                    let blob = session.stage_runtime_source(&mut reader).map_err(err)?;
                    let digest = blob.blob_digest.clone();
                    acquired.insert(record.source, digest.clone());
                    blobs.entry(digest.clone()).or_insert(blob);
                    digest
                };
                files.push(RuntimeFileV1 {
                    id: record.id,
                    path: record.path,
                    kind: RuntimeFileKindV1::RegularFile,
                    blob_digest: digest,
                    executable: record.executable,
                });
            }
            let runtime =
                project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files })
                    .map_err(err)?;
            let definition = crate::authoring::NormalizedPackDefinition {
                package_id: candidate.package_id,
                revision: candidate.revision,
                runtime_content: runtime,
                portable_metadata: candidate.portable_metadata,
            };
            let content = crate::revision_content::validate_revision_content(
                definition.revision,
                definition.runtime_content,
            )
            .map_err(err)?;
            let digest = crate::revision_content::calculate_revision_content_digest(&content)
                .map_err(err)?;
            Ok(PreparedRevision {
                identity: RevisionIdentity::new(definition.package_id, digest),
                content,
                metadata: definition.portable_metadata,
                blobs,
            })
        }
        (None, Some(descriptor)) => {
            let bytes = read_json(descriptor)?;
            fn has_null(value: &crate::strict_json::RawJsonValue) -> bool {
                use crate::strict_json::RawJsonValue::*;
                match value {
                    Null => true,
                    Array(items) => items.iter().any(has_null),
                    Object(items) => items.iter().any(|(_, v)| has_null(v)),
                    _ => false,
                }
            }
            if has_null(&crate::strict_json::parse_json(&bytes, JSON_LIMIT as usize).map_err(err)?)
            {
                return Err("Pack descriptor optional values must be omitted, not null".into());
            }
            let dto: Descriptor = serde_json::from_slice(&bytes).map_err(err)?;
            if dto.kind != "pactrun_distribution" || dto.format_version != 1 {
                return Err("unsupported Pack distribution format".into());
            }
            let identity = RevisionIdentity::new(
                dto.package_id.parse().map_err(err)?,
                dto.revision_digest.parse().map_err(err)?,
            );
            let content = crate::revision_content::decode_canonical_revision_content(
                &read_json(input.required(CORE)?)?,
                &read_json(input.required(RUNTIME)?)?,
            )
            .map_err(err)?;
            if crate::revision_content::calculate_revision_content_digest(&content).map_err(err)?
                != identity.content_digest
            {
                return Err("Pack Revision digest mismatch".into());
            }
            let mut expected =
                BTreeSet::from([DESCRIPTOR.to_owned(), CORE.to_owned(), RUNTIME.to_owned()]);
            let mut blobs = BTreeMap::new();
            for descriptor in content.runtime_content.files() {
                if blobs.contains_key(&descriptor.blob_digest) {
                    continue;
                }
                let name = blob_name(&descriptor.blob_digest);
                expected.insert(name.clone());
                let mut reader = CancelReader {
                    inner: input.required(&name)?,
                    cancellation: c,
                };
                let blob = session.stage_runtime_source(&mut reader).map_err(err)?;
                if blob.blob_digest != descriptor.blob_digest {
                    return Err("Pack runtime blob digest mismatch".into());
                }
                blobs.insert(blob.blob_digest.clone(), blob);
            }
            if input.names()? != expected {
                return Err("distribution Pack has missing or unreferenced members".into());
            }
            Ok(PreparedRevision {
                identity,
                content,
                metadata: dto
                    .portable_metadata
                    .map(|m| m.decode())
                    .transpose()?
                    .unwrap_or_default(),
                blobs,
            })
        }
    }
}
fn blob_name(digest: &Sha256Digest) -> String {
    format!("blobs/sha256/{}", hex::encode(digest.to_bytes()))
}

fn expand_entry(
    file: &mut File,
    e: &crate::pack_zip::Entry,
    session: &StagingSession,
    c: &ActionCancellation,
) -> Result<StagedRuntimeSource> {
    file.seek(SeekFrom::Start(e.data)).map_err(err)?;
    let mut input = file.take(e.compressed);
    let mut stage = session.create_snapshot_stage().map_err(err)?;
    let mut crc = crc32fast::Hasher::new();
    let mut sha = Sha256::new();
    let mut count = 0u64;
    let mut output = |bytes: &[u8]| -> Result<()> {
        check_cancel(c).map_err(err)?;
        count = count
            .checked_add(bytes.len() as u64)
            .ok_or("Pack length overflow")?;
        if count > e.size {
            return Err("Pack decompression exceeds declared length".into());
        }
        stage.writer().write_all(bytes).map_err(err)?;
        crc.update(bytes);
        sha.update(bytes);
        Ok(())
    };
    let mut buffer = [0u8; 64 * 1024];
    if e.method == 0 {
        loop {
            check_cancel(c).map_err(err)?;
            let n = input.read(&mut buffer).map_err(err)?;
            if n == 0 {
                break;
            }
            output(&buffer[..n])?;
        }
    } else {
        let mut decoder = flate2::Decompress::new(false);
        let mut decoded = [0u8; 64 * 1024];
        let mut offset = 0;
        let mut available = 0;
        loop {
            check_cancel(c).map_err(err)?;
            if offset == available {
                available = input.read(&mut buffer).map_err(err)?;
                offset = 0;
            }
            let before_in = decoder.total_in();
            let before_out = decoder.total_out();
            let status = decoder
                .decompress(
                    &buffer[offset..available],
                    &mut decoded,
                    flate2::FlushDecompress::None,
                )
                .map_err(err)?;
            let consumed = (decoder.total_in() - before_in) as usize;
            let produced = (decoder.total_out() - before_out) as usize;
            offset += consumed;
            output(&decoded[..produced])?;
            if status == flate2::Status::StreamEnd {
                if decoder.total_in() != e.compressed {
                    return Err("Pack Deflate trailing or missing bytes".into());
                }
                break;
            }
            if consumed == 0 && produced == 0 {
                return Err("truncated or stalled Pack Deflate stream".into());
            }
        }
    }
    if count != e.size || crc.finalize() != e.crc {
        return Err("Pack size or CRC mismatch".into());
    }
    stage.finish_operation_file().map_err(err)?;
    Ok(StagedRuntimeSource {
        blob_digest: Sha256Digest::from_bytes(sha.finalize().into()),
        bytes: stage,
    })
}

pub(crate) fn encode(
    pack: &PreparedRevision,
    include_metadata: bool,
    session: &StagingSession,
    c: &ActionCancellation,
) -> Result<StagedFile> {
    let dto = Descriptor {
        kind: "pactrun_distribution".into(),
        format_version: 1,
        package_id: pack.identity.package_id.to_string(),
        revision_digest: pack.identity.content_digest.to_string(),
        portable_metadata: include_metadata
            .then(|| crate::pack_metadata::Metadata::encode(&pack.metadata)),
    };
    let descriptor = serde_json::to_vec(&dto).map_err(err)?;
    if descriptor.len() as u64 > JSON_LIMIT {
        return Err("Pack descriptor exceeds codec limit".into());
    }
    let mut stage = session.create_snapshot_stage().map_err(err)?;
    {
        if pack.blobs.len().checked_add(3).is_none_or(|n| n > 65_539) {
            return Err("Pack entry limit exceeded".into());
        }
        let failed = std::rc::Rc::new(std::cell::Cell::new(None));
        let mut writer = zip::ZipWriter::new(crate::zip_output::DropSafeWriter {
            inner: stage.writer(),
            failed: failed.clone(),
            position: 0,
            length: 0,
        });
        let small = [
            (DESCRIPTOR, descriptor),
            (
                CORE,
                crate::revision_content::encode_canonical_revision_core(&pack.content.core)
                    .map_err(err)?,
            ),
            (
                RUNTIME,
                crate::revision_core_v1::encode_canonical_runtime_content_v1(
                    &pack.content.runtime_content,
                )
                .map_err(err)?,
            ),
        ];
        for (name, bytes) in small {
            check_cancel(c).map_err(err)?;
            writer
                .start_file(
                    name,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .map_err(err)?;
            writer.write_all(&bytes).map_err(err)?;
        }
        for (digest, blob) in &pack.blobs {
            check_cancel(c).map_err(err)?;
            writer
                .start_file(
                    blob_name(digest),
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated)
                        // Deflate can expand incompressible input across the u32
                        // boundary. Reserve ZIP64 before streaming, even when
                        // the uncompressed length alone still fits in u32.
                        .large_file(true),
                )
                .map_err(err)?;
            let mut reader = CancelReader {
                inner: blob.bytes.try_clone_reader().map_err(err)?,
                cancellation: c,
            };
            // copy does not swallow cancellation: use an explicit loop instead of io::copy.
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let n = reader.read(&mut buffer).map_err(err)?;
                if n == 0 {
                    break;
                }
                writer.write_all(&buffer[..n]).map_err(err)?;
            }
        }
        writer.finish().map_err(err)?;
        if let Some(kind) = failed.get() {
            return Err(format!("Pack ZIP output failed: {kind}"));
        }
    }
    stage.finish_operation_file().map_err(err)?;
    Ok(stage)
}

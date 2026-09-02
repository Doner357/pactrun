//! Opened-object source acquisition for PackSourceYamlV1.

use std::{fmt, fs::File, io, path::Path};

use super::SourceRelativePathV1;

const MANIFEST_NAME: &str = "pactrun.yaml";

#[derive(Debug)]
pub(crate) struct SecureSourceRoot {
    root: File,
}

#[derive(Debug)]
pub(crate) enum SourceAcquisitionError {
    Io {
        operation: &'static str,
        source: io::Error,
    },
    Unsupported(String),
}

impl SourceAcquisitionError {
    fn io(operation: &'static str, source: io::Error) -> Self {
        if source.kind() == io::ErrorKind::Unsupported {
            Self::Unsupported(source.to_string())
        } else {
            Self::Io { operation, source }
        }
    }
}

impl fmt::Display for SourceAcquisitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::Unsupported(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for SourceAcquisitionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Unsupported(_) => None,
        }
    }
}

impl SecureSourceRoot {
    pub(crate) fn open(path: &Path) -> Result<Self, SourceAcquisitionError> {
        platform::open_root(path)
            .map(|root| Self { root })
            .map_err(|error| SourceAcquisitionError::io("open Pack source root", error))
    }

    pub(crate) fn open_manifest(&self) -> Result<File, SourceAcquisitionError> {
        platform::open_file(&self.root, &[MANIFEST_NAME])
            .map_err(|error| SourceAcquisitionError::io("open exact pactrun.yaml", error))
    }

    pub(crate) fn open_runtime_source(
        &self,
        path: &SourceRelativePathV1,
    ) -> Result<File, SourceAcquisitionError> {
        let segments = path.as_str().split('/').collect::<Vec<_>>();
        platform::open_file(&self.root, &segments)
            .map_err(|error| SourceAcquisitionError::io("open runtime-content source", error))
    }
}

#[cfg(windows)]
mod platform {
    use std::{fs::File, io, path::Path};

    pub(super) fn open_root(path: &Path) -> io::Result<File> {
        pactrun_windows_ntfs::open_root(path)
    }

    pub(super) fn open_file(root: &File, segments: &[&str]) -> io::Result<File> {
        pactrun_windows_ntfs::open_source_file(root, segments)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::{fs::File, io, path::Path};

    use rustix::fs::{Dir, FileType, Mode, OFlags, ResolveFlags, fstat, fstatfs, open, openat2};

    const EXT4_SUPER_MAGIC: u64 = 0x0000_ef53;
    const XFS_SUPER_MAGIC: u64 = 0x5846_5342;
    const BTRFS_SUPER_MAGIC: u64 = 0x9123_683e;
    const ZFS_SUPER_MAGIC: u64 = 0x2fc1_2fc1;

    pub(super) fn open_root(path: &Path) -> io::Result<File> {
        let root = open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let filesystem = fstatfs(&root)?.f_type as u64;
        if !matches!(
            filesystem,
            EXT4_SUPER_MAGIC | XFS_SUPER_MAGIC | BTRFS_SUPER_MAGIC | ZFS_SUPER_MAGIC
        ) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("unsupported Linux Pack source filesystem type 0x{filesystem:x}"),
            ));
        }
        let stat = fstat(&root)?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::Directory {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Pack source root is not a directory",
            ));
        }
        Ok(File::from(root))
    }

    pub(super) fn open_file(root: &File, segments: &[&str]) -> io::Result<File> {
        if segments.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "source path has no components",
            ));
        }
        let mut current = root.try_clone()?;
        let resolve = ResolveFlags::BENEATH
            | ResolveFlags::NO_SYMLINKS
            | ResolveFlags::NO_MAGICLINKS
            | ResolveFlags::NO_XDEV;
        for (index, segment) in segments.iter().enumerate() {
            let final_component = index + 1 == segments.len();
            let expected_inode =
                exact_entry_inode(&current, segment.as_bytes())?.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "source directory has no exact requested UTF-8 entry name",
                    )
                })?;
            let flags = OFlags::RDONLY
                | OFlags::CLOEXEC
                | if final_component {
                    OFlags::empty()
                } else {
                    OFlags::DIRECTORY
                };
            let opened = openat2(&current, *segment, flags, Mode::empty(), resolve)?;
            let stat = fstat(&opened)?;
            let expected_kind = if final_component {
                FileType::RegularFile
            } else {
                FileType::Directory
            };
            if FileType::from_raw_mode(stat.st_mode) != expected_kind {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "source path component has an unsupported object type",
                ));
            }
            let current_inode =
                exact_entry_inode(&current, segment.as_bytes())?.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "source entry changed during exact-object acquisition",
                    )
                })?;
            if expected_inode != stat.st_ino || current_inode != stat.st_ino {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "source entry changed during exact-object acquisition",
                ));
            }
            current = File::from(opened);
        }
        Ok(current)
    }

    fn exact_entry_inode(directory: &File, requested: &[u8]) -> io::Result<Option<u64>> {
        let mut entries = Dir::read_from(directory)?;
        let mut found = None;
        while let Some(entry) = entries.read() {
            let entry = entry?;
            if entry.file_name().to_bytes() == requested && found.replace(entry.ino()).is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "directory exposes duplicate exact entry names",
                ));
            }
        }
        Ok(found)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
compile_error!("M2 Pack source acquisition supports Windows and Linux only");

#[cfg(test)]
mod tests {
    use std::{fs, io::Read, path::Path};

    use tempfile::TempDir;

    use super::*;

    fn source_root() -> (TempDir, std::path::PathBuf) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m2-source-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("source-")
            .tempdir_in(parent)
            .unwrap();
        fs::write(temporary.path().join(MANIFEST_NAME), b"source_format: 1\n").unwrap();
        fs::create_dir(temporary.path().join("content")).unwrap();
        fs::write(
            temporary.path().join("content").join("Payload.bin"),
            b"bytes",
        )
        .unwrap();
        let root = temporary.path().to_path_buf();
        (temporary, root)
    }

    fn assert_manifest_and_runtime_sources_are_exact_opened_objects() {
        let (_temporary, path) = source_root();
        let root = SecureSourceRoot::open(&path).unwrap();
        let mut manifest = String::new();
        root.open_manifest()
            .unwrap()
            .read_to_string(&mut manifest)
            .unwrap();
        assert_eq!(manifest, "source_format: 1\n");
        let source = SourceRelativePathV1::parse("content/Payload.bin").unwrap();
        let mut bytes = Vec::new();
        root.open_runtime_source(&source)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"bytes");

        let alias = SourceRelativePathV1::parse("content/payload.bin").unwrap();
        assert!(root.open_runtime_source(&alias).is_err());
    }

    // Test-ID: PR-TEST-0070
    // Verifies: PR-REQ-0259
    #[cfg(windows)]
    #[test]
    fn windows_manifest_and_runtime_sources_are_exact_opened_objects() {
        assert_manifest_and_runtime_sources_are_exact_opened_objects();
    }

    // Test-ID: PR-TEST-0071
    // Verifies: PR-REQ-0259
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_manifest_and_runtime_sources_are_exact_opened_objects() {
        assert_manifest_and_runtime_sources_are_exact_opened_objects();
    }

    #[test]
    fn symbolic_or_reparse_final_objects_are_rejected_when_creation_is_available() {
        let (_temporary, path) = source_root();
        let link = path.join("content").join("link.bin");
        #[cfg(windows)]
        let created =
            std::os::windows::fs::symlink_file(path.join("content").join("Payload.bin"), &link);
        #[cfg(target_os = "linux")]
        let created = std::os::unix::fs::symlink(path.join("content").join("Payload.bin"), &link);
        if created.is_err() {
            return;
        }
        let root = SecureSourceRoot::open(&path).unwrap();
        let source = SourceRelativePathV1::parse("content/link.bin").unwrap();
        assert!(root.open_runtime_source(&source).is_err());
    }
}

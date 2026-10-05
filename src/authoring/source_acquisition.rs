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
    pub(crate) fn file_names(
        &self,
        maximum: usize,
    ) -> io::Result<std::collections::BTreeSet<String>> {
        platform::file_names(&self.root, maximum)
    }
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

use crate::opened_files as platform;

#[cfg(test)]
mod tests {
    use std::{fs, io::Read, path::Path};

    use tempfile::TempDir;

    use super::*;

    fn source_root() -> (TempDir, std::path::PathBuf) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/source-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("source-")
            .tempdir_in(parent)
            .unwrap();
        fs::write(
            temporary.path().join(MANIFEST_NAME),
            b"source_format: 1.0-alpha.1\n",
        )
        .unwrap();
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
        assert_eq!(manifest, "source_format: 1.0-alpha.1\n");
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

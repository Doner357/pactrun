//! Filesystem substrate for provided live storage. This layer never owns live
//! content, implements no GC, and does not itself publish an Instance/association.
use crate::domain::{ObservedServiceObjectKind as ObservedObjectKind, ServiceLocatorV2};
use std::{fs::File, io, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResourceLookup {
    Present {
        kind: ObservedObjectKind,
        links: u64,
    },
    Absent {
        parent_exists: bool,
    },
}

pub(crate) fn observation_cause(error: io::Error) -> crate::domain::ServiceObservationCause {
    use crate::domain::ServiceObservationCause as Cause;
    #[cfg(target_os = "linux")]
    if matches!(error.raw_os_error(), Some(18 | 40)) {
        return Cause::UnsafePath;
    }
    match error.kind() {
        io::ErrorKind::PermissionDenied => Cause::PermissionDenied,
        io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput | io::ErrorKind::NotADirectory => {
            Cause::UnsafePath
        }
        _ => Cause::IoFailure,
    }
}

/// One no-follow, metadata-only lookup inside an already qualified allocation.
/// Errors are Unknown, never absence. The result is not a lock on live state.
pub(crate) fn observe_resource(
    allocation: &File,
    locator: &ServiceLocatorV2,
) -> io::Result<ResourceLookup> {
    let components: Vec<_> = locator.as_str().split('/').collect();
    let mut parent = allocation.try_clone()?;
    for (index, component) in components.iter().enumerate() {
        let leaf = index + 1 == components.len();
        if leaf {
            return match platform::observe_entry(&parent, component) {
                Ok((kind, links)) => Ok(ResourceLookup::Present { kind, links }),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(ResourceLookup::Absent {
                    parent_exists: true,
                }),
                Err(e) => Err(e),
            };
        }
        parent = match open_directory(&parent, component) {
            Ok(next) => next,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(ResourceLookup::Absent {
                    parent_exists: false,
                });
            }
            Err(e) => return Err(e),
        };
    }
    unreachable!("typed locator has at least one component")
}

pub(crate) fn create_directory(root: &File, name: &str) -> io::Result<File> {
    platform::create_directory(root, name)
}
pub(crate) fn open_directory(root: &File, name: &str) -> io::Result<File> {
    platform::open_directory(root, name)
}
pub(crate) fn open_root(path: &Path) -> io::Result<File> {
    crate::opened_files::open_root(path)
}

pub(crate) fn same_opened_object(left: &File, right: &File) -> io::Result<bool> {
    platform::same_opened_object(left, right)
}

#[cfg(windows)]
mod platform {
    use super::*;
    pub(super) fn observe_entry(root: &File, name: &str) -> io::Result<(ObservedObjectKind, u64)> {
        let (file, links) = pactrun_windows_ntfs::open_service_entry(root, name)?;
        let metadata = file.metadata()?;
        let kind = if metadata.is_file() {
            ObservedObjectKind::File
        } else if metadata.is_dir() {
            ObservedObjectKind::Directory
        } else {
            ObservedObjectKind::Other
        };
        Ok((kind, u64::from(links)))
    }
    pub(super) fn same_opened_object(left: &File, right: &File) -> io::Result<bool> {
        pactrun_windows_ntfs::same_opened_object(left, right)
    }
    pub(super) fn create_directory(root: &File, name: &str) -> io::Result<File> {
        pactrun_windows_ntfs::create_service_directory(root, name)
    }
    pub(super) fn open_directory(root: &File, name: &str) -> io::Result<File> {
        pactrun_windows_ntfs::open_service_directory(root, name)
    }
}
#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use rustix::fs::{Mode, OFlags, ResolveFlags, fsync, mkdirat, openat2};
    fn exact_inode(root: &File, name: &str) -> io::Result<Option<u64>> {
        let mut entries = rustix::fs::Dir::read_from(root)?;
        let mut found = None;
        while let Some(entry) = entries.read() {
            let entry = entry?;
            if entry.file_name().to_bytes() == name.as_bytes()
                && found.replace(entry.ino()).is_some()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ambiguous service entry",
                ));
            }
        }
        Ok(found)
    }
    pub(super) fn observe_entry(root: &File, name: &str) -> io::Result<(ObservedObjectKind, u64)> {
        valid(name)?;
        let expected = exact_inode(root, name)?;
        let file = openat2(
            root,
            name,
            OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
            ResolveFlags::BENEATH
                | ResolveFlags::NO_SYMLINKS
                | ResolveFlags::NO_MAGICLINKS
                | ResolveFlags::NO_XDEV,
        )?;
        let stat = rustix::fs::fstat(&file)?;
        if expected != Some(stat.st_ino) || exact_inode(root, name)? != Some(stat.st_ino) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "service entry changed or has a different exact name",
            ));
        }
        use rustix::fs::FileType;
        let kind = match FileType::from_raw_mode(stat.st_mode) {
            FileType::RegularFile => ObservedObjectKind::File,
            FileType::Directory => ObservedObjectKind::Directory,
            FileType::Symlink => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "service entry is a symlink",
                ));
            }
            _ => ObservedObjectKind::Other,
        };
        Ok((kind, stat.st_nlink as u64))
    }
    pub(super) fn same_opened_object(left: &File, right: &File) -> io::Result<bool> {
        let left = rustix::fs::fstat(left)?;
        let right = rustix::fs::fstat(right)?;
        Ok((left.st_dev, left.st_ino) == (right.st_dev, right.st_ino))
    }
    fn valid(name: &str) -> io::Result<()> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.len() > 128
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected one service directory component",
            ));
        }
        Ok(())
    }
    pub(super) fn open_directory(root: &File, name: &str) -> io::Result<File> {
        valid(name)?;
        let expected = exact_inode(root, name)?;
        let file = File::from(openat2(
            root,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
            ResolveFlags::BENEATH
                | ResolveFlags::NO_SYMLINKS
                | ResolveFlags::NO_MAGICLINKS
                | ResolveFlags::NO_XDEV,
        )?);
        let inode = rustix::fs::fstat(&file)?.st_ino;
        if expected != Some(inode) || exact_inode(root, name)? != Some(inode) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "service directory changed or has a different exact name",
            ));
        }
        Ok(file)
    }
    pub(super) fn create_directory(root: &File, name: &str) -> io::Result<File> {
        valid(name)?;
        mkdirat(root, name, Mode::from_raw_mode(0o700))?;
        let directory = open_directory(root, name)?;
        fsync(&directory)?;
        fsync(root)?;
        Ok(directory)
    }
}
#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::*;
    pub(super) fn observe_entry(_: &File, _: &str) -> io::Result<(ObservedObjectKind, u64)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "service storage filesystem unsupported",
        ))
    }
    pub(super) fn same_opened_object(_: &File, _: &File) -> io::Result<bool> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "service storage filesystem unsupported",
        ))
    }
    pub(super) fn create_directory(_: &File, _: &str) -> io::Result<File> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "service storage filesystem unsupported",
        ))
    }
    pub(super) fn open_directory(_: &File, _: &str) -> io::Result<File> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "service storage filesystem unsupported",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn permission_probe_worker() {
        use std::os::fd::AsFd;
        if std::env::var_os("PACTRUN_SERVICE_PERMISSION_PROBE").is_none() {
            return;
        }
        let root = File::from(std::io::stdin().as_fd().try_clone_to_owned().unwrap());
        assert!(matches!(
            observe_resource(&root, &ServiceLocatorV2::parse("readable").unwrap()).unwrap(),
            ResourceLookup::Present { .. }
        ));
        for name in ["blocked/existing", "blocked/missing"] {
            let error =
                observe_resource(&root, &ServiceLocatorV2::parse(name).unwrap()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(
                observation_cause(error),
                crate::domain::ServiceObservationCause::PermissionDenied
            );
        }
    }

    // Test-ID: PR-TEST-0387
    // Verifies: PR-REQ-0242, PR-REQ-0326, PR-REQ-0328
    #[cfg(target_os = "linux")]
    #[test]
    fn actual_permission_denial_cannot_be_inferred_as_resource_absence() {
        use std::os::unix::{fs::PermissionsExt, process::CommandExt};
        use std::process::{Command, Stdio};
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m65-observation-tests");
        std::fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::tempdir_in(parent).unwrap();
        std::fs::set_permissions(temporary.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(temporary.path().join("readable"), b"visible").unwrap();
        let blocked = temporary.path().join("blocked");
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("existing"), b"not inspected").unwrap();
        let root = open_root(temporary.path()).unwrap();
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o0)).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "service_storage::tests::permission_probe_worker",
                "--nocapture",
            ])
            .env("PACTRUN_SERVICE_PERMISSION_PROBE", "1")
            .stdin(Stdio::from(root));
        // A privileged test runner must not bypass the denial. Drop privilege
        // in this child only; the directory capability avoids home-path access.
        if rustix::process::geteuid().as_raw() == 0 {
            command.uid(65534).gid(65534);
        }
        let result = command.status();
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.unwrap().success());
        assert_eq!(
            std::fs::read(blocked.join("existing")).unwrap(),
            b"not inspected"
        );
        assert!(!blocked.join("missing").exists());
    }

    // Test-ID: PR-TEST-0354
    // Verifies: PR-REQ-0326, PR-REQ-0328, PR-REQ-0242
    #[test]
    fn resource_observation_distinguishes_absence_kind_and_link_count_without_content_reads() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m65-observation-tests");
        std::fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::tempdir_in(parent).unwrap();
        let root = open_root(temporary.path()).unwrap();
        let observe = |name: &str| observe_resource(&root, &ServiceLocatorV2::parse(name).unwrap());
        assert_eq!(
            observe("missing").unwrap(),
            ResourceLookup::Absent {
                parent_exists: true
            }
        );
        assert_eq!(
            observe("missing/child").unwrap(),
            ResourceLookup::Absent {
                parent_exists: false
            }
        );
        std::fs::write(temporary.path().join("file"), b"service bytes").unwrap();
        std::fs::create_dir(temporary.path().join("directory")).unwrap();
        assert!(matches!(
            observe("directory").unwrap(),
            ResourceLookup::Present {
                kind: ObservedObjectKind::Directory,
                ..
            }
        ));
        assert!(matches!(
            observe("file").unwrap(),
            ResourceLookup::Present {
                kind: ObservedObjectKind::File,
                links: 1
            }
        ));
        assert!(observe("file/child").is_err());
        std::fs::hard_link(
            temporary.path().join("file"),
            temporary.path().join("alias"),
        )
        .unwrap();
        assert!(matches!(
            observe("file").unwrap(),
            ResourceLookup::Present {
                kind: ObservedObjectKind::File,
                links: 2
            }
        ));
        assert_eq!(
            std::fs::read(temporary.path().join("file")).unwrap(),
            b"service bytes"
        );
        assert!(!temporary.path().join("missing").exists());
        #[cfg(windows)]
        assert!(observe("FILE").is_err()); // case-folded lookup is not exact spelling
        #[cfg(target_os = "linux")]
        {
            rustix::fs::mkfifoat(&root, "fifo", rustix::fs::Mode::from_raw_mode(0o600)).unwrap();
            assert!(matches!(
                observe("fifo").unwrap(),
                ResourceLookup::Present {
                    kind: ObservedObjectKind::Other,
                    ..
                }
            ));
        }
    }

    // Test-ID: PR-TEST-0355
    // Verifies: PR-REQ-0326, PR-REQ-0328
    #[test]
    fn resource_observation_refuses_reparse_or_symlink_prefixes_and_leaves_target_untouched() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m65-observation-tests");
        std::fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::tempdir_in(parent).unwrap();
        let outside = tempfile::tempdir_in(temporary.path().parent().unwrap()).unwrap();
        std::fs::write(outside.path().join("sentinel"), b"outside service bytes").unwrap();
        let link = temporary.path().join("alias");
        #[cfg(target_os = "linux")]
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let script = temporary.path().join("junction.ps1");
            std::fs::write(&script, b"param([string]$Link,[string]$Target)\n$ErrorActionPreference='Stop'\nNew-Item -ItemType Junction -Path $Link -Target $Target | Out-Null\n").unwrap();
            assert!(
                std::process::Command::new("powershell.exe")
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-File"
                    ])
                    .arg(script)
                    .arg(&link)
                    .arg(outside.path())
                    .creation_flags(0x0800_0000)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let root = open_root(temporary.path()).unwrap();
        for name in ["alias", "alias/sentinel", "alias/missing"] {
            assert!(
                observe_resource(&root, &ServiceLocatorV2::parse(name).unwrap()).is_err(),
                "{name}"
            );
        }
        assert_eq!(
            std::fs::read(outside.path().join("sentinel")).unwrap(),
            b"outside service bytes"
        );
        assert!(!outside.path().join("missing").exists());
    }
    // Test-ID: PR-TEST-0345
    // Verifies: PR-REQ-0324
    #[test]
    fn actual_directory_creation_is_no_replace_and_root_relative() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m65-service-fs-tests");
        std::fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::tempdir_in(parent).unwrap();
        let root = open_root(temporary.path()).unwrap();
        let directory = create_directory(&root, "allocation").unwrap();
        assert!(directory.metadata().unwrap().is_dir());
        std::fs::write(
            temporary.path().join("allocation/sentinel"),
            b"service-owned",
        )
        .unwrap();
        assert!(create_directory(&root, "allocation").is_err());
        assert_eq!(
            std::fs::read(temporary.path().join("allocation/sentinel")).unwrap(),
            b"service-owned"
        );
        assert!(
            open_directory(&root, "allocation")
                .unwrap()
                .metadata()
                .unwrap()
                .is_dir()
        );
        for invalid in ["", ".", "..", "../outside", "a/b", "a\\b", "C:escape"] {
            assert!(create_directory(&root, invalid).is_err());
        }
    }
}

//! Shared opened-object filesystem acquisition; adapters own lexical policy.
use std::{fs::File, io, path::Path};
pub(crate) fn open_root(path: &Path) -> io::Result<File> {
    platform::open_root(path)
}
pub(crate) fn open_file(root: &File, segments: &[&str]) -> io::Result<File> {
    platform::open_file(root, segments)
}

/// Enumerate relative to opened directory handles; never follow reconstructed paths.
pub(crate) fn file_names(
    root: &File,
    maximum: usize,
) -> io::Result<std::collections::BTreeSet<String>> {
    let mut result = std::collections::BTreeSet::new();
    let mut pending = vec![(String::new(), root.try_clone()?)];
    let mut count = 0usize;
    while let Some((prefix, directory)) = pending.pop() {
        for name in platform::names(&directory, maximum)? {
            count = count
                .checked_add(1)
                .ok_or_else(|| io::Error::other("Pack entry overflow"))?;
            if count > maximum {
                return Err(io::Error::other("Pack entry limit exceeded"));
            }
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if relative.len() > 1024 {
                return Err(io::Error::other("Pack member path limit exceeded"));
            }
            let entry = platform::entry(&directory, &name)?;
            if entry.metadata()?.is_dir() {
                pending.push((relative, entry));
            } else {
                result.insert(relative);
            }
        }
    }
    Ok(result)
}

#[cfg(windows)]
mod platform {
    use std::{fs::File, io, path::Path};

    pub(super) fn names(root: &File, maximum: usize) -> io::Result<Vec<String>> {
        pactrun_windows_ntfs::source_names(root, maximum)?
            .into_iter()
            .map(|s| {
                s.into_string()
                    .map_err(|_| io::Error::other("non-UTF-8 Pack member"))
            })
            .collect()
    }
    pub(super) fn entry(root: &File, name: &str) -> io::Result<File> {
        let (observed, _) = pactrun_windows_ntfs::open_service_entry(root, name)?;
        if observed.metadata()?.is_dir() {
            let readable = pactrun_windows_ntfs::open_service_directory(root, name)?;
            if !pactrun_windows_ntfs::same_opened_object(&observed, &readable)? {
                return Err(io::Error::other(
                    "Pack directory changed during acquisition",
                ));
            }
            Ok(readable)
        } else {
            Ok(observed)
        }
    }

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

    pub(super) fn names(root: &File, maximum: usize) -> io::Result<Vec<String>> {
        let mut entries = Dir::read_from(root)?;
        let mut names = Vec::new();
        while let Some(entry) = entries.read() {
            let entry = entry?;
            let n = entry.file_name().to_bytes();
            if n == b"." || n == b".." {
                continue;
            }
            if names.len() >= maximum {
                return Err(io::Error::other("Pack directory entry limit exceeded"));
            }
            names.push(
                std::str::from_utf8(n)
                    .map_err(|_| io::Error::other("non-UTF-8 Pack member"))?
                    .to_owned(),
            );
        }
        Ok(names)
    }
    pub(super) fn entry(root: &File, name: &str) -> io::Result<File> {
        let opened = openat2(
            root,
            name,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
            ResolveFlags::BENEATH
                | ResolveFlags::NO_SYMLINKS
                | ResolveFlags::NO_MAGICLINKS
                | ResolveFlags::NO_XDEV,
        )?;
        if !matches!(
            FileType::from_raw_mode(fstat(&opened)?.st_mode),
            FileType::RegularFile | FileType::Directory
        ) {
            return Err(io::Error::other("unsupported Pack entry type"));
        }
        Ok(File::from(opened))
    }

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
                        io::ErrorKind::NotFound,
                        "source directory has no exact requested UTF-8 entry name",
                    )
                })?;
            let flags = OFlags::RDONLY
                | OFlags::NONBLOCK
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
compile_error!("Pack source acquisition supports Windows and Linux only");

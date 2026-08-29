use std::{fs::File, io, os::fd::OwnedFd, path::Path};

use rustix::fs::{Mode, OFlags, RenameFlags, fstat, fstatfs, openat, renameat_with, unlinkat};

const EXT4_SUPER_MAGIC: u64 = 0x0000_ef53;
const XFS_SUPER_MAGIC: u64 = 0x5846_5342;
const BTRFS_SUPER_MAGIC: u64 = 0x9123_683e;
const ZFS_SUPER_MAGIC: u64 = 0x2fc1_2fc1;

pub(super) fn open_root(path: &Path) -> io::Result<File> {
    let fd = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let statfs = fstatfs(&fd)?;
    let filesystem = statfs.f_type as u64;
    if !matches!(
        filesystem,
        EXT4_SUPER_MAGIC | XFS_SUPER_MAGIC | BTRFS_SUPER_MAGIC | ZFS_SUPER_MAGIC
    ) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("unsupported Linux filesystem type 0x{filesystem:x}"),
        ));
    }
    let file = File::from(fd);
    file.sync_all()?;
    Ok(file)
}

pub(super) fn open_lock(_root: &Path, root_file: &File) -> io::Result<File> {
    let fd = openat(
        root_file,
        ".publish.lock",
        OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?;
    regular_file(fd)
}

pub(super) fn create_staging(_root: &Path, name: &str, root_file: &File) -> io::Result<File> {
    let fd = openat(
        root_file,
        name,
        OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?;
    regular_file(fd)
}

pub(super) fn open_existing_durable(
    _root: &Path,
    name: &str,
    root_file: &File,
) -> io::Result<File> {
    let fd = openat(
        root_file,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    regular_file(fd)
}

pub(super) fn open_existing_read(root: &Path, name: &str, root_file: &File) -> io::Result<File> {
    open_existing_durable(root, name, root_file)
}

pub(super) fn sync_file(file: &File) -> io::Result<()> {
    file.sync_all()
}

pub(super) fn publish_no_replace(
    _root: &Path,
    root_file: &File,
    staging_name: &str,
    _staging: &File,
    final_name: &str,
) -> io::Result<()> {
    renameat_with(
        root_file,
        staging_name,
        root_file,
        final_name,
        RenameFlags::NOREPLACE,
    )?;
    Ok(())
}

pub(super) fn sync_namespace(root_file: &File) -> io::Result<()> {
    root_file.sync_all()
}

pub(super) fn remove_staging(_root: &Path, name: &str, root_file: &File) -> io::Result<()> {
    unlinkat(root_file, name, rustix::fs::AtFlags::empty())?;
    Ok(())
}

fn regular_file(fd: OwnedFd) -> io::Result<File> {
    let stat = fstat(&fd)?;
    if rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::RegularFile {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "filesystem entry is not a regular file",
        ));
    }
    Ok(File::from(fd))
}

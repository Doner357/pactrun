use super::collection::{Candidate, NativeNames};
use std::os::unix::ffi::OsStringExt;
use std::{fs::File, io, os::fd::OwnedFd, path::Path};

use rustix::fs::{
    Mode, OFlags, RenameFlags, ResolveFlags, fstat, fstatfs, openat, openat2, renameat_with,
    unlinkat,
};

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

pub(super) fn open_collection_lock(
    _root: &Path,
    root_file: &File,
    create: bool,
) -> io::Result<File> {
    let flags = if create {
        OFlags::RDWR | OFlags::CREATE
    } else {
        OFlags::RDONLY
    };
    regular_file(openat(
        root_file,
        ".collection.lock",
        flags | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?)
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
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
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

pub(super) fn collection_names(parent: &File) -> io::Result<NativeNames> {
    let fresh = openat(
        parent,
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let mut dir = rustix::fs::Dir::read_from(&fresh)?;
    let mut names = vec![];
    while let Some(entry) = dir.read() {
        let entry = entry?;
        let name = entry.file_name().to_bytes();
        if name != b"." && name != b".." {
            names.push(std::ffi::OsString::from_vec(name.to_vec()));
        }
    }
    names.sort();
    Ok(names)
}

pub(super) fn open_collection_claim(root: &File, name: &str) -> io::Result<File> {
    let fd = openat2(
        root,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::BENEATH
            | ResolveFlags::NO_SYMLINKS
            | ResolveFlags::NO_MAGICLINKS
            | ResolveFlags::NO_XDEV,
    )?;
    let stat = fstat(&fd)?;
    let owner = fstat(root)?;
    if stat.st_mode & 0o777 != 0o700 || stat.st_uid != owner.st_uid || stat.st_dev != owner.st_dev {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unqualified collection claim",
        ));
    }
    Ok(fd.into())
}

pub(super) fn open_collection_blob(
    root: &File,
    candidate: &Candidate,
    _execute: bool,
) -> io::Result<File> {
    let parent = match &candidate.claim {
        Some(name) => open_collection_claim(root, name)?,
        None => root.try_clone()?,
    };
    let fd = openat2(
        &parent,
        candidate.name.as_str(),
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::BENEATH
            | ResolveFlags::NO_SYMLINKS
            | ResolveFlags::NO_MAGICLINKS
            | ResolveFlags::NO_XDEV,
    )?;
    if fstat(&fd)?.st_nlink != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "shared runtime blob is not collectible",
        ));
    }
    regular_file(fd)
}

pub(super) fn remove_collection_blob(
    root: &File,
    candidate: &Candidate,
    expected: &File,
) -> io::Result<()> {
    let fresh = candidate.claim.is_none();
    let claim = if let Some(claim) = &candidate.claim {
        claim.clone()
    } else {
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(io::Error::other)?;
        let name = format!(".gc-{}", hex::encode(random));
        rustix::fs::mkdirat(root, name.as_str(), Mode::RWXU)?;
        root.sync_all()?;
        name
    };
    let private = open_collection_claim(root, &claim)?;
    if fresh {
        renameat_with(
            root,
            candidate.name.as_str(),
            &private,
            candidate.name.as_str(),
            RenameFlags::NOREPLACE,
        )?;
        private.sync_all()?;
        root.sync_all()?;
        crate::persistence::fault(crate::persistence::FaultPoint::AfterCollectionClaim);
    }
    let moved = open_collection_blob(
        root,
        &Candidate {
            name: candidate.name.clone(),
            claim: Some(claim.clone()),
        },
        true,
    );
    let qualified = moved.and_then(|file| {
        let a = fstat(&file)?;
        let b = fstat(expected)?;
        if (a.st_dev, a.st_ino, a.st_nlink) != (b.st_dev, b.st_ino, 1) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "runtime entry changed during collection",
            ));
        }
        Ok(file)
    });
    if let Err(error) = qualified {
        if fresh
            && renameat_with(
                &private,
                candidate.name.as_str(),
                root,
                candidate.name.as_str(),
                RenameFlags::NOREPLACE,
            )
            .is_ok()
        {
            root.sync_all()?;
            let _ = unlinkat(root, claim.as_str(), rustix::fs::AtFlags::REMOVEDIR);
        }
        return Err(error);
    }
    // The binding is now in a fresh private parent, not the replaceable source namespace.
    unlinkat(
        &private,
        candidate.name.as_str(),
        rustix::fs::AtFlags::empty(),
    )?;
    private.sync_all()?;
    unlinkat(root, claim.as_str(), rustix::fs::AtFlags::REMOVEDIR)?;
    root.sync_all()
}

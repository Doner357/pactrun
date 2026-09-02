//! Narrow safe wrappers for the Win32 calls required by Pactrun's NTFS store.
//!
//! This crate is an internal implementation boundary. It is not a Pactrun API.

#[cfg(windows)]
mod implementation {

    use std::{
        ffi::OsStr,
        fs::{File, OpenOptions},
        io,
        mem::size_of,
        os::windows::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
            io::{AsRawHandle, FromRawHandle},
        },
        path::Path,
        ptr,
    };

    use windows_sys::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
                FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
            },
        },
        Win32::{
            Foundation::{
                GENERIC_READ, GENERIC_WRITE, HANDLE, RtlNtStatusToDosError, UNICODE_STRING,
            },
            Storage::FileSystem::{
                DELETE, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH,
                FILE_NAME_NORMALIZED, FILE_RENAME_INFO, FILE_RENAME_INFO_0, FILE_SHARE_DELETE,
                FILE_SHARE_READ, FILE_SHARE_WRITE, FileRenameInfo, FlushFileBuffers, GetDriveTypeW,
                GetFinalPathNameByHandleW, GetVolumeInformationByHandleW, GetVolumePathNameW,
                SYNCHRONIZE, SetFileInformationByHandle, VOLUME_NAME_DOS,
            },
            System::IO::IO_STATUS_BLOCK,
            System::WindowsProgramming::DRIVE_FIXED,
        },
    };

    const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

    pub fn open_root(path: &Path) -> io::Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        validate_entry(&file, EntryKind::Directory)?;
        validate_local_fixed_ntfs(path, &file)?;
        Ok(file)
    }

    /// Opens one exact source-root-relative path without reparsing any
    /// component. Every intermediate handle is validated before it becomes the
    /// root of the next lookup.
    pub fn open_source_file(root: &File, segments: &[&str]) -> io::Result<File> {
        if segments.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "source path has no components",
            ));
        }
        let mut current = root.try_clone()?;
        for (index, segment) in segments.iter().enumerate() {
            let final_component = index + 1 == segments.len();
            let next = open_relative_component(&current, segment, final_component)?;
            validate_exact_opened_name(&next, segment)?;
            validate_entry(
                &next,
                if final_component {
                    EntryKind::RegularFile
                } else {
                    EntryKind::Directory
                },
            )?;
            current = next;
        }
        Ok(current)
    }

    pub fn open_lock(path: &Path) -> io::Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        validate_entry(&file, EntryKind::RegularFile)?;
        Ok(file)
    }

    pub fn create_staging(path: &Path) -> io::Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE)
            .create_new(true)
            .share_mode(SHARE_ALL)
            .attributes(FILE_ATTRIBUTE_NORMAL)
            .custom_flags(FILE_FLAG_WRITE_THROUGH | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        validate_entry(&file, EntryKind::RegularFile)?;
        Ok(file)
    }

    pub fn open_existing_for_durability(path: &Path) -> io::Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(GENERIC_READ | GENERIC_WRITE)
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_WRITE_THROUGH | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        validate_entry(&file, EntryKind::RegularFile)?;
        Ok(file)
    }

    pub fn open_existing_read(path: &Path) -> io::Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        validate_entry(&file, EntryKind::RegularFile)?;
        Ok(file)
    }

    pub fn flush(file: &File) -> io::Result<()> {
        // SAFETY: `file` owns a valid Windows file handle for the duration of this
        // call. Durable callers open it with GENERIC_WRITE as required.
        let succeeded = unsafe { FlushFileBuffers(file.as_raw_handle().cast()) };
        if succeeded == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub fn rename_no_replace(staging: &File, final_path: &Path) -> io::Result<()> {
        let name = wide(final_path.as_os_str());
        let name_bytes = name.len().checked_mul(size_of::<u16>()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "rename path is too long")
        })?;
        let buffer_bytes = size_of::<FILE_RENAME_INFO>()
            .checked_add(name_bytes.saturating_sub(size_of::<u16>()))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "rename buffer is too large")
            })?;
        let words = buffer_bytes.div_ceil(size_of::<usize>());
        let mut buffer = vec![0_usize; words];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        let name_bytes = u32::try_from(name_bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "rename path is too long"))?;
        let buffer_bytes = u32::try_from(buffer_bytes).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "rename buffer is too large")
        })?;

        // SAFETY: `buffer` is aligned for `FILE_RENAME_INFO` and sized for its
        // fixed header plus every UTF-16 code unit copied into the trailing array.
        // `staging` remains open and was created with DELETE access.
        let succeeded = unsafe {
            ptr::write(
                info,
                FILE_RENAME_INFO {
                    Anonymous: FILE_RENAME_INFO_0 {
                        ReplaceIfExists: false,
                    },
                    RootDirectory: ptr::null_mut(),
                    FileNameLength: name_bytes,
                    FileName: [0],
                },
            );
            ptr::copy_nonoverlapping(
                name.as_ptr(),
                ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
                name.len(),
            );
            SetFileInformationByHandle(
                staging.as_raw_handle().cast(),
                FileRenameInfo,
                info.cast(),
                buffer_bytes,
            )
        };

        if succeeded == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    #[derive(Clone, Copy)]
    enum EntryKind {
        Directory,
        RegularFile,
    }

    fn validate_entry(file: &File, expected: EntryKind) -> io::Result<()> {
        let metadata = file.metadata()?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "reparse entries are not supported",
            ));
        }
        let matches = match expected {
            EntryKind::Directory => metadata.is_dir(),
            EntryKind::RegularFile => metadata.is_file(),
        };
        if matches {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "filesystem entry has an unsupported type",
            ))
        }
    }

    fn validate_local_fixed_ntfs(path: &Path, root: &File) -> io::Result<()> {
        let mut filesystem = [0_u16; 32];
        // SAFETY: all pointers refer to writable stack buffers or valid handles;
        // unused optional output pointers are null as documented.
        let volume_ok = unsafe {
            GetVolumeInformationByHandleW(
                root.as_raw_handle().cast(),
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                filesystem.as_mut_ptr(),
                filesystem.len() as u32,
            )
        };
        if volume_ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let filesystem_end = filesystem
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(filesystem.len());
        if String::from_utf16_lossy(&filesystem[..filesystem_end]) != "NTFS" {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Windows runtime-content storage requires NTFS",
            ));
        }

        let path = wide_nul(path.as_os_str());
        let mut volume_path = vec![0_u16; 32_768];
        // SAFETY: `path` is NUL-terminated and `volume_path` is writable for the
        // supplied length.
        let path_ok = unsafe {
            GetVolumePathNameW(
                path.as_ptr(),
                volume_path.as_mut_ptr(),
                volume_path.len() as u32,
            )
        };
        if path_ok == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: GetVolumePathNameW produced a NUL-terminated root path in the
        // supplied buffer.
        if unsafe { GetDriveTypeW(volume_path.as_ptr()) } != DRIVE_FIXED {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Windows runtime-content storage requires a local fixed volume",
            ));
        }
        Ok(())
    }

    fn open_relative_component(
        root: &File,
        segment: &str,
        final_component: bool,
    ) -> io::Result<File> {
        let mut name = segment.encode_utf16().collect::<Vec<_>>();
        let name_bytes = name
            .len()
            .checked_mul(size_of::<u16>())
            .and_then(|length| u16::try_from(length).ok())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "source name is too long")
            })?;
        let maximum_length = u16::try_from(name_bytes as usize + size_of::<u16>())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source name is too long"))?;
        let unicode = UNICODE_STRING {
            Length: name_bytes,
            MaximumLength: maximum_length,
            Buffer: name.as_mut_ptr(),
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>())
                .expect("OBJECT_ATTRIBUTES size fits u32"),
            RootDirectory: root.as_raw_handle().cast(),
            ObjectName: &unicode,
            Attributes: 0,
            SecurityDescriptor: ptr::null_mut(),
            SecurityQualityOfService: ptr::null_mut(),
        };
        let mut handle: HANDLE = ptr::null_mut();
        let mut status_block = IO_STATUS_BLOCK::default();
        let kind = if final_component {
            FILE_NON_DIRECTORY_FILE
        } else {
            FILE_DIRECTORY_FILE
        };
        // SAFETY: every pointer references initialized stack storage for the
        // duration of the synchronous call. `root` owns a valid directory
        // handle, and ownership of a successful returned handle is immediately
        // transferred to `File`.
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                GENERIC_READ | SYNCHRONIZE,
                &attributes,
                &mut status_block,
                ptr::null(),
                FILE_ATTRIBUTE_NORMAL,
                SHARE_ALL,
                FILE_OPEN,
                kind | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
                ptr::null(),
                0,
            )
        };
        if status < 0 {
            // SAFETY: conversion is a pure status-code mapping.
            let error = unsafe { RtlNtStatusToDosError(status) };
            Err(io::Error::from_raw_os_error(error as i32))
        } else {
            // SAFETY: successful NtCreateFile returned a newly owned handle.
            Ok(unsafe { File::from_raw_handle(handle.cast()) })
        }
    }

    fn validate_exact_opened_name(file: &File, requested: &str) -> io::Result<()> {
        let flags = FILE_NAME_NORMALIZED | VOLUME_NAME_DOS;
        // SAFETY: the handle is valid and the null buffer is used only for the
        // documented size query.
        let required = unsafe {
            GetFinalPathNameByHandleW(file.as_raw_handle().cast(), ptr::null_mut(), 0, flags)
        };
        if required == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut path = vec![0_u16; required as usize];
        // SAFETY: `path` is writable for the exact supplied capacity.
        let written = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle().cast(),
                path.as_mut_ptr(),
                required,
                flags,
            )
        };
        if written == 0 || written >= required {
            return Err(io::Error::last_os_error());
        }
        path.truncate(written as usize);
        let actual = path
            .rsplit(|unit| *unit == b'\\' as u16 || *unit == b'/' as u16)
            .next()
            .unwrap_or(&path);
        let requested = requested.encode_utf16().collect::<Vec<_>>();
        if actual == requested {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "source lookup did not resolve the exact requested long entry name",
            ))
        }
    }

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().collect()
    }

    fn wide_nul(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain([0]).collect()
    }
}

#[cfg(windows)]
pub use implementation::*;

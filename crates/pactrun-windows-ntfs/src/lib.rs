//! Narrow safe wrappers for the Win32 calls required by Pactrun's NTFS store.
//!
//! This crate is an internal implementation boundary. It is not a Pactrun API.

#[cfg(windows)]
mod process;
#[cfg(windows)]
pub use process::{
    HookProcess, HookTerminal, allocate_console, generate_console_ctrl_c, ignore_console_ctrl_c,
    run_exact_in_current_job,
};

#[cfg(windows)]
mod implementation {

    use std::{
        ffi::OsStr,
        fmt,
        fs::{File, OpenOptions},
        io,
        mem::size_of,
        os::windows::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
            process::ExitStatusExt,
        },
        path::Path,
        process::ExitStatus,
        ptr,
    };

    use windows_sys::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
                FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT, FILE_WRITE_THROUGH,
                NtCreateFile,
            },
        },
        Win32::{
            Foundation::{
                ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, GENERIC_READ,
                GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, LocalFree, RtlNtStatusToDosError,
                UNICODE_STRING,
            },
            Security::{
                Authorization::{
                    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                    SDDL_REVISION_1,
                },
                GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_GROUPS,
                TOKEN_INFORMATION_CLASS, TOKEN_QUERY, TOKEN_USER, TokenRestrictedSids, TokenUser,
            },
            Storage::FileSystem::{
                BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ATTRIBUTE_DIRECTORY,
                FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
                FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_OVERLAPPED,
                FILE_FLAG_WRITE_THROUGH, FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES,
                FILE_RENAME_INFO, FILE_RENAME_INFO_0, FILE_SHARE_DELETE, FILE_SHARE_READ,
                FILE_SHARE_WRITE, FileRenameInfo, FlushFileBuffers, GetDriveTypeW,
                GetFileInformationByHandle, GetFinalPathNameByHandleW,
                GetVolumeInformationByHandleW, GetVolumePathNameW, PIPE_ACCESS_DUPLEX, ReadFile,
                SYNCHRONIZE, SetFileInformationByHandle, VOLUME_NAME_DOS, WriteFile,
            },
            System::WindowsProgramming::DRIVE_FIXED,
            System::{
                IO::{CancelIoEx, GetOverlappedResult, IO_STATUS_BLOCK, OVERLAPPED},
                Pipes::{
                    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE,
                    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
                },
                Threading::{CreateEventW, GetCurrentProcess, OpenProcessToken},
            },
        },
    };

    use windows_sys::core::BOOL;

    const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

    /// One owner-restricted server instance of a named pipe that accepts
    /// exactly one client.
    ///
    /// The instance is created for overlapped I/O: a synchronous pipe handle
    /// serialises every operation on its file object, so a read blocked on
    /// one duplicate would also block a concurrent write on another.
    pub struct NamedPipeListener {
        endpoint: String,
        pipe: Option<OwnedHandle>,
        pending: Option<PendingConnect>,
    }

    struct PendingConnect {
        overlapped: Box<OVERLAPPED>,
        _event: OwnedHandle,
    }

    // SAFETY: the OVERLAPPED block is heap-owned by this value and `hEvent`
    // is an owned kernel handle; both are usable from whichever thread owns
    // the listener, and the block outlives the operation (see `Drop`).
    unsafe impl Send for PendingConnect {}

    impl fmt::Debug for NamedPipeListener {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("NamedPipeListener")
        }
    }

    impl NamedPipeListener {
        pub fn bind(endpoint: &str) -> io::Result<Self> {
            let wide_endpoint = wide_nul(OsStr::new(endpoint));
            let descriptor_text = owner_pipe_descriptor()?;
            let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
            if unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    descriptor_text.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    ptr::null_mut(),
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            let attributes = SECURITY_ATTRIBUTES {
                nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
                    .expect("SECURITY_ATTRIBUTES size fits u32"),
                lpSecurityDescriptor: descriptor,
                bInheritHandle: 0,
            };
            let handle = unsafe {
                CreateNamedPipeW(
                    wide_endpoint.as_ptr(),
                    PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    64 * 1024,
                    64 * 1024,
                    0,
                    &attributes,
                )
            };
            unsafe {
                LocalFree(descriptor.cast());
            }
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            Ok(Self {
                endpoint: endpoint.to_owned(),
                pipe: Some(unsafe { OwnedHandle::from_raw_handle(handle.cast()) }),
                pending: None,
            })
        }

        pub fn endpoint(&self) -> &str {
            &self.endpoint
        }

        /// Polls for the single client without blocking. The first call
        /// starts an overlapped `ConnectNamedPipe`; later calls observe it.
        pub fn try_accept(&mut self) -> io::Result<Option<NamedPipeStream>> {
            let pipe = self.pipe.as_ref().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "named pipe was already accepted",
                )
            })?;
            let handle: HANDLE = pipe.as_raw_handle().cast();
            let connected = match &self.pending {
                None => {
                    let event = create_event()?;
                    let mut overlapped = Box::new(OVERLAPPED {
                        hEvent: event.as_raw_handle().cast(),
                        ..Default::default()
                    });
                    if unsafe { ConnectNamedPipe(handle, &mut *overlapped) } != 0 {
                        true
                    } else {
                        let error = io::Error::last_os_error();
                        match error.raw_os_error().map(|code| code as u32) {
                            Some(ERROR_PIPE_CONNECTED) => true,
                            Some(ERROR_IO_PENDING) => {
                                self.pending = Some(PendingConnect {
                                    overlapped,
                                    _event: event,
                                });
                                false
                            }
                            _ => return Err(error),
                        }
                    }
                }
                Some(pending) => {
                    let mut transferred = 0_u32;
                    if unsafe {
                        GetOverlappedResult(handle, &*pending.overlapped, &mut transferred, 0)
                    } != 0
                    {
                        true
                    } else {
                        let error = io::Error::last_os_error();
                        match error.raw_os_error().map(|code| code as u32) {
                            Some(ERROR_IO_INCOMPLETE) => false,
                            Some(ERROR_PIPE_CONNECTED) => true,
                            _ => return Err(error),
                        }
                    }
                }
            };
            if !connected {
                return Ok(None);
            }
            self.pending = None;
            Ok(self.pipe.take().map(|handle| NamedPipeStream { handle }))
        }
    }

    fn owner_pipe_descriptor() -> io::Result<Vec<u16>> {
        owner_descriptor(false)
    }

    fn owner_descriptor(inherit: bool) -> io::Result<Vec<u16>> {
        let mut token = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = unsafe { OwnedHandle::from_raw_handle(token.cast()) };
        let user_storage = token_information(&token, TokenUser)?;
        let user = unsafe { &*user_storage.as_ptr().cast::<TOKEN_USER>() };
        let flags = if inherit { "OICI" } else { "" };
        let mut descriptor = format!(
            "D:P(A;{flags};GA;;;SY)(A;{flags};GA;;;{})",
            sid_text(user.User.Sid)?
        );

        // Restricted-token access checks evaluate the DACL twice: once with
        // the ordinary token SIDs and once with the restricting SIDs. Keep the
        // user-specific ACE for the first pass and admit the token's exact
        // restricting SIDs for the second, rather than broadening to Everyone.
        let restricted_storage = token_information(&token, TokenRestrictedSids)?;
        let restricted = unsafe { &*restricted_storage.as_ptr().cast::<TOKEN_GROUPS>() };
        for index in 0..restricted.GroupCount as usize {
            let group = unsafe { &*restricted.Groups.as_ptr().add(index) };
            descriptor.push_str(&format!("(A;{flags};GA;;;"));
            descriptor.push_str(&sid_text(group.Sid)?);
            descriptor.push(')');
        }
        Ok(wide_nul(OsStr::new(&descriptor)))
    }

    fn token_information(
        token: &OwnedHandle,
        class: TOKEN_INFORMATION_CLASS,
    ) -> io::Result<Vec<usize>> {
        let mut bytes = 0;
        unsafe {
            GetTokenInformation(
                token.as_raw_handle().cast(),
                class,
                ptr::null_mut(),
                0,
                &mut bytes,
            );
        }
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut storage = vec![0_usize; (bytes as usize).div_ceil(size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle().cast(),
                class,
                storage.as_mut_ptr().cast(),
                bytes,
                &mut bytes,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(storage)
    }

    fn sid_text(sid: windows_sys::Win32::Security::PSID) -> io::Result<String> {
        let mut string_sid = ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(sid, &mut string_sid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut length = 0;
        while unsafe { *string_sid.add(length) } != 0 {
            length += 1;
        }
        let sid =
            String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(string_sid, length) });
        unsafe {
            LocalFree(string_sid.cast());
        }
        Ok(sid)
    }

    impl Drop for NamedPipeListener {
        fn drop(&mut self) {
            // A pending connect must complete before its OVERLAPPED is freed.
            if let (Some(pipe), Some(pending)) = (&self.pipe, &self.pending) {
                let handle: HANDLE = pipe.as_raw_handle().cast();
                let mut transferred = 0_u32;
                unsafe {
                    CancelIoEx(handle, &*pending.overlapped);
                    GetOverlappedResult(handle, &*pending.overlapped, &mut transferred, 1);
                }
            }
        }
    }

    /// The server end of one accepted named-pipe connection.
    ///
    /// Every read and write is an overlapped operation with its own event,
    /// so a duplicate blocked in a read never serialises a concurrent write.
    pub struct NamedPipeStream {
        handle: OwnedHandle,
    }

    impl fmt::Debug for NamedPipeStream {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("NamedPipeStream")
        }
    }

    impl NamedPipeStream {
        pub fn try_clone(&self) -> io::Result<Self> {
            Ok(Self {
                handle: self.handle.try_clone()?,
            })
        }

        fn overlapped_io(
            &self,
            operation: impl FnOnce(HANDLE, *mut OVERLAPPED) -> BOOL,
        ) -> io::Result<usize> {
            let event = create_event()?;
            let mut overlapped = OVERLAPPED {
                hEvent: event.as_raw_handle().cast(),
                ..Default::default()
            };
            let handle: HANDLE = self.handle.as_raw_handle().cast();
            if operation(handle, &mut overlapped) == 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error().map(|code| code as u32) != Some(ERROR_IO_PENDING) {
                    return Err(error);
                }
            }
            let mut transferred = 0_u32;
            if unsafe { GetOverlappedResult(handle, &overlapped, &mut transferred, 1) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(transferred as usize)
        }
    }

    impl io::Read for NamedPipeStream {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if buffer.is_empty() {
                return Ok(0);
            }
            let length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
            let pointer = buffer.as_mut_ptr();
            self.overlapped_io(|handle, overlapped| unsafe {
                ReadFile(handle, pointer, length, ptr::null_mut(), overlapped)
            })
        }
    }

    impl io::Write for NamedPipeStream {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if buffer.is_empty() {
                return Ok(0);
            }
            let length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
            let pointer = buffer.as_ptr();
            self.overlapped_io(|handle, overlapped| unsafe {
                WriteFile(handle, pointer, length, ptr::null_mut(), overlapped)
            })
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn create_event() -> io::Result<OwnedHandle> {
        let handle = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { OwnedHandle::from_raw_handle(handle.cast()) })
    }

    pub fn exit_status_from_code(code: u32) -> ExitStatus {
        ExitStatus::from_raw(code)
    }

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

    /// Creates one new private directory relative to an already qualified root.
    /// The caller must persist allocation intent before invoking this primitive.
    /// FILE_CREATE is no-replace; FILE_WRITE_THROUGH and the explicit flush are
    /// required rather than treating an ordinary mkdir return as publication.
    pub fn create_service_directory(root: &File, segment: &str) -> io::Result<File> {
        validate_service_segment(segment)?;
        let mut name = wide_nul(OsStr::new(segment));
        let length = u16::try_from((name.len() - 1) * size_of::<u16>())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "directory name too long"))?;
        let maximum_length = u16::try_from(name.len() * size_of::<u16>())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "directory name too long"))?;
        let unicode = UNICODE_STRING {
            Length: length,
            MaximumLength: maximum_length,
            Buffer: name.as_mut_ptr(),
        };
        let descriptor_text = owner_descriptor(true)?;
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: descriptor_text is NUL-terminated; the API returns a LocalFree
        // allocation, released after the synchronous NtCreateFile call below.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                descriptor_text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let attributes = OBJECT_ATTRIBUTES {
            Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>())
                .expect("object attributes fit u32"),
            RootDirectory: root.as_raw_handle().cast(),
            ObjectName: &unicode,
            Attributes: 0,
            SecurityDescriptor: descriptor.cast(),
            SecurityQualityOfService: ptr::null_mut(),
        };
        let mut handle: HANDLE = ptr::null_mut();
        let mut status_block = IO_STATUS_BLOCK::default();
        // SAFETY: the counted name and security descriptor stay alive for the
        // synchronous call; root owns its handle. A successful returned handle
        // is immediately transferred to File, including all later error paths.
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                GENERIC_READ | GENERIC_WRITE | DELETE | SYNCHRONIZE,
                &attributes,
                &mut status_block,
                ptr::null(),
                FILE_ATTRIBUTE_DIRECTORY,
                SHARE_ALL,
                FILE_CREATE,
                FILE_DIRECTORY_FILE
                    | FILE_WRITE_THROUGH
                    | FILE_SYNCHRONOUS_IO_NONALERT
                    | FILE_OPEN_REPARSE_POINT,
                ptr::null(),
                0,
            )
        };
        unsafe {
            LocalFree(descriptor.cast());
        }
        if status < 0 {
            return Err(io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(status) } as i32,
            ));
        }
        let file = unsafe { File::from_raw_handle(handle.cast()) };
        validate_entry(&file, EntryKind::Directory)?;
        validate_exact_opened_name(&file, segment)?;
        flush(&file)?;
        Ok(file)
    }

    pub fn open_service_directory(root: &File, segment: &str) -> io::Result<File> {
        validate_service_segment(segment)?;
        let file = open_relative_component(root, segment, false)?;
        validate_entry(&file, EntryKind::Directory)?;
        validate_exact_opened_name(&file, segment)?;
        Ok(file)
    }

    /// Compare opened objects, not path spellings or mutable directory metadata.
    pub fn same_opened_object(left: &File, right: &File) -> io::Result<bool> {
        fn identity(file: &File) -> io::Result<(u32, u32, u32)> {
            let mut info = BY_HANDLE_FILE_INFORMATION::default();
            // SAFETY: File owns a live handle; info is writable for the complete
            // output structure and is read only after the API reports success.
            if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok((
                info.dwVolumeSerialNumber,
                info.nFileIndexHigh,
                info.nFileIndexLow,
            ))
        }
        Ok(identity(left)? == identity(right)?)
    }

    /// Metadata-only, no-follow acquisition of a service leaf for observation.
    pub fn open_service_entry(root: &File, segment: &str) -> io::Result<(File, u32)> {
        validate_service_segment(segment)?;
        let file = open_relative_with(root, segment, 0, FILE_READ_ATTRIBUTES | SYNCHRONIZE)?;
        if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "service entry is a reparse point",
            ));
        }
        validate_exact_opened_name(&file, segment)?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: File owns a live handle and info is the full writable output.
        // No service content is read by this metadata query.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((file, info.nNumberOfLinks))
    }

    fn validate_service_segment(segment: &str) -> io::Result<()> {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.len() > 128
            || !segment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected one service directory component",
            ));
        }
        Ok(())
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
        open_relative_with(
            root,
            segment,
            if final_component {
                FILE_NON_DIRECTORY_FILE
            } else {
                FILE_DIRECTORY_FILE
            },
            GENERIC_READ | SYNCHRONIZE,
        )
    }

    fn open_relative_with(root: &File, segment: &str, kind: u32, access: u32) -> io::Result<File> {
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
        // SAFETY: every pointer references initialized stack storage for the
        // duration of the synchronous call. `root` owns a valid directory
        // handle, and ownership of a successful returned handle is immediately
        // transferred to `File`.
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                access,
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

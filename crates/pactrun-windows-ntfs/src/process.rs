//! Exact-path Hook launch. Only the declared terminal handles are inherited,
//! and no Hook instruction runs before its process belongs to the Job Object.

use std::{
    cmp::Ordering,
    env,
    ffi::OsStr,
    fs::OpenOptions,
    io,
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    process::ExitStatus,
    ptr,
};

use windows_sys::Win32::{
    Foundation::{
        DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    },
    Globalization::CompareStringOrdinal,
    Storage::FileSystem::GetBinaryTypeW,
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject,
        },
        Threading::{
            CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess,
            GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList,
            LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION,
            ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
            UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};

#[derive(Clone, Copy, Debug)]
pub enum HookTerminal {
    None,
    Output,
    Interactive,
}

#[derive(Debug)]
pub struct HookProcess {
    process: OwnedHandle,
    // Closing the owner's last job handle kills any remaining descendants.
    job: OwnedHandle,
}

impl HookProcess {
    /// Inherits the parent environment with the two protocol discovery values
    /// replaced. The admitted pathname is passed unchanged as lpApplicationName.
    pub fn spawn(
        program: &Path,
        arguments: &[String],
        terminal: HookTerminal,
        discovery: [(&str, &str); 2],
    ) -> io::Result<Self> {
        let mut application = checked_wide(program.as_os_str())?;
        let mut command_line = command_line(&application, arguments)?;
        application.push(0);
        // CreateProcessW itself can redirect non-image batch files to cmd.exe.
        // This Executor-only check enforces the existing no-implicit-shell
        // rule; a PE named .cmd/.bat still reaches the exact native launch.
        // It is not an Admission eligibility or replacement-detection check.
        if batch_path(program) {
            let mut binary_type = 0;
            if unsafe { GetBinaryTypeW(application.as_ptr(), &mut binary_type) } == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        let environment = environment_block(env::vars_os(), discovery)?;
        let standard_handles = terminal_handles(terminal)?;
        let inherited: Vec<HANDLE> = standard_handles
            .iter()
            .flatten()
            .map(|handle| handle.as_raw_handle().cast())
            .collect();
        let attributes = if inherited.is_empty() {
            None
        } else {
            Some(HandleList::new(&inherited)?)
        };
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        let raw = |index: usize| {
            standard_handles[index]
                .as_ref()
                .map_or(ptr::null_mut(), |handle| handle.as_raw_handle().cast())
        };
        startup.StartupInfo.hStdInput = raw(0);
        startup.StartupInfo.hStdOutput = raw(1);
        startup.StartupInfo.hStdError = raw(2);
        startup.lpAttributeList = attributes
            .as_ref()
            .map_or(ptr::null_mut(), HandleList::as_ptr);
        let job = create_job()?;
        let mut information = PROCESS_INFORMATION::default();
        // SAFETY: every buffer, standard handle, and attribute allocation is
        // owned here and lives through CreateProcessW. Only the explicit list
        // is inheritable by this child. Both output handles are adopted below.
        if unsafe {
            CreateProcessW(
                application.as_ptr(),
                command_line.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                i32::from(!inherited.is_empty()),
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                environment.as_ptr().cast(),
                ptr::null(),
                &startup.StartupInfo,
                &mut information,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful CreateProcessW returns two uniquely owned handles.
        let process = unsafe { OwnedHandle::from_raw_handle(information.hProcess.cast()) };
        let thread = unsafe { OwnedHandle::from_raw_handle(information.hThread.cast()) };
        let child = Self { process, job };
        let process_handle = child.process.as_raw_handle().cast();
        // The primary thread is still suspended, including during assignment.
        let assigned =
            unsafe { AssignProcessToJobObject(child.job.as_raw_handle().cast(), process_handle) }
                != 0;
        if !assigned || unsafe { ResumeThread(thread.as_raw_handle().cast()) } == u32::MAX {
            let error = io::Error::last_os_error();
            // The Hook never ran. Terminate and reap the suspended process even
            // when assignment failed and the job cannot clean it up for us.
            unsafe {
                TerminateProcess(process_handle, 1);
                WaitForSingleObject(process_handle, INFINITE);
            }
            return Err(error);
        }
        Ok(child)
    }

    pub fn try_wait(&self) -> io::Result<Option<ExitStatus>> {
        self.wait_for(0)
    }

    pub fn wait(&self) -> io::Result<ExitStatus> {
        self.wait_for(INFINITE)?
            .ok_or_else(|| io::Error::other("infinite process wait timed out"))
    }

    fn wait_for(&self, timeout: u32) -> io::Result<Option<ExitStatus>> {
        let process = self.process.as_raw_handle().cast();
        // SAFETY: the owned process handle remains open throughout the wait.
        match unsafe { WaitForSingleObject(process, timeout) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(process, &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                // Test the wait result, not STILL_ACTIVE: 259 is a legal exit code.
                Ok(Some(crate::exit_status_from_code(code)))
            }
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub fn terminate_tree(&self) -> io::Result<()> {
        // SAFETY: this job is uniquely owned and contains only this Hook tree.
        if unsafe { TerminateJobObject(self.job.as_raw_handle().cast(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

fn create_job() -> io::Result<OwnedHandle> {
    let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(handle.cast()) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            handle.as_raw_handle().cast(),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(handle)
}

fn terminal_handles(terminal: HookTerminal) -> io::Result<[Option<OwnedHandle>; 3]> {
    let inherited = [
        matches!(terminal, HookTerminal::Interactive),
        !matches!(terminal, HookTerminal::None),
        !matches!(terminal, HookTerminal::None),
    ];
    let parent = [
        io::stdin().as_raw_handle(),
        io::stdout().as_raw_handle(),
        io::stderr().as_raw_handle(),
    ];
    let mut handles = [None, None, None];
    for index in 0..3 {
        let null;
        let source = if inherited[index] {
            let handle: HANDLE = parent[index].cast();
            // Preserve std's absent-standard-handle behavior.
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                continue;
            }
            handle
        } else {
            null = OpenOptions::new()
                .read(index == 0)
                .write(index != 0)
                .open(r"\\.\NUL")?;
            null.as_raw_handle().cast()
        };
        let mut duplicate = ptr::null_mut();
        // SAFETY: duplicate the live borrowed source without changing its
        // inheritance flags. Only this short-lived duplicate is inheritable.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                source,
                GetCurrentProcess(),
                &mut duplicate,
                0,
                1,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        handles[index] = Some(unsafe { OwnedHandle::from_raw_handle(duplicate.cast()) });
    }
    Ok(handles)
}

struct HandleList {
    // usize provides pointer alignment for the opaque Win32 attribute storage.
    storage: Vec<usize>,
}

impl HandleList {
    fn as_ptr(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_ptr().cast_mut().cast()
    }

    fn new(handles: &[HANDLE]) -> io::Result<Self> {
        let mut bytes = 0;
        unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut bytes) };
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut storage = vec![0_usize; bytes.div_ceil(size_of::<usize>())];
        if unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 1, 0, &mut bytes)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut list = Self { storage };
        // SAFETY: the initialized list is aligned and large enough; the caller
        // keeps the handle slice and all handles alive through process creation.
        if unsafe {
            UpdateProcThreadAttribute(
                list.storage.as_mut_ptr().cast(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                size_of_val(handles),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(list)
    }
}

impl Drop for HandleList {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.storage.as_mut_ptr().cast()) };
    }
}

fn checked_wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let wide: Vec<_> = value.encode_wide().collect();
    if wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "embedded NUL in process input",
        ));
    }
    Ok(wide)
}

fn batch_path(program: &Path) -> bool {
    let mut name: Vec<_> = program.as_os_str().encode_wide().collect();
    // Win32 paths can carry trailing dots/spaces that normal pathname lookup
    // removes. Inspect that spelling without rewriting the admitted path.
    while matches!(name.last(), Some(32 | 46)) {
        name.pop();
    }
    matches!(
        name.len().checked_sub(4).map(|index| &name[index..]),
        Some([46, 98 | 66, 97 | 65, 116 | 84] | [46, 99 | 67, 109 | 77, 100 | 68])
    )
}

fn command_line(program: &[u16], arguments: &[String]) -> io::Result<Vec<u16>> {
    let quote = u16::from(b'"');
    let slash = u16::from(b'\\');
    let mut line = vec![quote];
    line.extend_from_slice(program);
    line.push(quote);
    for argument in arguments {
        let argument = checked_wide(OsStr::new(argument))?;
        let quoted = argument.is_empty() || argument.iter().any(|c| *c == 32 || *c == 9);
        line.push(u16::from(b' '));
        if quoted {
            line.push(quote);
        }
        let mut slashes = 0;
        for c in argument {
            if c == slash {
                slashes += 1;
            } else {
                // CRT parsing halves slashes before a quote; an odd extra
                // slash protects a literal quote instead of ending the argument.
                let count = if c == quote { 2 * slashes + 1 } else { slashes };
                line.extend(std::iter::repeat_n(slash, count));
                slashes = 0;
                line.push(c);
            }
        }
        line.extend(std::iter::repeat_n(
            slash,
            slashes * if quoted { 2 } else { 1 },
        ));
        if quoted {
            line.push(quote);
        }
    }
    line.push(0);
    Ok(line)
}

fn compare_keys(left: &[u16], right: &[u16]) -> Ordering {
    // Windows environment names are ordered case-insensitively using the OS
    // ordinal mapping, not Rust Unicode case conversion or locale collation.
    let order = unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            left.len() as i32,
            right.as_ptr(),
            right.len() as i32,
            1,
        )
    };
    match order {
        1 => Ordering::Less,
        2 => Ordering::Equal,
        3 => Ordering::Greater,
        _ => unreachable!("valid environment names fit CompareStringOrdinal"),
    }
}

fn environment_block(
    parent: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
    discovery: [(&str, &str); 2],
) -> io::Result<Vec<u16>> {
    let mut entries = parent
        .into_iter()
        .map(|(key, value)| Ok((checked_wide(&key)?, checked_wide(&value)?)))
        .collect::<io::Result<Vec<_>>>()?;
    for (key, value) in discovery {
        let key = checked_wide(OsStr::new(key))?;
        entries.retain(|(existing, _)| compare_keys(existing, &key) != Ordering::Equal);
        entries.push((key, checked_wide(OsStr::new(value))?));
    }
    entries.sort_by(|(left, _), (right, _)| compare_keys(left, right));
    let mut block = Vec::new();
    for (key, value) in entries {
        block.extend(key);
        block.push(u16::from(b'='));
        block.extend(value);
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

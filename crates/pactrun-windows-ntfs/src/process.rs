//! Exact-path Hook launch. Only the declared terminal handles are inherited,
//! and no Hook instruction runs before its process belongs to the Job Object.

use std::{
    cmp::Ordering,
    env,
    ffi::OsStr,
    fs::{File, OpenOptions},
    io::{self, Read},
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
        Console::{
            AllocConsole, CTRL_C_EVENT, GenerateConsoleCtrlEvent, GetConsoleCP, GetConsoleMode,
            GetConsoleWindow, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
            SetConsoleCtrlHandler,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Pipes::{CreatePipe, PeekNamedPipe},
        Threading::{
            CREATE_NEW_PROCESS_GROUP, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
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

pub fn capture_pipe() -> io::Result<(File, File)> {
    let mut read = ptr::null_mut();
    let mut write = ptr::null_mut();
    // No inheritable handles are created here. Process creation duplicates only
    // the write ends into its explicit inheritance list.
    if unsafe { CreatePipe(&mut read, &mut write, ptr::null(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe {
        (
            File::from_raw_handle(read.cast()),
            File::from_raw_handle(write.cast()),
        )
    })
}
pub fn read_capture_pipe(file: &mut File, bytes: &mut [u8]) -> io::Result<usize> {
    let mut available = 0;
    if unsafe {
        PeekNamedPipe(
            file.as_raw_handle().cast(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            &mut available,
            ptr::null_mut(),
        )
    } == 0
    {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(109) {
            return Ok(0);
        }
        return Err(error);
    }
    if available == 0 {
        return Err(io::ErrorKind::WouldBlock.into());
    }
    let take = bytes.len().min(available as usize);
    file.read(&mut bytes[..take])
}

/// Allocates a console for an isolated integration-test driver.
pub fn allocate_console() -> io::Result<()> {
    let has_console_handles = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE]
        .into_iter()
        .any(|which| {
            let handle = unsafe { GetStdHandle(which) };
            let mut mode = 0;
            !handle.is_null() && unsafe { GetConsoleMode(handle, &mut mode) } != 0
        });
    if has_console_handles
        || unsafe { GetConsoleCP() } != 0
        || !unsafe { GetConsoleWindow() }.is_null()
    {
        return Ok(());
    }
    if unsafe { AllocConsole() } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Sends the same processed CTRL+C console event used by the console input
/// driver to the current console. The target group is zero so the parent
/// Pactrun process receives the event; Hooks launched in a new process group
/// keep their interactive handles without bypassing the owner handler.
pub fn generate_console_ctrl_c() -> io::Result<()> {
    if unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Makes the current helper and its descendants ignore CTRL+C. Pactrun keeps
/// the authoritative handler in the owner process; the interactive adapter
/// calls this before starting the real Hook so a console event cannot make the
/// supervised child escape through the default console handler.
pub fn ignore_console_ctrl_c() -> io::Result<()> {
    if unsafe { SetConsoleCtrlHandler(None, 1) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub struct HookProcess {
    process: OwnedHandle,
    // Closing the owner's last job handle kills any remaining descendants.
    job: OwnedHandle,
}

impl HookProcess {
    pub fn spawn_captured<S: AsRef<OsStr>, const N: usize>(
        program: &Path,
        arguments: &[S],
        discovery: [(&str, &str); N],
        stdout: &File,
        stderr: &File,
    ) -> io::Result<Self> {
        Self::spawn_inner_captured(
            program,
            arguments,
            HookTerminal::Output,
            discovery,
            None,
            Some((stdout, stderr)),
        )
    }
    /// Inherits the parent environment with the two protocol discovery values
    /// replaced. The admitted pathname is passed unchanged as lpApplicationName.
    pub fn spawn<S: AsRef<OsStr>, const N: usize>(
        program: &Path,
        arguments: &[S],
        terminal: HookTerminal,
        discovery: [(&str, &str); N],
    ) -> io::Result<Self> {
        Self::spawn_inner(program, arguments, terminal, discovery, None)
    }

    /// Test-only environment extension for host-native integration fixtures.
    /// It uses the same process-creation and Job Object path as `spawn`.
    #[doc(hidden)]
    pub fn spawn_with_extra_environment<S: AsRef<OsStr>, const N: usize>(
        program: &Path,
        arguments: &[S],
        terminal: HookTerminal,
        discovery: [(&str, &str); N],
        extra: (&str, &OsStr),
    ) -> io::Result<Self> {
        Self::spawn_inner(program, arguments, terminal, discovery, Some(extra))
    }

    fn spawn_inner<S: AsRef<OsStr>, const N: usize>(
        program: &Path,
        arguments: &[S],
        terminal: HookTerminal,
        discovery: [(&str, &str); N],
        extra: Option<(&str, &OsStr)>,
    ) -> io::Result<Self> {
        Self::spawn_inner_captured(program, arguments, terminal, discovery, extra, None)
    }
    fn spawn_inner_captured<S: AsRef<OsStr>, const N: usize>(
        program: &Path,
        arguments: &[S],
        terminal: HookTerminal,
        discovery: [(&str, &str); N],
        extra: Option<(&str, &OsStr)>,
        capture: Option<(&File, &File)>,
    ) -> io::Result<Self> {
        let job = create_job()?;
        let created = create_native_process_captured(
            program,
            arguments,
            terminal,
            &discovery,
            program.as_os_str(),
            extra,
            capture,
        )?;
        let CreatedProcess { process, thread } = created;
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
        wait_for_process(&self.process, timeout)
    }

    pub fn terminate_tree(&self) -> io::Result<()> {
        // SAFETY: this job is uniquely owned and contains only this Hook tree.
        if unsafe { TerminateJobObject(self.job.as_raw_handle().cast(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn tree_terminated(&self) -> io::Result<bool> {
        let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: the Job handle is owned; the output buffer has the exact
        // documented structure size and remains valid throughout the query.
        if unsafe {
            QueryInformationJobObject(
                self.job.as_raw_handle().cast(),
                JobObjectBasicAccountingInformation,
                (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(accounting.ActiveProcesses == 0)
    }
}

struct CreatedProcess {
    process: OwnedHandle,
    thread: OwnedHandle,
}

/// Launches an exact native image from an interactive adapter without making
/// another Job Object. The adapter is already a member of the owner's Job, so
/// Windows places this suspended child and its descendants in that same Job
/// before its primary thread is resumed.
#[doc(hidden)]
pub fn run_exact_in_current_job<S: AsRef<OsStr>>(
    program: &Path,
    arguments: &[S],
    terminal: HookTerminal,
) -> io::Result<ExitStatus> {
    let CreatedProcess { process, thread } = create_native_process(
        program,
        arguments,
        terminal,
        &[],
        OsStr::new("pactrun-hook"),
        None,
    )?;
    let process_handle = process.as_raw_handle().cast();
    if unsafe { ResumeThread(thread.as_raw_handle().cast()) } == u32::MAX {
        let error = io::Error::last_os_error();
        // The Hook never ran. Terminate and reap the suspended process even
        // though its containment is owned by the adapter's existing Job.
        unsafe {
            TerminateProcess(process_handle, 1);
            WaitForSingleObject(process_handle, INFINITE);
        }
        return Err(error);
    }
    wait_for_process(&process, INFINITE)?
        .ok_or_else(|| io::Error::other("infinite process wait timed out"))
}

fn create_native_process<S: AsRef<OsStr>>(
    program: &Path,
    arguments: &[S],
    terminal: HookTerminal,
    discovery: &[(&str, &str)],
    argv0: &OsStr,
    extra: Option<(&str, &OsStr)>,
) -> io::Result<CreatedProcess> {
    create_native_process_captured(program, arguments, terminal, discovery, argv0, extra, None)
}
fn create_native_process_captured<S: AsRef<OsStr>>(
    program: &Path,
    arguments: &[S],
    terminal: HookTerminal,
    discovery: &[(&str, &str)],
    argv0: &OsStr,
    extra: Option<(&str, &OsStr)>,
    capture: Option<(&File, &File)>,
) -> io::Result<CreatedProcess> {
    let mut application = checked_wide(program.as_os_str())?;
    // An exact lpApplicationName launch is not shell redirection. This
    // Executor-side guard preserves the no-implicit-shell policy for
    // batch-suffixed non-images; a PE named .cmd/.bat still reaches the exact
    // native launch. It is not an Admission eligibility or replacement-
    // detection check.
    application.push(0);
    if batch_path(program) {
        let mut binary_type = 0;
        if unsafe { GetBinaryTypeW(application.as_ptr(), &mut binary_type) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let argv0 = checked_wide(argv0)?;
    let mut command_line = command_line(&argv0, arguments)?;
    let environment = environment_block(env::vars_os(), discovery, extra)?;
    let standard_handles = terminal_handles(terminal, capture)?;
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
            CREATE_NEW_PROCESS_GROUP
                | CREATE_SUSPENDED
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
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
    Ok(CreatedProcess {
        process: unsafe { OwnedHandle::from_raw_handle(information.hProcess.cast()) },
        thread: unsafe { OwnedHandle::from_raw_handle(information.hThread.cast()) },
    })
}

fn wait_for_process(process: &OwnedHandle, timeout: u32) -> io::Result<Option<ExitStatus>> {
    let process = process.as_raw_handle().cast();
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

fn terminal_handles(
    terminal: HookTerminal,
    capture: Option<(&File, &File)>,
) -> io::Result<[Option<OwnedHandle>; 3]> {
    let inherited = [
        matches!(terminal, HookTerminal::Interactive),
        !matches!(terminal, HookTerminal::None),
        !matches!(terminal, HookTerminal::None),
    ];
    let mut parent = [
        io::stdin().as_raw_handle(),
        io::stdout().as_raw_handle(),
        io::stderr().as_raw_handle(),
    ];
    if let Some((stdout, stderr)) = capture {
        parent[1] = stdout.as_raw_handle();
        parent[2] = stderr.as_raw_handle();
    }
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

fn command_line<S: AsRef<OsStr>>(program: &[u16], arguments: &[S]) -> io::Result<Vec<u16>> {
    let quote = u16::from(b'"');
    let slash = u16::from(b'\\');
    let mut line = vec![quote];
    line.extend_from_slice(program);
    line.push(quote);
    for argument in arguments {
        let argument = checked_wide(argument.as_ref())?;
        let quoted = argument.is_empty() || argument.iter().any(|c| *c == 32 || *c == 9);
        line.push(u16::from(b' '));
        if quoted {
            line.push(quote);
        }
        let mut slashes = 0;
        for c in argument.iter().copied() {
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
    discovery: &[(&str, &str)],
    extra: Option<(&str, &OsStr)>,
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
    if let Some((key, value)) = extra {
        let key = checked_wide(OsStr::new(key))?;
        entries.retain(|(existing, _)| compare_keys(existing, &key) != Ordering::Equal);
        entries.push((key, checked_wide(value)?));
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

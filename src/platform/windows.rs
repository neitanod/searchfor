//! The native Windows launcher (M5b spec §4.3; plan D3, D4, D20, D22). Every child starts
//! suspended (`CREATE_SUSPENDED`), is assigned to s4's Job Object — kill-on-close, so the system
//! ends it and everything it starts when s4 ends, however it ends — and to a reader's own Job
//! with its memory limit, and only then resumes. The program is resolved here, as std resolves
//! it but never in the current directory, and given to `CreateProcessW` as `lpApplicationName`;
//! a `.bat` or `.cmd` runs through System32's `cmd.exe`, as std runs it. The child inherits only
//! its own three handles (`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`), so concurrent starts never
//! leak each other's pipe ends.
use super::batch::{is_batch_path, make_bat_command_line};
use super::{Child, Input, Limits, Output, PipeReader, PipeWriter, SpawnSpec};
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::OnceLock;
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_ACCESS_DENIED, GENERIC_READ, GENERIC_WRITE,
    GetLastError, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileAttributesW, GetFullPathNameW,
    INVALID_FILE_ATTRIBUTES, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::SystemInformation::{GetSystemDirectoryW, GetWindowsDirectoryW};
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess, GetExitCodeProcess, INFINITE,
    InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION,
    ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject,
};

/// `ResumeThread`'s failure value.
const RESUME_FAILED: u32 = u32::MAX;

fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, what)
}

/// `s` as UTF-16, refused when it holds a NUL (it would end the string early).
fn wide(s: &OsStr) -> io::Result<Vec<u16>> {
    let w: Vec<u16> = s.encode_wide().collect();
    if w.contains(&0) {
        return Err(invalid("nul byte found in provided data"));
    }
    Ok(w)
}

fn nul_terminated(mut w: Vec<u16>) -> Vec<u16> {
    w.push(0);
    w
}

/// Calls a Win32 function that fills a UTF-16 buffer and returns the length it wrote, or the
/// size it needs when the buffer is too small, or 0 on failure.
fn fill_utf16(mut f: impl FnMut(*mut u16, u32) -> u32) -> io::Result<Vec<u16>> {
    let mut buf = vec![0u16; 512];
    loop {
        let n = f(buf.as_mut_ptr(), buf.len() as u32) as usize;
        if n == 0 {
            return Err(io::Error::last_os_error());
        }
        if n < buf.len() {
            buf.truncate(n);
            return Ok(buf);
        }
        buf.resize(n + 1, 0);
    }
}

fn system_directory() -> io::Result<Vec<u16>> {
    // SAFETY: the function writes at most `size` units into `buf`.
    fill_utf16(|buf, size| unsafe { GetSystemDirectoryW(buf, size) })
}

fn windows_directory() -> io::Result<Vec<u16>> {
    // SAFETY: as in `system_directory`.
    fill_utf16(|buf, size| unsafe { GetWindowsDirectoryW(buf, size) })
}

/// The absolute form of `path` (no NUL), as `GetFullPathNameW` gives it.
fn full_path(path: &[u16]) -> io::Result<Vec<u16>> {
    let p = nul_terminated(path.to_vec());
    // SAFETY: `p` is NUL-terminated and the function writes at most `size` units.
    fill_utf16(|buf, size| unsafe { GetFullPathNameW(p.as_ptr(), size, buf, std::ptr::null_mut()) })
}

/// A file or directory exists at `path` (std's `program_exists`: no symlink is followed).
fn exists(path: &Path) -> Option<Vec<u16>> {
    let w = wide(path.as_os_str()).ok()?;
    let z = nul_terminated(w.clone());
    // SAFETY: `z` is NUL-terminated.
    (unsafe { GetFileAttributesW(z.as_ptr()) } != INVALID_FILE_ATTRIBUTES).then_some(w)
}

fn is_sep(c: u16) -> bool {
    c == '/' as u16 || c == '\\' as u16
}

fn ends_with_exe(w: &[u16]) -> bool {
    w.len() >= 4 && String::from_utf16_lossy(&w[w.len() - 4..]).eq_ignore_ascii_case(".exe")
}

/// The program to start, as std's `resolve_exe` finds it, except that it is never searched for
/// in the current directory (plan D20). A name with a path separator is used as given when it
/// ends in `.exe`; otherwise `.exe` is appended and that file is used when it exists, and the
/// name as given when it does not. A bare name is searched in the child's `PATH`, the
/// directory of s4's executable, System32, the Windows directory and s4's `PATH`, with `.exe`
/// appended when the name has no `.`.
fn resolve(program: &OsStr, child_path: Option<&OsStr>) -> io::Result<Vec<u16>> {
    let w = wide(program)?;
    let verbatim = w.starts_with(&wide(OsStr::new(r"\\?\")).unwrap());
    if w.is_empty()
        || w.last().is_some_and(|&c| {
            if verbatim {
                c == '\\' as u16
            } else {
                is_sep(c)
            }
        })
    {
        return Err(invalid("program path has no file name"));
    }
    if w.iter().any(|&c| is_sep(c)) {
        if ends_with_exe(&w) {
            return Ok(w);
        }
        let mut with_exe = program.to_os_string();
        with_exe.push(".exe");
        return Ok(exists(Path::new(&with_exe)).unwrap_or(w));
    }
    let has_extension = w.contains(&('.' as u16));
    let candidate = |dir: &Path| {
        let mut p = dir.join(program);
        if !has_extension {
            p.set_extension("exe");
        }
        exists(&p)
    };
    let in_list = |list: &OsStr| {
        std::env::split_paths(list)
            .filter(|p| !p.as_os_str().is_empty())
            .find_map(|dir| candidate(&dir))
    };
    let found = child_path
        .and_then(in_list)
        .or_else(|| {
            let exe = std::env::current_exe().ok()?;
            candidate(exe.parent()?)
        })
        .or_else(|| {
            candidate(&PathBuf::from(OsString::from_wide(
                &system_directory().ok()?,
            )))
        })
        .or_else(|| {
            candidate(&PathBuf::from(OsString::from_wide(
                &windows_directory().ok()?,
            )))
        })
        .or_else(|| in_list(&std::env::var_os("PATH")?));
    found.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "program not found"))
}

/// One argument quoted as `CommandLineToArgvW` and the C runtime parse it back: always between
/// quotes, with the backslashes before a quote, and before the closing quote, doubled (std's
/// `append_arg` with forced quotes).
fn quote_windows_arg(arg: &[u16]) -> Vec<u16> {
    let mut cmd = vec!['"' as u16];
    let mut backslashes = 0usize;
    for &x in arg {
        if x == '\\' as u16 {
            backslashes += 1;
        } else {
            if x == '"' as u16 {
                // n+1 more backslashes make 2n+1 before an inner quote.
                cmd.extend(std::iter::repeat_n('\\' as u16, backslashes + 1));
            }
            backslashes = 0;
        }
        cmd.push(x);
    }
    // n more backslashes make 2n before the closing quote.
    cmd.extend(std::iter::repeat_n('\\' as u16, backslashes));
    cmd.push('"' as u16);
    cmd
}

/// The command line of an executable: the program as the caller named it, quoted, then each
/// argument (std's `make_command_line`).
fn make_command_line(program: &[u16], args: &[Vec<u16>]) -> Vec<u16> {
    let mut cmd = vec!['"' as u16];
    cmd.extend_from_slice(program);
    cmd.push('"' as u16);
    for a in args {
        cmd.push(' ' as u16);
        cmd.extend(quote_windows_arg(a));
    }
    cmd
}

/// The key Windows compares environment names by: case-insensitive.
fn env_key(k: &OsStr) -> Vec<u16> {
    k.encode_wide()
        .map(|c| {
            if ('a' as u16..='z' as u16).contains(&c) {
                c - 32
            } else {
                c
            }
        })
        .collect()
}

/// The child's environment block, None when it is s4's unchanged; and the child's `PATH`
/// when the spec sets it.
fn environment(spec: &SpawnSpec) -> io::Result<(Option<Vec<u16>>, Option<OsString>)> {
    if spec.env.is_empty() {
        return Ok((None, None));
    }
    let mut vars: std::collections::BTreeMap<Vec<u16>, (OsString, OsString)> = std::env::vars_os()
        .map(|(k, v)| (env_key(&k), (k, v)))
        .collect();
    for (k, v) in &spec.env {
        match v {
            Some(v) => vars.insert(env_key(k), (k.clone(), v.clone())),
            None => vars.remove(&env_key(k)),
        };
    }
    let child_path = spec
        .env
        .iter()
        .any(|(k, _)| env_key(k) == env_key(OsStr::new("PATH")))
        .then(|| {
            vars.get(&env_key(OsStr::new("PATH")))
                .map(|(_, v)| v.clone())
        })
        .flatten();
    // Sorted by name, case-insensitive, as CreateProcessW requires.
    let mut block = Vec::new();
    for (k, v) in vars.values() {
        let k = wide(k)?;
        if k.is_empty() || k[1..].contains(&('=' as u16)) {
            return Err(invalid("environment variable name holds `=`"));
        }
        block.extend(k);
        block.push('=' as u16);
        block.extend(wide(v)?);
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok((Some(block), child_path))
}

/// The child's working directory, without a verbatim prefix CreateProcessW rejects.
fn directory(cwd: &Path) -> io::Result<Vec<u16>> {
    let w = wide(cwd.as_os_str())?;
    let s = String::from_utf16_lossy(&w);
    let stripped = if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        return Ok(nul_terminated(w));
    };
    Ok(nul_terminated(stripped.encode_utf16().collect()))
}

fn security(inherit: bool) -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: inherit as i32,
    }
}

/// SAFETY: `h` is a valid handle s4 owns from now on.
unsafe fn owned(h: HANDLE) -> OwnedHandle {
    unsafe { OwnedHandle::from_raw_handle(h) }
}

/// An inheritable copy of `h`, for the child.
fn inheritable_copy(h: HANDLE) -> io::Result<OwnedHandle> {
    let mut dup: HANDLE = std::ptr::null_mut();
    // SAFETY: duplicates a handle of this process into this process.
    check(unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            h,
            GetCurrentProcess(),
            &mut dup,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    })?;
    Ok(unsafe { owned(dup) })
}

fn null_device(write: bool) -> io::Result<OwnedHandle> {
    let sa = security(true);
    let name: Vec<u16> = "NUL\0".encode_utf16().collect();
    let access = if write { GENERIC_WRITE } else { GENERIC_READ };
    // SAFETY: a NUL-terminated name and a valid SECURITY_ATTRIBUTES.
    let h = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &sa,
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { owned(h) })
}

/// A pipe: (read end, write end), neither inheritable yet.
fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let sa = security(false);
    let (mut r, mut w): (HANDLE, HANDLE) = (std::ptr::null_mut(), std::ptr::null_mut());
    // SAFETY: CreatePipe writes both handles on success.
    check(unsafe { CreatePipe(&mut r, &mut w, &sa, 0) })?;
    Ok(unsafe { (owned(r), owned(w)) })
}

fn make_inheritable(h: &OwnedHandle) -> io::Result<()> {
    // SAFETY: changes a flag of a handle s4 owns.
    check(unsafe {
        SetHandleInformation(h.as_raw_handle(), HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT)
    })
}

/// s4's own standard handle `id`, as an inheritable copy; None when s4 has none.
fn inherited(id: STD_HANDLE) -> io::Result<Option<OwnedHandle>> {
    // SAFETY: GetStdHandle only reads the process's table.
    let h = unsafe { GetStdHandle(id) };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    if h.is_null() {
        return Ok(None);
    }
    inheritable_copy(h).map(Some)
}

/// The child's end of one stream, and s4's end when it is a pipe.
struct Stream {
    child: Option<OwnedHandle>,
}

fn stdin_of(i: Input) -> io::Result<(Stream, Option<PipeWriter>)> {
    Ok(match i {
        Input::Null => (
            Stream {
                child: Some(null_device(false)?),
            },
            None,
        ),
        Input::Inherit => (
            Stream {
                child: inherited(STD_INPUT_HANDLE)?,
            },
            None,
        ),
        Input::File(f) => (
            Stream {
                child: Some(inheritable_copy(f.as_raw_handle())?),
            },
            None,
        ),
        Input::Piped => {
            let (r, w) = pipe()?;
            make_inheritable(&r)?;
            (Stream { child: Some(r) }, Some(PipeWriter(w.into())))
        }
    })
}

fn output_of(o: &Output, id: STD_HANDLE) -> io::Result<(Stream, Option<PipeReader>)> {
    Ok(match o {
        Output::Null => (
            Stream {
                child: Some(null_device(true)?),
            },
            None,
        ),
        Output::Inherit => (
            Stream {
                child: inherited(id)?,
            },
            None,
        ),
        Output::Piped => {
            let (r, w) = pipe()?;
            make_inheritable(&w)?;
            (Stream { child: Some(w) }, Some(PipeReader(r.into())))
        }
    })
}

/// A Job Object: never inheritable (null security attributes), kill-on-close, and with a limit
/// of committed memory per process when asked.
struct Job(OwnedHandle);

impl Job {
    fn new(process_memory: Option<usize>) -> io::Result<Job> {
        // SAFETY: null attributes and name: an unnamed, non-inheritable Job.
        let h = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if h.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Job(unsafe { owned(h) });
        // SAFETY: a zeroed limit structure is valid; the fields set are plain integers.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if let Some(bytes) = process_memory {
            info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY;
            info.ProcessMemoryLimit = bytes;
        }
        // SAFETY: `info` is the structure the information class names, with its size.
        check(unsafe {
            SetInformationJobObject(
                job.handle(),
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        })?;
        Ok(job)
    }

    fn handle(&self) -> HANDLE {
        self.0.as_raw_handle()
    }
}

/// s4's Job: created before the first child starts, and never closed by s4, so it closes when
/// s4 ends, and the system ends every process in it.
fn root_job() -> io::Result<&'static Job> {
    static ROOT: OnceLock<Result<Job, i32>> = OnceLock::new();
    match ROOT.get_or_init(|| Job::new(None).map_err(|e| e.raw_os_error().unwrap_or(0))) {
        Ok(job) => Ok(job),
        Err(code) => Err(io::Error::from_raw_os_error(*code)),
    }
}

fn assign(job: &Job, process: HANDLE) -> io::Result<()> {
    // SAFETY: both handles are valid; the process is suspended.
    check(unsafe { AssignProcessToJobObject(job.handle(), process) })
}

/// The attribute list that limits the handles a child inherits to `handles`; its storage is
/// aligned and freed with the list.
struct HandleList {
    storage: Vec<u64>,
    /// The handles the list points to: they must outlive `CreateProcessW`.
    handles: Box<[HANDLE]>,
}

impl HandleList {
    fn new(handles: Box<[HANDLE]>) -> io::Result<HandleList> {
        let mut size = 0usize;
        // SAFETY: with a null list the call only reports the size it needs.
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size) };
        if size == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut storage = vec![0u64; size.div_ceil(8)];
        // SAFETY: `storage` has `size` bytes, 8-aligned.
        check(unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 1, 0, &mut size)
        })?;
        let list = HandleList { storage, handles };
        // SAFETY: the list was initialized; the handle array lives in `list`.
        check(unsafe {
            UpdateProcThreadAttribute(
                list.ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                list.handles.as_ptr().cast(),
                size_of_val(&*list.handles),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        })?;
        Ok(list)
    }

    fn ptr(&self) -> *mut core::ffi::c_void {
        self.storage.as_ptr().cast_mut().cast()
    }
}

impl Drop for HandleList {
    fn drop(&mut self) {
        // SAFETY: the list was initialized in `new` before `self` existed.
        unsafe { DeleteProcThreadAttributeList(self.ptr()) };
    }
}

/// A process created suspended. Until `into_child` commits it, dropping it terminates the
/// process and waits for it, so no half-started child survives an error.
struct Created {
    pi: PROCESS_INFORMATION,
    reader_job: Option<Job>,
    committed: bool,
}

impl Created {
    fn process(&self) -> HANDLE {
        self.pi.hProcess
    }

    fn thread(&self) -> HANDLE {
        self.pi.hThread
    }

    fn retain_reader_job(&mut self, job: Job) {
        self.reader_job = Some(job);
    }

    /// Closes the thread handle and keeps the process handle and the reader's Job.
    fn into_child(mut self) -> Process {
        self.committed = true;
        // SAFETY: both handles came from CreateProcessW and are closed only here.
        let (process, thread) = unsafe { (owned(self.pi.hProcess), owned(self.pi.hThread)) };
        drop(thread);
        Process {
            handle: process,
            pid: self.pi.dwProcessId,
            _reader_job: self.reader_job.take(),
            status: None,
        }
    }
}

impl Drop for Created {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        // SAFETY: the handles came from CreateProcessW and are closed only here.
        unsafe {
            TerminateProcess(self.pi.hProcess, 1);
            WaitForSingleObject(self.pi.hProcess, INFINITE);
            drop(owned(self.pi.hThread));
            drop(owned(self.pi.hProcess));
        }
    }
}

pub(super) fn spawn(spec: SpawnSpec, limits: Limits) -> io::Result<Child> {
    let (env_block, child_path) = environment(&spec)?;
    let program = full_path(&resolve(&spec.program, child_path.as_deref())?)?;
    let args: Vec<Vec<u16>> = spec
        .args
        .iter()
        .map(|a| wide(a))
        .collect::<io::Result<_>>()?;
    let (application, mut command_line) = if is_batch_path(&program) {
        let mut cmd = system_directory()?;
        cmd.extend("\\cmd.exe".encode_utf16());
        (cmd, make_bat_command_line(&program, &args)?)
    } else {
        (program, make_command_line(&wide(&spec.program)?, &args))
    };
    let application = nul_terminated(application);
    command_line.push(0);
    let dir = spec.cwd.as_deref().map(directory).transpose()?;
    let root = root_job()?;

    let (stdin, stdin_parent) = stdin_of(spec.stdin)?;
    let (stdout, stdout_parent) = output_of(&spec.stdout, STD_OUTPUT_HANDLE)?;
    let (stderr, stderr_parent) = output_of(&spec.stderr, STD_ERROR_HANDLE)?;
    let raw = |s: &Stream| {
        s.child
            .as_ref()
            .map_or(std::ptr::null_mut(), |h| h.as_raw_handle())
    };
    let handles: Box<[HANDLE]> = [&stdin, &stdout, &stderr]
        .into_iter()
        .filter_map(|s| s.child.as_ref().map(|h| h.as_raw_handle()))
        .collect();
    let inherit = !handles.is_empty();
    let list = if inherit {
        Some(HandleList::new(handles)?)
    } else {
        None
    };

    // SAFETY: a zeroed STARTUPINFOEXW is valid; the fields set below are plain values.
    let mut si: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    si.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.StartupInfo.hStdInput = raw(&stdin);
    si.StartupInfo.hStdOutput = raw(&stdout);
    si.StartupInfo.hStdError = raw(&stderr);
    let mut flags = CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT;
    if let Some(l) = &list {
        si.lpAttributeList = l.ptr();
        flags |= EXTENDED_STARTUPINFO_PRESENT;
    }
    // SAFETY: a zeroed PROCESS_INFORMATION is valid.
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: every pointer is NUL-terminated or null, and outlives the call.
    check(unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            inherit as i32,
            flags,
            env_block
                .as_ref()
                .map_or(std::ptr::null(), |b| b.as_ptr().cast()),
            dir.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
            (&raw const si).cast::<STARTUPINFOW>(),
            &mut pi,
        )
    })?;
    let mut created = Created {
        pi,
        reader_job: None,
        committed: false,
    };
    // The child's ends now belong to the child: s4 closes its copies, so EOF works.
    drop((stdin, stdout, stderr, list));

    // The critical order (plan D3): assign while suspended, then resume.
    assign(root, created.process())?;
    if let Some(bytes) = limits.process_memory_bytes {
        let reader_job = Job::new(Some(bytes))?;
        assign(&reader_job, created.process())?;
        created.retain_reader_job(reader_job);
    }
    // SAFETY: the thread handle came from CreateProcessW.
    if unsafe { ResumeThread(created.thread()) } == RESUME_FAILED {
        return Err(io::Error::last_os_error());
    }
    Ok(Child {
        stdin: stdin_parent,
        stdout: stdout_parent,
        stderr: stderr_parent,
        process: created.into_child(),
    })
}

pub(super) struct Process {
    handle: OwnedHandle,
    pid: u32,
    /// A reader's own Job: closing it ends the reader.
    _reader_job: Option<Job>,
    status: Option<ExitStatus>,
}

impl Process {
    pub(super) fn id(&self) -> u32 {
        self.pid
    }

    pub(super) fn kill(&mut self) -> io::Result<()> {
        if self.status.is_some() {
            return Ok(());
        }
        // SAFETY: the handle is valid while `self` lives.
        if unsafe { TerminateProcess(self.handle.as_raw_handle(), 1) } == 0 {
            // SAFETY: reads the calling thread's last error.
            let code = unsafe { GetLastError() };
            // A process that already ended refuses with ERROR_ACCESS_DENIED (as std checks).
            if code != ERROR_ACCESS_DENIED || self.try_wait()?.is_none() {
                return Err(io::Error::from_raw_os_error(code as i32));
            }
        }
        Ok(())
    }

    fn wait_for(&mut self, ms: u32) -> io::Result<Option<ExitStatus>> {
        if let Some(s) = self.status {
            return Ok(Some(s));
        }
        // SAFETY: the handle is valid while `self` lives.
        match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), ms) } {
            WAIT_OBJECT_0 => {}
            WAIT_TIMEOUT => return Ok(None),
            _ => return Err(io::Error::last_os_error()),
        }
        let mut code = 0u32;
        // SAFETY: the process ended; its code is read into `code`.
        check(unsafe { GetExitCodeProcess(self.handle.as_raw_handle(), &mut code) })?;
        let s = ExitStatus::from_raw(code);
        self.status = Some(s);
        Ok(Some(s))
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.wait_for(0)
    }

    pub(super) fn wait(&mut self) -> io::Result<ExitStatus> {
        self.wait_for(INFINITE)?
            .ok_or_else(|| io::Error::other("an infinite wait timed out"))
    }

    pub(super) fn wait_token(&self) -> io::Result<WaitToken> {
        let mut dup: HANDLE = std::ptr::null_mut();
        // SAFETY: duplicates the process handle within this process, not inheritable.
        check(unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.handle.as_raw_handle(),
                GetCurrentProcess(),
                &mut dup,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        })?;
        Ok(WaitToken(unsafe { owned(dup) }))
    }
}

/// A duplicate of the process handle: waiting on it neither needs the `Child` nor reaps.
pub(super) struct WaitToken(OwnedHandle);

impl WaitToken {
    pub(super) fn wait(&self, timeout: Option<Duration>) -> io::Result<bool> {
        let ms = timeout.map_or(INFINITE, |t| {
            u32::try_from(t.as_millis())
                .unwrap_or(INFINITE - 1)
                .min(INFINITE - 1)
        });
        // SAFETY: the handle is valid while `self` lives.
        match unsafe { WaitForSingleObject(self.0.as_raw_handle(), ms) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Limits, Output, SpawnSpec, spawn};

    #[test]
    fn windows_argument_quoting_covers_empty_quotes_and_trailing_slashes() {
        let q = |arg: &str| {
            String::from_utf16(&quote_windows_arg(&arg.encode_utf16().collect::<Vec<_>>())).unwrap()
        };
        // The builder always quotes each argument, including simple ones.
        assert_eq!(q(""), "\"\"");
        assert_eq!(q("a b"), "\"a b\"");
        assert_eq!(q("a\"b"), "\"a\\\"b\"");
        assert_eq!(q("a b\\"), "\"a b\\\\\"");
        assert_eq!(q("plain"), "\"plain\"");
    }

    #[test]
    #[ignore = "isolated child for Windows spawn resolution"]
    fn windows_spawn_resolution_probe() {
        let mut s = SpawnSpec::new("s4-fake-helper");
        // Harmless if it ever ran: the fake is a copy of cmd.exe.
        s.args = ["/c", "exit", "7"].into_iter().map(Into::into).collect();
        // Resolving the name in the process cwd would return Ok(child) and run it.
        let err = spawn(
            s,
            Limits {
                process_memory_bytes: None,
            },
        )
        .err()
        .unwrap();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn windows_bare_program_name_does_not_search_cwd() {
        let dir = tempfile::tempdir().unwrap();
        // A runnable executable named like a bare program, only in the cwd.
        let cmd = std::path::Path::new(&std::env::var_os("SystemRoot").unwrap())
            .join("System32")
            .join("cmd.exe");
        std::fs::copy(cmd, dir.path().join("s4-fake-helper.exe")).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "platform::windows::tests::windows_spawn_resolution_probe",
                "--ignored",
            ])
            .current_dir(dir.path())
            .env("PATH", dir.path().join("empty-path"))
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn windows_explicit_path_without_suffix_finds_the_exe() {
        let dir = tempfile::tempdir().unwrap();
        let cmd = std::path::Path::new(&std::env::var_os("SystemRoot").unwrap())
            .join("System32")
            .join("cmd.exe");
        // Only `helper.exe` exists; the caller names `helper`.
        std::fs::copy(cmd, dir.path().join("helper.exe")).unwrap();
        let mut s = SpawnSpec::new(dir.path().join("helper").into_os_string());
        s.args = ["/c", "exit", "7"].into_iter().map(Into::into).collect();
        s.stdout = Output::Null;
        s.stderr = Output::Null;
        let mut c = spawn(
            s,
            Limits {
                process_memory_bytes: None,
            },
        )
        .unwrap();
        assert_eq!(c.wait().unwrap().code(), Some(7));
    }

    #[test]
    fn windows_explicit_path_that_does_not_exist_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let err = spawn(
            SpawnSpec::new(dir.path().join("absent").into_os_string()),
            Limits {
                process_memory_bytes: None,
            },
        )
        .err()
        .unwrap();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn managed_spawn_runs_batch_files_like_std() {
        use std::{ffi::OsString, io::Read};
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path().join("with space");
        std::fs::create_dir(&dir).unwrap();
        for ext in ["cmd", "bat"] {
            let script = dir.join(format!("show args.{ext}"));
            std::fs::write(&script, "@echo off\r\necho [%1] [%2]\r\nexit /b 5\r\n").unwrap();
            for args in [
                vec!["plain", "a b"],
                vec!["&", "|"],
                vec!["%", "^"],
                vec!["say \"hi\"", "C:\\dir\\"],
                vec!["", "é"],
            ] {
                let want = std::process::Command::new(&script)
                    .args(&args)
                    .output()
                    .unwrap();
                let mut s = SpawnSpec::new(script.clone().into_os_string());
                s.args = args.iter().map(|a| OsString::from(*a)).collect();
                s.stdout = Output::Piped;
                s.stderr = Output::Null;
                let mut c = spawn(
                    s,
                    Limits {
                        process_memory_bytes: None,
                    },
                )
                .unwrap();
                let mut out = Vec::new();
                c.stdout.take().unwrap().read_to_end(&mut out).unwrap();
                let status = c.wait().unwrap();
                assert_eq!(out, want.stdout, "{ext} {args:?}");
                assert_eq!(status.code(), want.status.code(), "{ext} {args:?}");
                assert_eq!(status.code(), Some(5), "{ext} {args:?}");
            }
        }
    }

    #[test]
    #[ignore = "isolated child for the Windows batch interpreter lookup"]
    fn windows_batch_interpreter_probe() {
        let script = std::env::var_os("SEARCHFOR_BATCH_PROBE_SCRIPT").unwrap();
        let mut s = SpawnSpec::new(script);
        s.stdout = Output::Null;
        s.stderr = Output::Null;
        let mut c = spawn(
            s,
            Limits {
                process_memory_bytes: None,
            },
        )
        .unwrap();
        // The real interpreter ran the script, whose exit code is 5. A fake `cmd.exe`
        // in the cwd or on PATH would give another code.
        assert_eq!(c.wait().unwrap().code(), Some(5));
    }

    #[test]
    fn windows_batch_interpreter_is_not_searched_in_cwd_or_path() {
        let dir = tempfile::tempdir().unwrap();
        // A fake `cmd.exe`: the test binary itself, which fails on `/e:ON ...`.
        std::fs::copy(std::env::current_exe().unwrap(), dir.path().join("cmd.exe")).unwrap();
        let script = tempfile::Builder::new().suffix(".cmd").tempfile().unwrap();
        std::fs::write(script.path(), "@exit /b 5\r\n").unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "platform::windows::tests::windows_batch_interpreter_probe",
                "--ignored",
            ])
            .current_dir(dir.path())
            .env("PATH", dir.path())
            .env("SEARCHFOR_BATCH_PROBE_SCRIPT", script.path())
            .status()
            .unwrap();
        assert!(status.success());
    }
}

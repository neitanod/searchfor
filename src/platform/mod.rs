//! The process layer (M5b spec §4; plan D3, D4, D20, D21, D22): every process s4 starts goes
//! through `spawn`. On Unix it delegates to `std::process::Command`, keeping s4's process group;
//! on Windows a native launcher starts the child suspended, assigns it to s4's kill-on-close Job
//! Object (and to a reader's own memory-limited Job), and only then lets it run, so neither the
//! child nor anything it starts can outlive s4.

mod batch;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
use windows as imp;

use std::{ffi::OsString, io, path::PathBuf, process::ExitStatus, time::Duration};

/// Where a child's stdin comes from.
pub enum Input {
    /// The null device.
    Null,
    /// A pipe whose write end is `Child::stdin`.
    Piped,
    /// An open file, read by the child from its current position.
    File(std::fs::File),
    /// s4's own stdin.
    Inherit,
}

/// Where a child's stdout or stderr goes.
pub enum Output {
    /// The null device.
    Null,
    /// A pipe whose read end is `Child::stdout` or `Child::stderr`.
    Piped,
    /// s4's own stream.
    Inherit,
}

/// What to start: the program, its arguments, its directory, its environment and its three
/// streams. Nothing is inherited unless asked: the streams default to the null device.
pub struct SpawnSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    // None removes a variable; unset entries inherit the parent's environment.
    pub env: Vec<(OsString, Option<OsString>)>,
    pub stdin: Input,
    pub stdout: Output,
    pub stderr: Output,
}

impl SpawnSpec {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            stdin: Input::Null,
            stdout: Output::Null,
            stderr: Output::Null,
        }
    }
}

/// `process_memory_bytes`: on Windows, a second Job Object of the child's own with that limit
/// of committed memory (a document reader, M5b spec §4.3). None is the common Job only. Unix
/// ignores it: the reader sets its own rlimits.
#[derive(Debug, Clone, Copy, Default)]
pub struct Limits {
    pub process_memory_bytes: Option<usize>,
}

/// A started process. Dropping it closes its handles and pipes; it neither kills nor reaps, so
/// every owner keeps the obligation to `kill` and `wait` (on Windows, a reader's own Job ends
/// the reader when the handle closes).
pub struct Child {
    pub stdin: Option<PipeWriter>,
    pub stdout: Option<PipeReader>,
    pub stderr: Option<PipeReader>,
    // Owns the process handle and nested Job for the whole worker lifetime.
    process: imp::Process,
}

/// The read end of a child's stdout or stderr.
pub struct PipeReader(io::PipeReader);

/// The write end of a child's stdin.
pub struct PipeWriter(io::PipeWriter);

/// A capability to wait for a process to end, owned apart from its `Child`: a thread can wait
/// with it while another holds the `Child`'s lock to kill it. It never reaps.
pub struct WaitToken(imp::WaitToken);

/// Starts `spec`. An error means no process is left running.
pub fn spawn(mut spec: SpawnSpec, limits: Limits) -> io::Result<Child> {
    without_parent_variable(&mut spec.env);
    imp::spawn(spec, limits)
}

/// The pid of the s4 process that started this one and that this one must not outlive: set
/// for an MCP search (and the example helper's test wait knob), read by
/// `initialize_parent_watch` before anything else runs. Document readers keep their own
/// `SEARCHFOR_READER_OF`.
pub const PROCESS_PARENT_ENV: &str = "SEARCHFOR_PROCESS_PARENT";

/// Makes this process end when `expected_parent` ends (M5b spec §4.2): Linux has the kernel
/// kill it (`PR_SET_PDEATHSIG`), macOS watches the parent with kqueue, and Windows relies on
/// the parent's kill-on-close Job. A parent that is already gone, or is not this process's
/// parent, ends this process at once. Register once in the child, before starting background
/// work. On Linux the death signal follows the thread that started the child: start it from a
/// thread that lives as long as the parent's interest in it.
pub fn orphan_dies(expected_parent: u32) -> io::Result<()> {
    imp::orphan_dies(expected_parent)
}

/// `orphan_dies` for the parent `PROCESS_PARENT_ENV` names, when it names one.
pub fn initialize_parent_watch() -> io::Result<()> {
    let Some(v) = std::env::var_os(PROCESS_PARENT_ENV) else {
        return Ok(());
    };
    let pid = v
        .to_str()
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{PROCESS_PARENT_ENV} is not a process id: {v:?}"),
            )
        })?;
    orphan_dies(pid)
}

/// Windows (plan D5): on the console's Ctrl-C or Ctrl-Break, sets `stopped` and wakes a
/// coordinator thread started now, which runs `cancel` — outside the console's callback, which
/// takes no lock — and then ends s4 with STATUS_CONTROL_C_EXIT (0xC000013A), as an unhandled
/// Ctrl-C does. The callback waits for that cleanup at most two seconds, then ends s4 anyway:
/// Ctrl-C always ends s4, as it ends rg, and s4's Job Object ends what is left. Installed once;
/// later calls return the first outcome. Unix does nothing here: signals are the Supervisor's.
pub fn watch_console(
    stopped: &'static std::sync::atomic::AtomicBool,
    cancel: fn(),
) -> io::Result<()> {
    imp::watch_console(stopped, cancel)
}

/// A child never inherits `PROCESS_PARENT_ENV` unless its spec sets it: it names this
/// process's parent, not the child's.
fn without_parent_variable(env: &mut Vec<(OsString, Option<OsString>)>) {
    if !env.iter().any(|(k, _)| k == PROCESS_PARENT_ENV) {
        env.push((PROCESS_PARENT_ENV.into(), None));
    }
}

impl Child {
    pub fn id(&self) -> u32 {
        self.process.id()
    }

    /// Ends the process. A process that already ended, or was reaped, is not an error.
    pub fn kill(&mut self) -> io::Result<()> {
        self.process.kill()
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.process.try_wait()
    }

    /// Waits for the process to end and reaps it; later calls return the same status.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.process.wait()
    }

    pub fn wait_token(&self) -> io::Result<WaitToken> {
        self.process.wait_token().map(WaitToken)
    }
}

impl WaitToken {
    // Wait without owning the Supervisor's Child lock; does not reap.
    /// True once the process has ended; false when `timeout` passed first.
    pub fn wait(&self, timeout: Option<Duration>) -> io::Result<bool> {
        self.0.wait(timeout)
    }
}

impl io::Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    #[ignore = "child entry point invoked by managed_spawn_preserves_environment_and_pipes"]
    fn child_echo_environment() {
        if let Some(p) = std::env::var_os("SEARCHFOR_PLATFORM_ECHO") {
            std::fs::write(
                p,
                std::env::var_os("SEARCHFOR_PLATFORM_VALUE")
                    .unwrap()
                    .to_string_lossy()
                    .as_bytes(),
            )
            .unwrap();
        }
    }

    #[test]
    fn managed_spawn_preserves_environment_and_pipes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("echo file");
        let mut s = SpawnSpec::new(std::env::current_exe().unwrap().into_os_string());
        s.args = [
            "--exact",
            "platform::tests::child_echo_environment",
            "--ignored",
            "--nocapture",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        s.env = vec![
            (
                "SEARCHFOR_PLATFORM_ECHO".into(),
                Some(path.clone().into_os_string()),
            ),
            ("SEARCHFOR_PLATFORM_VALUE".into(), Some("a b\\\"é%".into())),
        ];
        s.stdout = Output::Piped;
        s.stderr = Output::Piped;
        let mut c = spawn(
            s,
            Limits {
                process_memory_bytes: None,
            },
        )
        .unwrap();
        let token = c.wait_token().unwrap();
        let mut out = String::new();
        c.stdout.take().unwrap().read_to_string(&mut out).unwrap();
        assert!(token.wait(Some(Duration::from_secs(10))).unwrap());
        assert!(c.wait().unwrap().success(), "{out}");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "a b\\\"é%");
    }

    #[test]
    fn spawn_failure_returns_error_without_a_live_child() {
        let s = SpawnSpec::new("searchfor-guaranteed-missing-platform-program");
        assert!(
            spawn(
                s,
                Limits {
                    process_memory_bytes: None
                }
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "child entry invoked by the parent watch tests"]
    fn parent_watch_probe() {
        initialize_parent_watch().unwrap();
        let out = std::env::var_os("SEARCHFOR_PLATFORM_ECHO").unwrap();
        std::fs::write(out, "survived").unwrap();
    }

    /// Runs `parent_watch_probe` with PROCESS_PARENT_ENV set to `parent`; its status, and
    /// whether it got past the watch.
    #[cfg(unix)]
    fn parent_watch(parent: &str) -> (std::process::ExitStatus, bool) {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "platform::tests::parent_watch_probe",
                "--ignored",
            ])
            .env(PROCESS_PARENT_ENV, parent)
            .env("SEARCHFOR_PLATFORM_ECHO", &out)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        (status, out.exists())
    }

    #[cfg(unix)]
    #[test]
    fn the_parent_watch_ends_a_child_whose_parent_is_another() {
        let began = std::time::Instant::now();
        let (status, survived) = parent_watch(&u32::MAX.to_string());
        assert!(!status.success() && !survived, "{status:?}");
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "{:?}",
            began.elapsed()
        );
        let (status, survived) = parent_watch(&std::process::id().to_string());
        assert!(status.success() && survived, "{status:?}");
    }

    /// A child never inherits the parent pid meant for s4 itself: a reader or a helper of an
    /// MCP search would otherwise compare it with its own parent and exit.
    #[test]
    fn spawn_removes_the_parent_variable_unless_the_spec_sets_it() {
        let mut env = vec![("A".into(), Some("1".into()))];
        without_parent_variable(&mut env);
        assert_eq!(
            env,
            vec![
                ("A".into(), Some("1".into())),
                (PROCESS_PARENT_ENV.into(), None)
            ]
        );
        let mut env = vec![(PROCESS_PARENT_ENV.into(), Some("7".into()))];
        without_parent_variable(&mut env);
        assert_eq!(env, vec![(PROCESS_PARENT_ENV.into(), Some("7".into()))]);
    }
}

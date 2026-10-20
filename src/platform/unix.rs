//! Unix: `std::process::Command` starts the child, in s4's process group, as before M5b. A
//! process dies with s4 through its pipes, the Supervisor's signal watcher and, for readers and
//! MCP searches, `orphan_dies`.
use super::{Child, Input, Limits, Output, PipeReader, PipeWriter, SpawnSpec};
use std::io;
use std::os::fd::OwnedFd;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub(super) struct Process {
    child: std::process::Child,
    /// Set once `wait` or `try_wait` reaped it: its pid may then belong to another process.
    status: Option<ExitStatus>,
}

pub(super) fn spawn(spec: SpawnSpec, _limits: Limits) -> io::Result<Child> {
    let mut c = Command::new(&spec.program);
    c.args(&spec.args);
    if let Some(dir) = &spec.cwd {
        c.current_dir(dir);
    }
    for (k, v) in &spec.env {
        match v {
            Some(v) => c.env(k, v),
            None => c.env_remove(k),
        };
    }
    c.stdin(match spec.stdin {
        Input::Null => Stdio::null(),
        Input::Piped => Stdio::piped(),
        Input::File(f) => f.into(),
        Input::Inherit => Stdio::inherit(),
    });
    let out = |o: &Output| match o {
        Output::Null => Stdio::null(),
        Output::Piped => Stdio::piped(),
        Output::Inherit => Stdio::inherit(),
    };
    c.stdout(out(&spec.stdout));
    c.stderr(out(&spec.stderr));
    let mut child = c.spawn()?;
    Ok(Child {
        stdin: child
            .stdin
            .take()
            .map(|p| PipeWriter(OwnedFd::from(p).into())),
        stdout: child
            .stdout
            .take()
            .map(|p| PipeReader(OwnedFd::from(p).into())),
        stderr: child
            .stderr
            .take()
            .map(|p| PipeReader(OwnedFd::from(p).into())),
        process: Process {
            child,
            status: None,
        },
    })
}

impl Process {
    pub(super) fn id(&self) -> u32 {
        self.child.id()
    }

    pub(super) fn kill(&mut self) -> io::Result<()> {
        // std signals nothing once the child was reaped, and a child that exited is not an error.
        self.child.kill()
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let s = self.child.try_wait()?;
        if s.is_some() {
            self.status = s;
        }
        Ok(s)
    }

    pub(super) fn wait(&mut self) -> io::Result<ExitStatus> {
        let s = self.child.wait()?;
        self.status = Some(s);
        Ok(s)
    }

    pub(super) fn wait_token(&self) -> io::Result<WaitToken> {
        Ok(WaitToken {
            pid: self.child.id() as libc::pid_t,
            reaped: self.status.is_some(),
        })
    }
}

/// Unix has no handle to a process that outlives its reaping, so the token asks the kernel
/// about the pid with `waitid(WNOWAIT)`, which reports an ended child without reaping it, and
/// polls. A pid its owner reaped meanwhile is no longer s4's child (`ECHILD`): it ended.
pub(super) struct WaitToken {
    pid: libc::pid_t,
    reaped: bool,
}

impl WaitToken {
    pub(super) fn wait(&self, timeout: Option<Duration>) -> io::Result<bool> {
        if self.reaped {
            return Ok(true);
        }
        let deadline = timeout.map(|t| Instant::now() + t);
        let mut pause = Duration::from_millis(1);
        loop {
            if self.ended()? {
                return Ok(true);
            }
            let left = match deadline {
                Some(d) => match d.checked_duration_since(Instant::now()) {
                    Some(left) if !left.is_zero() => left,
                    _ => return Ok(false),
                },
                None => pause,
            };
            std::thread::sleep(pause.min(left));
            pause = (pause * 2).min(Duration::from_millis(10));
        }
    }

    fn ended(&self) -> io::Result<bool> {
        loop {
            // SAFETY: waitid writes into the zeroed siginfo_t it is given, and nothing else.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let rc = unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.pid as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if rc == 0 {
                return Ok(siginfo_pid(&info) != 0);
            }
            let e = io::Error::last_os_error();
            match e.raw_os_error() {
                Some(libc::EINTR) => continue,
                Some(libc::ECHILD) => return Ok(true),
                _ => return Err(e),
            }
        }
    }
}

/// The pid of the child `waitid` reported; 0 when none changed state (`WNOHANG`).
fn siginfo_pid(info: &libc::siginfo_t) -> libc::pid_t {
    // SAFETY: waitid filled the SIGCHLD fields, or left the whole struct zeroed.
    unsafe { info.si_pid() }
}

#[cfg(test)]
mod tests {
    use crate::platform::{Limits, SpawnSpec, spawn};
    use std::time::{Duration, Instant};

    /// The token waits outside the `Child`, does not reap, and answers after the owner reaped.
    #[test]
    fn a_wait_token_sees_the_end_without_reaping_it() {
        let mut s = SpawnSpec::new("sleep");
        s.args = vec!["0.3".into()];
        let mut c = spawn(s, Limits::default()).unwrap();
        let token = c.wait_token().unwrap();
        let began = Instant::now();
        assert!(!token.wait(Some(Duration::from_millis(50))).unwrap());
        let waited = began.elapsed();
        assert!(waited >= Duration::from_millis(50), "{waited:?}");
        assert!(waited < Duration::from_millis(250), "{waited:?}");
        assert!(token.wait(Some(Duration::from_secs(10))).unwrap());
        // Not reaped by the token: the owner still collects its status.
        assert!(c.try_wait().unwrap().unwrap().success());
        assert!(token.wait(None).unwrap());
        assert!(c.wait_token().unwrap().wait(Some(Duration::ZERO)).unwrap());
        // A kill after the reap signals nothing and is not an error.
        assert!(c.kill().is_ok());
    }
}

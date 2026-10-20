//! macOS: a child watches its parent with kqueue (M5b spec §4.2; plan D6). The registration
//! (`EVFILT_PROC`, `NOTE_EXIT`) is followed by a second check of the parent pid, so a parent
//! that died before it was registered is noticed too; then a thread waits for the event and
//! ends the process, idle or busy.
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

pub(crate) fn orphan_dies(expected_parent: u32) -> io::Result<()> {
    // SAFETY: kqueue takes no arguments and returns a new descriptor or -1.
    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `kq` is a new descriptor nothing else owns.
    let kq = unsafe { OwnedFd::from_raw_fd(kq) };
    // SAFETY: a zeroed kevent is valid; the fields set are plain integers.
    let mut event: libc::kevent = unsafe { std::mem::zeroed() };
    event.ident = expected_parent as libc::uintptr_t;
    event.filter = libc::EVFILT_PROC;
    event.flags = libc::EV_ADD | libc::EV_ONESHOT;
    event.fflags = libc::NOTE_EXIT;
    // SAFETY: one change, read from `event`; no event is returned.
    let rc = unsafe {
        libc::kevent(
            kq.as_raw_fd(),
            &event,
            1,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
        )
    };
    if rc < 0 {
        let e = io::Error::last_os_error();
        // ESRCH: the parent no longer exists.
        if e.raw_os_error() == Some(libc::ESRCH) {
            std::process::exit(2);
        }
        return Err(e);
    }
    // A parent that ended before the registration left this process to another parent.
    // SAFETY: getppid has no memory effects.
    if unsafe { libc::getppid() } as u32 != expected_parent {
        std::process::exit(2);
    }
    // A NOTE_EXIT already queued (or an EV_ERROR for the parent) ends this process now.
    if next_event(
        &kq,
        Some(libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        }),
    )? {
        std::process::exit(2);
    }
    std::thread::Builder::new()
        .name("parent watch".into())
        .spawn(move || {
            // The descriptor lives, and is closed, with this thread. Whatever ends the wait,
            // the parent's exit or a broken watch, the process ends: an orphan never stays.
            let _ = next_event(&kq, None);
            // SAFETY: ends the process at once, as the kernel's SIGKILL does on Linux.
            unsafe { libc::_exit(2) }
        })?;
    Ok(())
}

/// Waits for one event of `kq`, at most `timeout`; true when one came. Retries on EINTR.
fn next_event(kq: &OwnedFd, timeout: Option<libc::timespec>) -> io::Result<bool> {
    loop {
        // SAFETY: a zeroed kevent is valid output storage.
        let mut out: libc::kevent = unsafe { std::mem::zeroed() };
        let t = timeout.as_ref().map_or(std::ptr::null(), |t| t as *const _);
        // SAFETY: no changes; at most one event is written into `out`.
        let n = unsafe { libc::kevent(kq.as_raw_fd(), std::ptr::null(), 0, &mut out, 1, t) };
        if n >= 0 {
            // NOTE_EXIT, or EV_ERROR with the errno of the parent's watch: either way the
            // parent cannot be watched any longer.
            return Ok(n > 0);
        }
        let e = io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::EINTR) {
            return Err(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "child entry for a parent mismatch; exits this process"]
    fn orphan_probe() {
        crate::platform::orphan_dies(u32::MAX).unwrap();
        std::thread::sleep(Duration::from_secs(30));
        panic!("an orphan survived registration");
    }

    #[test]
    fn already_dead_parent_is_detected_before_work() {
        let began = Instant::now();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "platform::macos::tests::orphan_probe",
                "--ignored",
            ])
            .status()
            .unwrap();
        assert!(!status.success());
        assert!(began.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn watching_the_actual_parent_does_not_end_this_child() {
        let parent = unsafe { libc::getppid() } as u32;
        crate::platform::orphan_dies(parent).unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
}

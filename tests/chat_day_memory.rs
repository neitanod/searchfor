//! Review r1 of M6, Fable #15: a WhatsApp `day` keeps no list of its messages, so one day of
//! millions of messages costs what its bytes cost. Alone in its binary: `RUSAGE_CHILDREN`
//! reports the largest child this process waited for, and no other test spawns one here.
#![cfg(target_os = "linux")]
mod common;
use common::{Fixture, code, run, stderr, stdout};

/// The peak RSS, in bytes, of the largest child this process has waited for.
fn children_peak_rss() -> u64 {
    // SAFETY: getrusage writes a `rusage` it is given a valid pointer to.
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    assert_eq!(unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut u) }, 0);
    u.ru_maxrss as u64 * 1024
}

#[test]
fn m6_one_whatsapp_day_of_two_million_messages() {
    let fx = Fixture::new();
    let line = b"01/01/24, 10:00 - Ana: plain\n";
    let mut src = line.repeat(2_000_000);
    src.extend_from_slice(b"01/01/24, 10:01 - Bob: needle\n");
    let len = src.len() as u64;
    std::fs::write(fx.file("chat.txt"), &src).unwrap();
    drop(src);
    let out = run(&mut fx.s4([
        "--no-docs",
        "--flat",
        "--format",
        "whatsapp",
        "needle",
        "chat.txt",
    ]));
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "2000001: 01/01/24 › 10:01 Bob │ 01/01/24, 10:01 - Bob: needle\n"
    );
    // 88 bytes a message would be 176 MB here; the file itself may be mapped whole.
    let peak = children_peak_rss();
    assert!(
        peak <= len + (64 << 20),
        "peak RSS {peak} for a {len}-byte day"
    );
}

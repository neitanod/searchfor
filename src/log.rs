//! `--debug` and `--trace` (M5a plan, D15): messages on stderr in ripgrep 14.1.1's format
//! (crates/core/logger.rs), `s4: LEVEL|target|file:line: message`, from s4 and from the crates
//! it shares with ripgrep (ignore, globset, grep-regex, grep-searcher…), which log through the
//! `log` crate as they do in rg: why a file is skipped reads as rg says it. Without either flag
//! nothing is logged, and a message costs one comparison of its level.
use crate::cli::Logging;

struct Logger;

static LOGGER: Logger = Logger;

impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        let line = format_record(record);
        // One write per message, so the lines of several threads do not mix.
        let _ = std::io::Write::write_all(&mut std::io::stderr().lock(), line.as_bytes());
    }

    fn flush(&self) {}
}

/// rg's format, prefixed `s4: ` as rg's messages are `rg: `.
fn format_record(record: &log::Record<'_>) -> String {
    match (record.file(), record.line()) {
        (Some(file), Some(line)) => format!(
            "s4: {}|{}|{}:{}: {}\n",
            record.level(),
            record.target(),
            file,
            line,
            record.args()
        ),
        (Some(file), None) => format!(
            "s4: {}|{}|{}: {}\n",
            record.level(),
            record.target(),
            file,
            record.args()
        ),
        _ => format!(
            "s4: {}|{}: {}\n",
            record.level(),
            record.target(),
            record.args()
        ),
    }
}

/// Starts the logger at the level asked for; nothing is logged without one. A second call (a
/// test, `--mcp`'s searches) changes the level only.
pub fn init(level: Option<Logging>) {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(match level {
        None => log::LevelFilter::Off,
        Some(Logging::Debug) => log::LevelFilter::Debug,
        Some(Logging::Trace) => log::LevelFilter::Trace,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_print_as_ripgreps() {
        let r = log::Record::builder()
            .level(log::Level::Debug)
            .target("ignore::walk")
            .file(Some("src/walk.rs"))
            .line(Some(7))
            .args(format_args!("ignoring ./x: Ignore(...)"))
            .build();
        assert_eq!(
            format_record(&r),
            "s4: DEBUG|ignore::walk|src/walk.rs:7: ignoring ./x: Ignore(...)\n"
        );
        let r = log::Record::builder()
            .level(log::Level::Trace)
            .target("searchfor")
            .args(format_args!("x"))
            .build();
        assert_eq!(format_record(&r), "s4: TRACE|searchfor: x\n");
    }
}

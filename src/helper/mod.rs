//! Helper Processors (spec §4.4, ADR 0005): the configuration of a helper, how a process is
//! launched, and the errors of a request.
pub mod host;
pub mod protocol;
pub mod serve;

use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

/// Where warnings go: the pipeline prints them on stderr unless `--no-messages`.
pub type Warn = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pool,
    Concurrent,
    Oneshot,
}

pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(5000);

#[derive(Debug, Clone)]
pub struct HostConfig {
    /// The Format Definition's name: the key of the host and the prefix of its warnings.
    pub format: String,
    pub command: Vec<String>,
    pub mode: Mode,
    pub timeout: Duration,
    /// Sent in `hello`.
    pub params: toml::Table,
    /// The search's thread count: the pool's size, and the requests in flight in `concurrent`.
    pub threads: usize,
}

impl HostConfig {
    /// From the `[params]` of an `external` Definition: `command` (array of strings, required),
    /// `mode` ("pool" by default, "concurrent", "oneshot"), `timeout_ms` (positive, 5000).
    pub fn external(format: &str, params: &toml::Table, threads: usize) -> Result<Self, String> {
        let command: Vec<String> = match params.get("command").and_then(|v| v.as_array()) {
            Some(a) if !a.is_empty() && a.iter().all(|v| v.is_str()) => {
                a.iter().map(|v| v.as_str().unwrap().to_string()).collect()
            }
            _ => return Err("`command` must be a non-empty array of strings".into()),
        };
        let mode = match params.get("mode").map(|v| v.as_str()) {
            None => Mode::Pool,
            Some(Some("pool")) => Mode::Pool,
            Some(Some("concurrent")) => Mode::Concurrent,
            Some(Some("oneshot")) => Mode::Oneshot,
            Some(_) => return Err("`mode` must be \"pool\", \"concurrent\" or \"oneshot\"".into()),
        };
        let timeout = match params.get("timeout_ms") {
            None => DEFAULT_TIMEOUT,
            Some(v) => match v.as_integer() {
                Some(ms) if ms > 0 => Duration::from_millis(ms as u64),
                _ => return Err("`timeout_ms` must be a positive integer".into()),
            },
        };
        Ok(Self {
            format: format.into(),
            command,
            mode,
            timeout,
            params: params.clone(),
            threads: threads.max(1),
        })
    }
}

/// A launched helper: its three pipes and a way to stop it.
pub struct Proc {
    pub stdin: Box<dyn Write + Send>,
    pub stdout: Box<dyn Read + Send>,
    pub stderr: Option<Box<dyn Read + Send>>,
    pub control: Box<dyn Control>,
}

pub trait Control: Send {
    fn kill(&mut self);
    /// Waits up to `within` for the process to exit; true when it did.
    fn wait(&mut self, within: Duration) -> bool;
    /// Collects the exit status after an exit or a kill, so no zombie is left (D32). Blocks
    /// until the process is gone; call it after `kill` or after `wait` returned true.
    fn reap(&mut self);
}

/// Starts a helper process. The real one runs a command; tests run threads over pipes.
pub trait Launcher: Send + Sync {
    fn launch(&self) -> std::io::Result<Proc>;
}

/// A Processor that cannot describe this file: the pipeline prints it as Plain, without a
/// warning of its own (D33). Returned inside `anyhow::Error`; the pipeline finds it with
/// `downcast_ref`.
#[derive(Debug, thiserror::Error)]
#[error("unavailable")]
pub struct Unavailable;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HelperError {
    /// The Definition is disabled for this run; its files print as Plain, silently.
    #[error("disabled for this run")]
    Disabled,
    #[error("{0}")]
    Answer(String),
    #[error("did not answer within {0} ms")]
    Timeout(u128),
    #[error("{0}")]
    Crashed(String),
    /// A JSON answer with a known id whose fields do not fit the protocol (D41).
    #[error("{0}")]
    Protocol(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(src: &str) -> toml::Table {
        toml::from_str(src).unwrap()
    }

    #[test]
    fn external_config_reads_command_mode_and_timeout() {
        let c = HostConfig::external(
            "ini",
            &params("command = ['ini-trail', '-q']\nmode = 'concurrent'\ntimeout_ms = 300"),
            6,
        )
        .unwrap();
        assert_eq!(c.command, ["ini-trail", "-q"]);
        assert_eq!(c.mode, Mode::Concurrent);
        assert_eq!(c.timeout, Duration::from_millis(300));
        assert_eq!(c.threads, 6);
        let d = HostConfig::external("ini", &params("command = ['x']"), 1).unwrap();
        assert_eq!(
            (d.mode, d.timeout),
            (Mode::Pool, Duration::from_millis(5000))
        );
        assert_eq!(
            HostConfig::external("ini", &params("command = ['x']\nmode = 'oneshot'"), 1)
                .unwrap()
                .mode,
            Mode::Oneshot
        );
    }

    #[test]
    fn external_config_errors_name_the_field() {
        let e = |src| HostConfig::external("ini", &params(src), 1).unwrap_err();
        assert!(e("").contains("`command`"));
        assert!(e("command = []").contains("`command`"));
        assert!(e("command = 'x'").contains("`command`"));
        assert!(e("command = ['x']\nmode = 'fast'").contains("`mode`"));
        assert!(e("command = ['x']\ntimeout_ms = 0").contains("`timeout_ms`"));
    }
}

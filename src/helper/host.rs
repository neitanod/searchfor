//! The host side of Helper Processors: Host, Conn, Launcher, modes and failures (spec §4.4,
//! design note 5). C4 fills it.
use super::{HostConfig, Launcher, Proc, Warn};

/// Runs `argv` with stdin, stdout and stderr piped, in sfor's working directory.
pub struct CommandLauncher {
    pub argv: Vec<String>,
}

impl Launcher for CommandLauncher {
    fn launch(&self) -> std::io::Result<Proc> {
        unimplemented!("C4")
    }
}

/// The helper processes of one Definition for one run (design note 5).
pub struct Host {
    _cfg: HostConfig,
}

impl Host {
    pub fn new(cfg: HostConfig, _launcher: Box<dyn Launcher>, _warn: Warn) -> Self {
        Self { _cfg: cfg }
    }

    pub fn request(
        &self,
        _req: impl FnOnce(u64) -> super::protocol::Request,
    ) -> Result<super::protocol::Reply, super::HelperError> {
        unimplemented!("C4")
    }

    pub fn shutdown(&self) {}
}

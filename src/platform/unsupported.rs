//! Fallback for unsupported operating systems.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::InstallReport;
use crate::check::Check;

pub fn find_browser_impl(_id: &str) -> Option<PathBuf> {
    None
}

pub fn stable_install_dir(_app_dir: Option<&Path>) -> PathBuf {
    std::env::temp_dir().join(env!("CARGO_BIN_NAME"))
}

pub fn install_registration(_exe: &Path) -> Result<InstallReport> {
    bail!("this operating system is not supported for default-browser registration")
}

pub fn uninstall_registration() -> Result<()> {
    Ok(())
}

pub fn registration_checks() -> Vec<Check> {
    vec![Check::fail(
        "os: registration",
        "default-browser registration is not implemented for this OS",
    )]
}

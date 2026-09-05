//! Platform abstraction: browser catalog, detection, and OS registration.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::check::Check;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
mod unsupported;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
use unsupported as imp;
#[cfg(windows)]
use windows as imp;

pub const EXE_NAME: &str = env!("CARGO_BIN_NAME");
#[cfg(windows)]
pub const EXE_FILE_NAME: &str = concat!(env!("CARGO_BIN_NAME"), ".exe");
#[cfg(not(windows))]
pub const EXE_FILE_NAME: &str = env!("CARGO_BIN_NAME");
pub const DISPLAY_NAME: &str = "Browser Dispatcher";

// ---------------------------------------------------------------------------
// Browser catalog
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsFilter {
    All,
    Macos,
}

#[derive(Debug)]
pub struct BrowserDef {
    pub id: &'static str,
    pub name: &'static str,
    /// Flags that open a private/incognito window, e.g. `--incognito`.
    pub private_args: &'static [&'static str],
    pub os: OsFilter,
}

pub static CATALOG: &[BrowserDef] = &[
    BrowserDef {
        id: "firefox",
        name: "Firefox",
        private_args: &["-private-window"],
        os: OsFilter::All,
    },
    BrowserDef {
        id: "chrome",
        name: "Google Chrome",
        private_args: &["--incognito"],
        os: OsFilter::All,
    },
    BrowserDef {
        id: "chromium",
        name: "Chromium",
        private_args: &["--incognito"],
        os: OsFilter::All,
    },
    BrowserDef {
        id: "edge",
        name: "Microsoft Edge",
        private_args: &["--incognito"],
        os: OsFilter::All,
    },
    BrowserDef {
        id: "brave",
        name: "Brave",
        private_args: &["--incognito"],
        os: OsFilter::All,
    },
    BrowserDef {
        id: "vivaldi",
        name: "Vivaldi",
        private_args: &["--incognito"],
        os: OsFilter::All,
    },
    BrowserDef {
        id: "opera",
        name: "Opera",
        private_args: &["--incognito"],
        os: OsFilter::All,
    },
    BrowserDef {
        id: "safari",
        name: "Safari",
        private_args: &[],
        os: OsFilter::Macos,
    },
];

pub fn catalog_def(id: &str) -> Option<&'static BrowserDef> {
    CATALOG.iter().find(|d| d.id == id)
}

pub fn known_ids() -> Vec<&'static str> {
    CATALOG
        .iter()
        .filter(|d| d.os == OsFilter::All || d.os == OsFilter::Macos)
        .map(|d| d.id)
        .collect()
}

pub fn os_name() -> &'static str {
    #[cfg(windows)]
    {
        "windows"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        "unsupported"
    }
}

/// Detect the executable of a built-in browser id on this machine.
pub fn find_browser(id: &str) -> Option<PathBuf> {
    catalog_def(id)?;
    imp::find_browser_impl(id)
}

/// All built-in browsers detected on this machine, in catalog order.
pub fn detect_all() -> Vec<(&'static BrowserDef, PathBuf)> {
    CATALOG
        .iter()
        .filter(|d| d.os == OsFilter::All || cfg!(target_os = "macos"))
        .filter_map(|d| imp::find_browser_impl(d.id).map(|p| (d, p)))
        .collect()
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Stable, user-writable location the binary is copied to during install.
/// The OS registration points at this copy, so it survives `cargo clean`.
pub fn stable_install_dir(app_dir: Option<&Path>) -> PathBuf {
    imp::stable_install_dir(app_dir)
}

/// Where the previous step put the executable.
pub fn stable_exe_path(app_dir: Option<&Path>) -> PathBuf {
    stable_install_dir(app_dir).join(EXE_FILE_NAME)
}

#[derive(Debug, Default)]
pub struct InstallReport {
    /// Steps completed automatically.
    pub automated: Vec<String>,
    /// Steps the user must perform by hand.
    pub manual: Vec<String>,
}

pub fn install_registration(exe: &Path) -> Result<InstallReport> {
    imp::install_registration(exe)
}

pub fn uninstall_registration() -> Result<()> {
    imp::uninstall_registration()
}

/// Registration health checks for `doctor`.
pub fn registration_checks() -> Vec<Check> {
    imp::registration_checks()
}

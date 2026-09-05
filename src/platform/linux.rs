//! Linux: registration via an XDG desktop file + xdg-settings.

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::InstallReport;
use crate::check::Check;
use crate::platform::{DISPLAY_NAME, EXE_NAME};

pub const DESKTOP_FILE_NAME: &str = "browser-dispatcher.desktop";

pub fn desktop_file_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("applications")
        .join(DESKTOP_FILE_NAME)
}

pub fn find_browser_impl(id: &str) -> Option<PathBuf> {
    let names: &[&str] = match id {
        "firefox" => &["firefox", "firefox-esr"],
        "chrome" => &["google-chrome-stable", "google-chrome"],
        "chromium" => &["chromium", "chromium-browser"],
        "edge" => &["microsoft-edge", "microsoft-edge-stable"],
        "brave" => &["brave-browser", "brave"],
        "vivaldi" => &["vivaldi", "vivaldi-stable"],
        "opera" => &["opera"],
        _ => &[],
    };
    names.iter().find_map(|n| crate::util::find_in_path(n))
}

pub fn stable_install_dir(_app_dir: Option<&Path>) -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(EXE_NAME)
}

pub fn install_registration(exe: &Path) -> Result<InstallReport> {
    let mut report = InstallReport::default();
    let path = desktop_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let desktop = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name={DISPLAY_NAME}\n\
         GenericName=Default browser dispatcher\n\
         Comment=Route URLs to different browsers based on rules\n\
         Exec=\"{exe}\" dispatch %U\n\
         Terminal=false\n\
         NoDisplay=true\n\
         Categories=Network;WebBrowser;\n\
         MimeType=x-scheme-handler/http;x-scheme-handler/https;\n",
        exe = exe.display(),
    );
    std::fs::write(&path, desktop)?;
    report.automated.push(format!("Created {}", path.display()));

    if crate::util::find_in_path("update-desktop-database").is_some() {
        let _ = std::process::Command::new("update-desktop-database")
            .arg(path.parent().unwrap_or(Path::new(".")))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    match crate::util::try_command(
        "xdg-settings",
        &["set", "default-web-browser", DESKTOP_FILE_NAME],
    ) {
        Some(_) => report
            .automated
            .push("xdg-settings: default web browser set".to_string()),
        None => report.manual.push(format!(
            "Could not set the default browser automatically. Run:\n\
             \n  \
             xdg-settings set default-web-browser {DESKTOP_FILE_NAME}"
        )),
    }
    Ok(report)
}

pub fn uninstall_registration() -> Result<()> {
    let path = desktop_file_path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    if crate::util::find_in_path("update-desktop-database").is_some() {
        let _ = std::process::Command::new("update-desktop-database")
            .arg(path.parent().unwrap_or(Path::new(".")))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    Ok(())
}

pub fn registration_checks() -> Vec<Check> {
    let mut checks = Vec::new();
    let stable = super::stable_exe_path(None);

    match std::fs::read_to_string(desktop_file_path()) {
        Ok(text) if text.contains(stable.to_string_lossy().as_ref()) => checks.push(Check::pass(
            "linux: desktop file",
            desktop_file_path().display().to_string(),
        )),
        Ok(_) => checks.push(Check::fail(
            "linux: desktop file",
            format!(
                "{} exists but points at another executable; run `browser-dispatcher install` again",
                desktop_file_path().display()
            ),
        )),
        Err(_) => checks.push(Check::fail(
            "linux: desktop file",
            format!("{} missing; run `browser-dispatcher install`", desktop_file_path().display()),
        )),
    }

    match crate::util::try_command("xdg-settings", &["get", "default-web-browser"]) {
        Some(current) if current.trim_end_matches(".desktop") == DESKTOP_FILE_NAME.trim_end_matches(".desktop") => {
            checks.push(Check::pass(
                "linux: default web browser",
                format!("xdg-settings -> {current}"),
            ));
        }
        Some(current) => checks.push(Check::warn(
            "linux: default web browser",
            format!("current default is {current}; run `xdg-settings set default-web-browser {DESKTOP_FILE_NAME}`"),
        )),
        None => checks.push(Check::warn(
            "linux: default web browser",
            "xdg-settings not available; cannot verify the default browser setting".to_string(),
        )),
    }
    checks
}

//! Windows: browser registration via HKCU registry entries.
//!
//! Layout (all under HKEY_CURRENT_USER, no admin rights needed):
//!
//! ```text
//! Software\Clients\StartMenuInternet\BrowserDispatcher
//!   (Default)                    = "Browser Dispatcher"
//!   Capabilities\ApplicationName / ApplicationDescription / ApplicationIcon / StartMenu
//!   Capabilities\UrlAssociations\http|https = BrowserDispatcherURL
//!   DefaultIcon                  = "<exe>,0"
//!   InstallInfo\ReinstallCommand, IconsVisible
//!   shell\open\command           = "\"<exe>\" \"%1\""
//! Software\Classes\BrowserDispatcherURL      (ProgID)
//!   (Default) = "Browser Dispatcher URL"
//!   URL Protocol = ""
//!   DefaultIcon / shell\open\command
//! ```
//!
//! Windows 8+ protects the `UserChoice` key with a hash, so the final
//! "set as default" step must be done by the user in Settings
//! (`ms-settings:defaultapps`); `install` opens it automatically.

use std::io;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::InstallReport;
use crate::check::Check;
use crate::platform::{DISPLAY_NAME, EXE_NAME};

const CLIENT_KEY: &str = r"Software\Clients\StartMenuInternet\BrowserDispatcher";
const PROG_ID: &str = "BrowserDispatcherURL";

pub fn find_browser_impl(id: &str) -> Option<PathBuf> {
    for tmpl in install_candidates(id) {
        if let Some(p) = expand_env(tmpl)
            && p.is_file()
        {
            return Some(p);
        }
    }
    app_paths_lookup(id)
}

fn install_candidates(id: &str) -> &'static [&'static str] {
    match id {
        "firefox" => &[
            r"%ProgramFiles%\Mozilla Firefox\firefox.exe",
            r"%ProgramFiles(x86)%\Mozilla Firefox\firefox.exe",
            r"%LocalAppData%\Mozilla Firefox\firefox.exe",
        ],
        "chrome" => &[
            r"%ProgramFiles%\Google\Chrome\Application\chrome.exe",
            r"%ProgramFiles(x86)%\Google\Chrome\Application\chrome.exe",
            r"%LocalAppData%\Google\Chrome\Application\chrome.exe",
        ],
        "chromium" => &[
            r"%LocalAppData%\Chromium\Application\chrome.exe",
            r"%ProgramFiles%\Chromium\Application\chrome.exe",
        ],
        "edge" => &[
            r"%ProgramFiles(x86)%\Microsoft\Edge\Application\msedge.exe",
            r"%ProgramFiles%\Microsoft\Edge\Application\msedge.exe",
        ],
        "brave" => &[
            r"%LocalAppData%\BraveSoftware\Brave-Browser\Application\brave.exe",
            r"%ProgramFiles%\BraveSoftware\Brave-Browser\Application\brave.exe",
        ],
        "vivaldi" => &[
            r"%LocalAppData%\Vivaldi\Application\vivaldi.exe",
            r"%ProgramFiles%\Vivaldi\Application\vivaldi.exe",
        ],
        "opera" => &[
            r"%LocalAppData%\Programs\Opera\opera.exe",
            r"%ProgramFiles%\Opera\opera.exe",
            r"%ProgramFiles(x86)%\Opera\opera.exe",
        ],
        _ => &[],
    }
}

fn app_paths_exe(id: &str) -> Option<&'static str> {
    Some(match id {
        "firefox" => "firefox.exe",
        "chrome" => "chrome.exe",
        "chromium" => "chromium.exe",
        "edge" => "msedge.exe",
        "brave" => "brave.exe",
        "vivaldi" => "vivaldi.exe",
        "opera" => "opera.exe",
        _ => return None,
    })
}

fn app_paths_lookup(id: &str) -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    let exe = app_paths_exe(id)?;
    let subkey = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe}");
    for root in [
        RegKey::predef(HKEY_LOCAL_MACHINE),
        RegKey::predef(HKEY_CURRENT_USER),
    ] {
        if let Ok(k) = root.open_subkey(&subkey)
            && let Ok(path) = k.get_value::<String, _>("")
        {
            let p = PathBuf::from(&path);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn expand_env(tmpl: &str) -> Option<PathBuf> {
    let mut s = tmpl.to_string();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
        if let Ok(v) = std::env::var(var) {
            s = s.replace(&format!("%{var}%"), &v);
        }
    }
    if s.contains('%') {
        None
    } else {
        Some(PathBuf::from(s))
    }
}

pub fn stable_install_dir(_app_dir: Option<&Path>) -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(EXE_NAME)
}

pub fn install_registration(exe: &Path) -> Result<InstallReport> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let mut report = InstallReport::default();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let exe_str = exe.to_string_lossy();
    let open_cmd = format!("\"{exe_str}\" \"%1\"");
    let icon_cmd = format!("{exe_str},0");

    let set = |subkey: &str, name: &str, value: &str| -> io::Result<()> {
        let (key, _) = hkcu.create_subkey(subkey)?;
        key.set_value(name, &value)
    };

    // --- Browser client registration ---
    set(CLIENT_KEY, "", DISPLAY_NAME)?;
    set(
        &format!("{CLIENT_KEY}\\Capabilities"),
        "ApplicationName",
        DISPLAY_NAME,
    )?;
    set(
        &format!("{CLIENT_KEY}\\Capabilities"),
        "ApplicationDescription",
        "Routes URLs to Firefox / Chrome / Edge / ... based on configurable rules.",
    )?;
    set(
        &format!("{CLIENT_KEY}\\Capabilities"),
        "ApplicationIcon",
        &icon_cmd,
    )?;
    set(
        &format!("{CLIENT_KEY}\\Capabilities"),
        "StartMenu",
        "BrowserDispatcher",
    )?;
    set(
        &format!("{CLIENT_KEY}\\Capabilities\\UrlAssociations"),
        "http",
        PROG_ID,
    )?;
    set(
        &format!("{CLIENT_KEY}\\Capabilities\\UrlAssociations"),
        "https",
        PROG_ID,
    )?;
    set(&format!("{CLIENT_KEY}\\DefaultIcon"), "", &icon_cmd)?;
    set(
        &format!("{CLIENT_KEY}\\shell\\open\\command"),
        "",
        &open_cmd,
    )?;

    // --- InstallInfo (optional, helps some validators) ---
    set(
        &format!("{CLIENT_KEY}\\InstallInfo"),
        "ReinstallCommand",
        &format!("\"{exe_str}\" install --no-wizard"),
    )?;
    {
        let (key, _) = hkcu.create_subkey(format!("{CLIENT_KEY}\\InstallInfo"))?;
        key.set_value("IconsVisible", &1u32)?;
    }

    // --- ProgID / URL protocol handler ---
    set(
        &format!(r"Software\Classes\{PROG_ID}"),
        "",
        "Browser Dispatcher URL",
    )?;
    set(&format!(r"Software\Classes\{PROG_ID}"), "URL Protocol", "")?;
    set(
        &format!(r"Software\Classes\{PROG_ID}\DefaultIcon"),
        "",
        &icon_cmd,
    )?;
    set(
        &format!(r"Software\Classes\{PROG_ID}\shell\open\command"),
        "",
        &open_cmd,
    )?;

    report
        .automated
        .push("Registered browser client + http/https handler in HKCU".to_string());

    // Windows guards UserChoice with a hash we cannot forge, so guide the user.
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "", "ms-settings:defaultapps"])
        .spawn();
    report.manual.push(
        "In the Settings page that just opened, choose \"Browser Dispatcher\" and click \
         \"Set default\" (Windows requires this step to be done by hand)."
            .to_string(),
    );
    Ok(report)
}

pub fn uninstall_registration() -> Result<()> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    delete_tree(&hkcu, CLIENT_KEY);
    delete_tree(&hkcu, &format!(r"Software\Classes\{PROG_ID}"));
    Ok(())
}

fn delete_tree(parent: &winreg::RegKey, subkey: &str) {
    if let Ok(k) = parent.open_subkey(subkey) {
        for child in k.enum_keys().flatten() {
            delete_tree(&k, &child);
        }
    }
    let _ = parent.delete_subkey(subkey);
}

pub fn registration_checks() -> Vec<Check> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let mut checks = Vec::new();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let stable = super::stable_exe_path(None);

    // 1. Registered as a browser client?
    let registered = hkcu
        .open_subkey(format!("{CLIENT_KEY}\\shell\\open\\command"))
        .ok()
        .and_then(|k| k.get_value::<String, _>("").ok());
    match registered {
        Some(cmd) if cmd.contains(stable.to_string_lossy().as_ref()) => {
            checks.push(Check::pass(
                "windows: browser registration",
                format!("HKCU\\{CLIENT_KEY} -> {}", stable.display()),
            ));
        }
        Some(cmd) => checks.push(Check::fail(
            "windows: browser registration",
            format!(
                "registered command points elsewhere ({cmd}); run `browser-dispatcher install` again"
            ),
        )),
        None => checks.push(Check::fail(
            "windows: browser registration",
            format!("HKCU\\{CLIENT_KEY} missing; run `browser-dispatcher install`"),
        )),
    }

    // 2. Active default for http/https (UserChoice)?
    for scheme in ["http", "https"] {
        let path = format!(
            r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\{scheme}\UserChoice"
        );
        let prog_id = hkcu
            .open_subkey(path)
            .ok()
            .and_then(|k| k.get_value::<String, _>("ProgId").ok());
        match prog_id.as_deref() {
            Some(p) if p == PROG_ID => checks.push(Check::pass(
                format!("windows: default for {scheme}"),
                format!("UserChoice ProgId = {PROG_ID}"),
            )),
            other => checks.push(Check::warn(
                format!("windows: default for {scheme}"),
                format!(
                    "current default is {}; open Settings > Default apps and pick \
                     \"Browser Dispatcher\" (Windows only allows manual selection)",
                    other.unwrap_or("<none>")
                ),
            )),
        }
    }

    checks
}

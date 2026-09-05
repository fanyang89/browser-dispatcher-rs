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
//! "set as default" step must be done by the user in Settings. On current
//! Windows 11 builds, `install` opens the app-specific page with
//! `ms-settings:defaultapps?registeredAppUser=Browser%20Dispatcher`.

use std::ffi::c_void;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::InstallReport;
use crate::check::Check;
use crate::platform::{DISPLAY_NAME, EXE_NAME};

const CLIENT_NAME: &str = "BrowserDispatcher";
const CLIENT_KEY: &str = r"Software\Clients\StartMenuInternet\BrowserDispatcher";
const CAPABILITIES_KEY: &str = r"Software\Clients\StartMenuInternet\BrowserDispatcher\Capabilities";
const REGISTERED_APPLICATIONS_KEY: &str = r"Software\RegisteredApplications";
const REGISTERED_APP_NAME: &str = DISPLAY_NAME;
const PROG_ID: &str = "BrowserDispatcherURL";
const DEFAULT_APPS_URI: &str = "ms-settings:defaultapps?registeredAppUser=Browser%20Dispatcher";

const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;
const SHCNF_IDLIST: u32 = 0;

#[link(name = "shell32")]
unsafe extern "system" {
    fn SHChangeNotify(event_id: i32, flags: u32, item1: *const c_void, item2: *const c_void);
}

fn notify_association_changed() {
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

fn open_default_apps_settings() {
    if std::process::Command::new("explorer.exe")
        .arg(DEFAULT_APPS_URI)
        .spawn()
        .is_err()
    {
        // Older Windows builds do not support the app-specific query string.
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", "ms-settings:defaultapps"])
            .spawn();
    }
}

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
    set(CAPABILITIES_KEY, "ApplicationName", REGISTERED_APP_NAME)?;
    set(
        CAPABILITIES_KEY,
        "ApplicationDescription",
        "Routes URLs to Firefox / Chrome / Edge / ... based on configurable rules.",
    )?;
    set(CAPABILITIES_KEY, "ApplicationIcon", &icon_cmd)?;
    set(
        &format!("{CAPABILITIES_KEY}\\Startmenu"),
        "StartMenuInternet",
        CLIENT_NAME,
    )?;
    set(
        &format!("{CAPABILITIES_KEY}\\UrlAssociations"),
        "http",
        PROG_ID,
    )?;
    set(
        &format!("{CAPABILITIES_KEY}\\UrlAssociations"),
        "https",
        PROG_ID,
    )?;

    // Required for the Windows 11 app-specific Default Apps settings page.
    // The value name must match Capabilities\\ApplicationName.
    set(
        REGISTERED_APPLICATIONS_KEY,
        REGISTERED_APP_NAME,
        CAPABILITIES_KEY,
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

    notify_association_changed();
    report.automated.push(
        "Registered browser client + http/https handler in HKCU, including \
         Software\\RegisteredApplications"
            .to_string(),
    );

    // Windows guards UserChoice with a hash we cannot forge, so guide the user.
    open_default_apps_settings();
    report.manual.push(
        "In the app-specific Settings page that just opened, click \"Set default\". \
         Windows requires this confirmation to be done by the user."
            .to_string(),
    );
    Ok(report)
}

pub fn uninstall_registration() -> Result<()> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    delete_tree(&hkcu, CLIENT_KEY);
    delete_tree(&hkcu, &format!(r"Software\Classes\{PROG_ID}"));
    if let Ok(key) = hkcu.open_subkey_with_flags(REGISTERED_APPLICATIONS_KEY, KEY_SET_VALUE) {
        let _ = key.delete_value(REGISTERED_APP_NAME);
    }
    notify_association_changed();
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

    // 2. Visible in the Windows 11 Default Apps list?
    let capabilities = hkcu
        .open_subkey(REGISTERED_APPLICATIONS_KEY)
        .ok()
        .and_then(|k| k.get_value::<String, _>(REGISTERED_APP_NAME).ok());
    match capabilities.as_deref() {
        Some(path) if path.eq_ignore_ascii_case(CAPABILITIES_KEY) => checks.push(Check::pass(
            "windows: RegisteredApplications",
            format!("{REGISTERED_APP_NAME} -> {CAPABILITIES_KEY}"),
        )),
        other => checks.push(Check::fail(
            "windows: RegisteredApplications",
            format!(
                "expected {REGISTERED_APP_NAME} -> {CAPABILITIES_KEY}, found {}; \
                 run `browser-dispatcher install` again",
                other.unwrap_or("<missing>")
            ),
        )),
    }

    // 3. Active default for http/https (UserChoice)?
    for scheme in ["http", "https"] {
        let path = format!(
            r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\{scheme}\UserChoice"
        );
        let prog_id = hkcu
            .open_subkey(path)
            .ok()
            .and_then(|k| k.get_value::<String, _>("ProgId").ok());
        match prog_id.as_deref() {
            Some(p) if p.eq_ignore_ascii_case(PROG_ID) => checks.push(Check::pass(
                format!("windows: default for {scheme}"),
                format!("UserChoice ProgId = {PROG_ID}"),
            )),
            other => checks.push(Check::warn(
                format!("windows: default for {scheme}"),
                format!(
                    "current default is {}; open {DEFAULT_APPS_URI} and click \"Set default\" \
                     (Windows requires user confirmation)",
                    other.unwrap_or("<none>")
                ),
            )),
        }
    }

    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_settings_uri_targets_per_user_registration() {
        assert_eq!(
            DEFAULT_APPS_URI,
            "ms-settings:defaultapps?registeredAppUser=Browser%20Dispatcher"
        );
        assert_eq!(REGISTERED_APP_NAME, DISPLAY_NAME);
    }

    #[test]
    fn registered_application_points_to_capabilities() {
        assert_eq!(
            CAPABILITIES_KEY,
            r"Software\Clients\StartMenuInternet\BrowserDispatcher\Capabilities"
        );
    }
}

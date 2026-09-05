//! macOS: default browser registration via an AppleScript applet bundle.
//!
//! LaunchServices delivers URL opens as `GURL` Apple Events rather than
//! ordinary command-line arguments. `install` therefore creates a tiny
//! `~/Applications/BrowserDispatcher.app` with the system `osacompile` tool.
//! Its `on open location` handler safely invokes the stable Rust CLI copy.
//! The applet is an LSUIElement (no Dock icon) and declares http/https URL
//! schemes in Info.plist.

use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::InstallReport;
use crate::check::Check;
use crate::platform::{DISPLAY_NAME, EXE_NAME};

pub const BUNDLE_ID: &str = "com.browser-dispatcher.app";
const BUNDLE_NAME: &str = "BrowserDispatcher.app";

// ---------------------------------------------------------------------------
// CoreFoundation / CoreServices FFI (minimal, no extra crates)
// ---------------------------------------------------------------------------

type CFStringRef = *const c_void;
const KCF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(
        alloc: *const c_void,
        c_str: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFRelease(cf: *const c_void);
    fn CFStringGetCStringPtr(the_string: CFStringRef, encoding: u32) -> *const c_char;
    fn CFStringGetLength(the_string: CFStringRef) -> isize;
    fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut c_char,
        buffer_size: isize,
        encoding: u32,
    ) -> u8;
}

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn LSSetDefaultHandlerForURLScheme(
        in_url_scheme: CFStringRef,
        in_handler_bundle_id: CFStringRef,
    ) -> i32;
    fn LSCopyDefaultHandlerForURLScheme(in_url_scheme: CFStringRef) -> CFStringRef;
}

struct CfString(CFStringRef);

impl CfString {
    fn new(s: &str) -> Option<CfString> {
        let c = CString::new(s).ok()?;
        let cf = unsafe {
            CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), KCF_STRING_ENCODING_UTF8)
        };
        if cf.is_null() {
            None
        } else {
            Some(CfString(cf))
        }
    }

    fn as_ref(&self) -> CFStringRef {
        self.0
    }

    fn from_cf(cf: CFStringRef) -> Option<String> {
        if cf.is_null() {
            return None;
        }
        unsafe {
            let ptr = CFStringGetCStringPtr(cf, KCF_STRING_ENCODING_UTF8);
            let s = if !ptr.is_null() {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            } else {
                let len = CFStringGetLength(cf);
                let max = CFStringGetMaximumSizeForEncoding(len, KCF_STRING_ENCODING_UTF8) + 1;
                let mut buf = vec![0 as c_char; max as usize];
                if CFStringGetCString(cf, buf.as_mut_ptr(), max, KCF_STRING_ENCODING_UTF8) != 0 {
                    CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
                } else {
                    String::new()
                }
            };
            Some(s)
        }
    }
}

impl Drop for CfString {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

// ---------------------------------------------------------------------------
// Browser detection
// ---------------------------------------------------------------------------

fn app_candidates(id: &str) -> Option<(&'static str, &'static str)> {
    Some(match id {
        "firefox" => ("Firefox.app", "firefox"),
        "chrome" => ("Google Chrome.app", "Google Chrome"),
        "chromium" => ("Chromium.app", "Chromium"),
        "edge" => ("Microsoft Edge.app", "Microsoft Edge"),
        "brave" => ("Brave Browser.app", "Brave Browser"),
        "vivaldi" => ("Vivaldi.app", "Vivaldi"),
        "opera" => ("Opera.app", "Opera"),
        "safari" => ("Safari.app", "Safari"),
        _ => return None,
    })
}

pub fn find_browser_impl(id: &str) -> Option<PathBuf> {
    let (app, exec) = app_candidates(id)?;
    let home = dirs::home_dir()?;
    for base in [
        PathBuf::from("/Applications"),
        home.join("Applications"),
        PathBuf::from("/System/Applications"),
    ] {
        let p = base.join(app).join("Contents/MacOS").join(exec);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Applet bundle & registration
// ---------------------------------------------------------------------------

pub fn stable_install_dir(_app_dir: Option<&Path>) -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(EXE_NAME)
}

fn bundle_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Applications")
        .join(BUNDLE_NAME)
}

fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn applet_script(exe: &Path) -> String {
    format!(
        "on open location theURL\n\
         \tset cliPath to {}\n\
         \tdo shell script quoted form of cliPath & \" \" & quoted form of theURL\n\
         end open location\n",
        applescript_string(&exe.to_string_lossy())
    )
}

fn info_plist(version: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleName</key>
    <string>{DISPLAY_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>{DISPLAY_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>{BUNDLE_ID}</string>
    <key>CFBundleExecutable</key>
    <string>applet</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleSignature</key>
    <string>aplt</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSAppleScriptEnabled</key>
    <true/>
    <key>OSAAppletShowStartupScreen</key>
    <false/>
    <key>CFBundleURLTypes</key>
    <array>
        <dict>
            <key>CFBundleURLName</key>
            <string>{BUNDLE_ID} http/https handler</string>
            <key>CFBundleURLSchemes</key>
            <array>
                <string>http</string>
                <string>https</string>
            </array>
            <key>CFBundleURLRole</key>
            <string>Viewer</string>
        </dict>
    </array>
</dict>
</plist>
"#
    )
}

pub fn install_registration(exe: &Path) -> Result<InstallReport> {
    let mut report = InstallReport::default();
    let bundle = bundle_path();
    if let Some(parent) = bundle.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if bundle.exists() {
        std::fs::remove_dir_all(&bundle)
            .with_context(|| format!("failed to replace {}", bundle.display()))?;
    }

    let output = std::process::Command::new("/usr/bin/osacompile")
        .arg("-o")
        .arg(&bundle)
        .arg("-e")
        .arg(applet_script(exe))
        .output()
        .context("failed to run /usr/bin/osacompile")?;
    if !output.status.success() {
        bail!(
            "osacompile failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let plist = bundle.join("Contents/Info.plist");
    std::fs::write(&plist, info_plist(env!("CARGO_PKG_VERSION")))
        .with_context(|| format!("failed to write {}", plist.display()))?;

    // Ad-hoc sign after modifying Info.plist. Failure is not fatal for every
    // macOS release, but users get an explicit warning.
    let sign_ok = std::process::Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(&bundle)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !sign_ok {
        report
            .manual
            .push("Ad-hoc codesigning failed; macOS may reject the applet bundle.".to_string());
    }

    // Refresh LaunchServices' knowledge of the bundle.
    let lsregister = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    if Path::new(lsregister).exists() {
        let _ = std::process::Command::new(lsregister)
            .arg("-f")
            .arg(&bundle)
            .status();
    }
    report
        .automated
        .push(format!("Created URL-handler applet {}", bundle.display()));

    // Make it the default handler for http/https.
    let mut set_ok = true;
    for scheme in ["http", "https"] {
        let (Some(s), Some(b)) = (CfString::new(scheme), CfString::new(BUNDLE_ID)) else {
            set_ok = false;
            break;
        };
        let status = unsafe { LSSetDefaultHandlerForURLScheme(s.as_ref(), b.as_ref()) };
        if status == 0 {
            report
                .automated
                .push(format!("Set default handler for {scheme} -> {BUNDLE_ID}"));
        } else {
            set_ok = false;
            report.manual.push(format!(
                "LSSetDefaultHandlerForURLScheme({scheme}) failed with status {status}"
            ));
        }
    }
    if !set_ok {
        report.manual.push(
            "Open System Settings > Desktop & Dock > Default web browser and pick \
             \"Browser Dispatcher\"."
                .to_string(),
        );
    }
    Ok(report)
}

pub fn uninstall_registration() -> Result<()> {
    let bundle = bundle_path();
    if bundle.exists() {
        std::fs::remove_dir_all(&bundle)
            .with_context(|| format!("failed to remove {}", bundle.display()))?;
    }
    Ok(())
}

pub fn registration_checks() -> Vec<Check> {
    let mut checks = Vec::new();
    let bundle = bundle_path();
    let plist = bundle.join("Contents/Info.plist");
    let script = bundle.join("Contents/Resources/Scripts/main.scpt");
    let applet = bundle.join("Contents/MacOS/applet");
    let plist_text = std::fs::read_to_string(&plist).ok();

    match plist_text {
        Some(t)
            if t.contains("<string>http</string>")
                && t.contains("<string>https</string>")
                && t.contains("<string>applet</string>")
                && script.is_file()
                && applet.is_file() =>
        {
            checks.push(Check::pass(
                "macos: applet bundle",
                format!("{} (GURL handler, LSUIElement)", bundle.display()),
            ));
        }
        _ => checks.push(Check::fail(
            "macos: applet bundle",
            format!(
                "{} is missing or incomplete; run `browser-dispatcher install`",
                bundle.display()
            ),
        )),
    }

    for scheme in ["http", "https"] {
        let current = CfString::new(scheme).and_then(|s| {
            let cf = unsafe { LSCopyDefaultHandlerForURLScheme(s.as_ref()) };
            let out = CfString::from_cf(cf);
            if !cf.is_null() {
                unsafe { CFRelease(cf) };
            }
            out
        });
        match current.as_deref() {
            Some(id) if id.eq_ignore_ascii_case(BUNDLE_ID) => checks.push(Check::pass(
                format!("macos: default for {scheme}"),
                format!("handler = {BUNDLE_ID}"),
            )),
            other => checks.push(Check::warn(
                format!("macos: default for {scheme}"),
                format!(
                    "current handler is {}; set the default web browser in \
                     System Settings > Desktop & Dock",
                    other.unwrap_or("<none>")
                ),
            )),
        }
    }
    checks
}

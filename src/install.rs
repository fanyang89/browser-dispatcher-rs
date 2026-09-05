//! `install` and `uninstall` command flows.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config;
use crate::platform;
use crate::util;
use crate::wizard;

pub struct InstallOptions<'a> {
    pub no_wizard: bool,
    pub default_browser: Option<&'a str>,
}

pub fn run(args: InstallOptions, cfg_path: Option<&Path>) -> Result<()> {
    let InstallOptions {
        no_wizard,
        default_browser,
    } = args;

    println!(
        "{} {} install ({})",
        platform::DISPLAY_NAME,
        env!("CARGO_PKG_VERSION"),
        platform::os_name()
    );

    // 1. Copy this binary to a stable location; registration points there so
    //    it survives `cargo clean` and arbitrary working directories.
    let current = std::env::current_exe().context("failed to locate the running executable")?;
    let stable = platform::stable_exe_path(None);
    if util::copy_if_needed(&current, &stable)? {
        println!("Installed binary: {}", stable.display());
    } else {
        println!("Binary already up to date: {}", stable.display());
    }

    #[cfg(windows)]
    install_windows_handler(&current)?;

    // Make `browser-dispatcher <cmd>` convenient on unix if ~/.local/bin exists.
    #[cfg(unix)]
    link_into_local_bin(&stable);

    // 2. Default config.
    let path = cfg_path
        .map(PathBuf::from)
        .unwrap_or_else(config::default_path);
    let mut created = false;
    if !path.exists() {
        if let Some(id) = default_browser {
            config::require_browser(&empty_config(), id)
                .with_context(|| format!("--default-browser {id} is not a known browser id"))?;
        }
        let preferred = default_browser
            .map(|s| s.to_string())
            .or_else(|| {
                platform::detect_all()
                    .first()
                    .map(|(def, _)| def.id.to_string())
            })
            .unwrap_or_else(|| "firefox".to_string());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, config::default_config_toml(&preferred))
            .with_context(|| format!("failed to write {}", path.display()))?;
        created = true;
        println!("Created default config: {}", path.display());
    } else {
        println!("Using existing config: {}", path.display());
        if let Some(id) = default_browser {
            let mut cfg = config::Config::load(&path)?;
            config::require_browser(&cfg, id)?;
            cfg.default_browser = id.to_string();
            cfg.save(&path)?;
            println!("Set default browser to '{id}'");
        }
    }

    // 3. Register with the OS.
    let report = platform::install_registration(&stable)?;
    for step in &report.automated {
        println!("  \u{2714} {step}");
    }
    for step in &report.manual {
        println!("  ! {step}");
    }

    // 4. Wizard.
    if no_wizard {
        println!(
            "\nNext: edit {} to add rules, then run `browser-dispatcher doctor`.",
            path.display()
        );
    } else if util::stdin_is_tty() && util::stdout_is_tty() {
        if created {
            println!(
                "\nThe default config contains two example rules; the wizard can adjust them."
            );
        }
        wizard::run(&path)?;
        println!("Next: run `browser-dispatcher doctor` to verify everything works.");
    } else {
        println!("\nNo interactive terminal; skipped the wizard.");
        println!(
            "Next: edit {} to add rules, then run `browser-dispatcher doctor`.",
            path.display()
        );
    }
    Ok(())
}

fn empty_config() -> config::Config {
    config::Config {
        version: config::CONFIG_VERSION,
        default_browser: String::new(),
        rules: Vec::new(),
        browsers: Default::default(),
    }
}

#[cfg(windows)]
fn install_windows_handler(current_cli: &Path) -> Result<()> {
    let source = current_cli.with_file_name(platform::HANDLER_EXE_FILE_NAME);
    let destination = platform::stable_handler_exe_path(None);
    if !source.is_file() {
        anyhow::bail!(
            "Windows URL handler is missing at {}; install both binaries with \
             `cargo install --path .` or use the release archive",
            source.display()
        );
    }
    if util::copy_if_needed(&source, &destination)? {
        println!("Installed URL handler: {}", destination.display());
    } else {
        println!("URL handler already up to date: {}", destination.display());
    }
    Ok(())
}

/// Best-effort symlink into ~/.local/bin when that directory already exists.
#[cfg(unix)]
fn link_into_local_bin(stable: &Path) {
    let Some(home) = dirs::home_dir() else { return };
    let bin = home.join(".local/bin");
    if !bin.is_dir() {
        return;
    }
    let link = bin.join(platform::EXE_NAME);
    let existing = std::fs::read_link(&link).ok();
    if existing.as_deref() == Some(stable) {
        return;
    }
    let _ = std::fs::remove_file(&link);
    if std::os::unix::fs::symlink(stable, &link).is_ok() {
        println!("Symlinked: {} -> {}", link.display(), stable.display());
    }
}

pub fn uninstall() -> Result<()> {
    println!(
        "{} uninstall ({})",
        platform::DISPLAY_NAME,
        platform::os_name()
    );
    platform::uninstall_registration()?;
    println!("  \u{2714} registration removed");

    let stable_dir = platform::stable_install_dir(None);
    #[cfg(unix)]
    {
        if std::fs::remove_dir_all(&stable_dir).is_ok() {
            println!("  \u{2714} removed {}", stable_dir.display());
        }
    }
    #[cfg(windows)]
    {
        // The running copy may be in use; ignore failures and tell the user.
        let exe = stable_dir.join(format!("{}.exe", platform::EXE_NAME));
        if std::fs::remove_dir_all(&stable_dir).is_ok() {
            println!("  \u{2714} removed {}", stable_dir.display());
        } else {
            println!(
                "  ! could not remove {}; delete it manually after closing terminals",
                exe.display()
            );
        }
    }
    #[cfg(not(unix))]
    {
        let _ = stable_dir;
    }

    // Uninstalling on Windows/macOS cannot restore the previous default browser.
    #[cfg(any(windows, target_os = "macos"))]
    println!("  ! remember to pick a different default browser in your OS settings");

    println!(
        "Config kept at {}; delete it manually if you no longer need it.",
        config::default_path().display()
    );
    Ok(())
}

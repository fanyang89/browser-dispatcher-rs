//! `doctor`: verify config, browsers, registration, and dispatch behavior.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::check::{self, Check};
use crate::config::{self, Config};
use crate::dispatch::{self, Engine};
use crate::platform;
use crate::util;

/// Returns `true` when no check failed.
pub fn run(cfg_path: Option<&Path>) -> Result<bool> {
    println!(
        "{} doctor — {} {}, {}",
        platform::DISPLAY_NAME,
        platform::os_name(),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH
    );

    let path = cfg_path
        .map(PathBuf::from)
        .unwrap_or_else(config::default_path);
    let mut checks: Vec<Check> = Vec::new();

    // --- Config -------------------------------------------------------------
    let cfg: Option<Config> = match config::Config::load(&path) {
        Ok(cfg) => {
            checks.push(Check::pass("config: file", path.display().to_string()));
            checks.extend(cfg.validate());
            Some(cfg)
        }
        Err(e) => {
            checks.push(Check::fail(
                "config: file",
                format!(
                    "{} ({e:#}); run `browser-dispatcher install`",
                    path.display()
                ),
            ));
            None
        }
    };

    // --- Browsers -----------------------------------------------------------
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    if let Some(cfg) = &cfg {
        referenced.insert(cfg.default_browser.trim().to_string());
        for r in &cfg.rules {
            referenced.insert(r.browser.trim().to_string());
        }
    }
    if referenced.is_empty() {
        checks.push(Check::warn("browsers", "nothing configured yet"));
    } else {
        let Some(cfg) = &cfg else {
            return finish(checks);
        };
        for id in &referenced {
            match dispatch::resolve_browser(cfg, id) {
                Ok(rb) => checks.push(Check::pass(
                    format!("browser: {id}"),
                    rb.path.display().to_string(),
                )),
                Err(e) => checks.push(Check::fail(format!("browser: {id}"), format!("{e:#}"))),
            }
        }
    }

    // --- OS registration ----------------------------------------------------
    checks.extend(platform::registration_checks());

    // --- Installed binary freshness -----------------------------------------
    match std::env::current_exe() {
        Ok(current) => {
            let stable = platform::stable_exe_path(None);
            if current != stable {
                let cur_len = std::fs::metadata(&current).map(|m| m.len()).unwrap_or(0);
                let stb_len = std::fs::metadata(&stable).map(|m| m.len()).unwrap_or(0);
                if cur_len != stb_len {
                    checks.push(Check::warn(
                        "binary: installed copy",
                        format!(
                            "{} differs from the running {}; run `browser-dispatcher install` \
                             to refresh it",
                            stable.display(),
                            current.display()
                        ),
                    ));
                } else {
                    checks.push(Check::pass(
                        "binary: installed copy",
                        stable.display().to_string(),
                    ));
                }
            } else {
                checks.push(Check::pass(
                    "binary: installed copy",
                    stable.display().to_string(),
                ));
            }
        }
        Err(e) => checks.push(Check::warn(
            "binary: installed copy",
            format!("current_exe failed: {e}"),
        )),
    }

    // --- Installed CLI self-test ---------------------------------------------
    if cfg.is_some() {
        let stable = platform::stable_exe_path(None);
        if stable.is_file() {
            match std::process::Command::new(&stable)
                .args(["dispatch", "--dry-run", "--config"])
                .arg(&path)
                .arg("https://example.org/")
                .output()
            {
                Ok(output) if output.status.success() => checks.push(Check::pass(
                    "binary: dispatch self-test",
                    String::from_utf8_lossy(&output.stdout).trim().to_string(),
                )),
                Ok(output) => checks.push(Check::fail(
                    "binary: dispatch self-test",
                    format!(
                        "exit {}; {}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr).trim()
                    ),
                )),
                Err(e) => checks.push(Check::fail(
                    "binary: dispatch self-test",
                    format!("failed to execute {}: {e}", stable.display()),
                )),
            }
        } else {
            checks.push(Check::fail(
                "binary: dispatch self-test",
                format!(
                    "{} missing; run `browser-dispatcher install`",
                    stable.display()
                ),
            ));
        }
    }

    // --- Dispatch dry-runs ----------------------------------------------------
    if let Some(cfg) = &cfg {
        let (engine, warnings) = Engine::new(cfg);
        for w in &warnings {
            checks.push(Check::warn("rules", w.clone()));
        }
        println!("\nRule preview (what would happen):");
        for url in sample_urls(cfg) {
            let d = engine.decide(&url);
            println!(
                "  {} -> {} ({})",
                url.as_str(),
                d.browser_id,
                d.rule_name.as_deref().unwrap_or("default")
            );
        }
    }

    // --- Dispatch log ---------------------------------------------------------
    let log = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("dispatch.log");
    if let Ok(content) = std::fs::read_to_string(&log) {
        let tail: Vec<&str> = content.lines().rev().take(5).collect();
        println!("\nRecent dispatch log ({}):", log.display());
        for line in tail.iter().rev() {
            println!("  {line}");
        }
    }

    finish(checks)
}

/// Derive a few representative URLs from the rules for the preview.
fn sample_urls(cfg: &Config) -> Vec<url::Url> {
    let mut urls = Vec::new();
    let push = |host: String, urls: &mut Vec<url::Url>| {
        if let Ok(u) = url::Url::parse(&format!("https://{host}/"))
            && !urls.contains(&u)
        {
            urls.push(u);
        }
    };
    for rule in cfg.rules.iter().take(8) {
        if let Some(pat) = rule.host.first() {
            let host = pat
                .trim_start_matches("*.")
                .trim_start_matches("*.")
                .replace('*', "example");
            push(host, &mut urls);
        } else if rule.url_regex.is_some() {
            push("example.com".to_string(), &mut urls);
        }
    }
    push("example.org".to_string(), &mut urls);
    urls
}

fn finish(checks: Vec<Check>) -> Result<bool> {
    let color = util::use_color();
    println!();
    for c in &checks {
        c.print(color);
    }
    let (pass, warn, fail) = check::summarize(&checks);
    println!("\nSummary: {pass} passed, {warn} warnings, {fail} failures");
    Ok(fail == 0)
}

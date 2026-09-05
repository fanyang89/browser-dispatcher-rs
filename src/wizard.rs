//! Interactive setup wizard shown by `install`.

use std::io::IsTerminal;
use std::path::Path;

use anyhow::Result;
use dialoguer::{Confirm, Input, Select};

use crate::config::{self, Config, Rule};
use crate::dispatch;
use crate::platform;
use crate::util;

/// A browser choice offered in the wizard: `Label (id) — path`.
struct Choice {
    id: String,
    label: String,
}

fn browser_choices(cfg: &Config) -> Vec<Choice> {
    let mut out: Vec<Choice> = platform::detect_all()
        .into_iter()
        .map(|(def, path)| Choice {
            id: def.id.to_string(),
            label: format!("{} ({}) — {}", def.name, def.id, path.display()),
        })
        .collect();
    // Custom browsers configured with an explicit path.
    for (id, bc) in &cfg.browsers {
        if let Some(p) = &bc.path {
            out.push(Choice {
                id: id.clone(),
                label: format!("{id} (custom) — {}", p.display()),
            });
        }
    }
    out
}

fn select_browser(prompt: &str, choices: &[Choice], preselect: Option<&str>) -> Result<String> {
    if choices.is_empty() {
        let id: String = Input::new()
            .with_prompt(prompt)
            .with_initial_text(preselect.unwrap_or("firefox").to_string())
            .interact_text()?;
        return Ok(id);
    }
    let default = choices
        .iter()
        .position(|c| Some(c.id.as_str()) == preselect)
        .unwrap_or(0);
    let idx = Select::new()
        .with_prompt(prompt)
        .items(&choices.iter().map(|c| c.label.as_str()).collect::<Vec<_>>())
        .default(default)
        .interact()?;
    Ok(choices[idx].id.clone())
}

/// If the input looks like a URL, extract its host; otherwise return as-is.
fn extract_host_pattern(input: &str) -> String {
    match dispatch::normalize_url(input) {
        Ok(u) => u.host_str().unwrap_or(input).to_string(),
        Err(_) => input.trim().to_string(),
    }
}

pub fn run(config_path: &Path) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!("wizard needs an interactive terminal; skipping");
        return Ok(());
    }
    println!("\n--- Setup wizard ---");

    let mut cfg = config::Config::load(config_path)?;
    let mut choices = browser_choices(&cfg);

    // Default browser.
    println!("\n1/2: pick the fallback browser (used when no rule matches).");
    cfg.default_browser = select_browser(
        "Default browser",
        &choices,
        Some(cfg.default_browser.trim()),
    )?;
    choices = browser_choices(&cfg); // refresh labels in case a custom entry was typed

    // Rules.
    println!("\n2/2: routing rules (first match wins).");
    if !cfg.rules.is_empty() {
        println!("Existing rules:");
        for r in &cfg.rules {
            let matcher = r
                .host
                .first()
                .cloned()
                .or_else(|| r.url_regex.clone())
                .unwrap_or_else(|| "<empty>".to_string());
            println!(
                "  - {} : {matcher} -> {}{}",
                r.name,
                r.browser,
                if r.private { " (private)" } else { "" }
            );
        }
    }

    loop {
        let add = Confirm::new()
            .with_prompt(if cfg.rules.is_empty() {
                "Add a routing rule?"
            } else {
                "Add another rule?"
            })
            .default(!cfg.rules.is_empty())
            .interact()?;
        if !add {
            break;
        }

        let raw: String = Input::new()
            .with_prompt("Site (host glob like *.company.com, or paste a URL)")
            .interact_text()?;
        if raw.trim().is_empty() {
            continue;
        }
        let host = extract_host_pattern(&raw);
        println!("  will match host: {host}");

        let browser = select_browser("Open with", &choices, Some(&cfg.default_browser))?;
        let private = Confirm::new()
            .with_prompt("Open in a private/incognito window?")
            .default(false)
            .interact()?;
        let default_name = util::slug(&host);
        let name: String = Input::new()
            .with_prompt("Rule name")
            .with_initial_text(default_name)
            .interact_text()?;

        cfg.rules.push(Rule {
            name: name.trim().to_string(),
            host: vec![host],
            url_regex: None,
            browser,
            private,
        });
        println!("  rule added");
    }

    cfg.save(config_path)?;
    println!("\nSaved config: {}", config_path.display());
    println!("Edit this file anytime to fine-tune rules (glob hosts, url_regex, ...).");
    Ok(())
}

//! Config file schema, loading/saving, and validation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer, Serialize};

use crate::check::Check;
use crate::platform;

pub const CONFIG_VERSION: u32 = 1;

/// Default config location, e.g. `~/.config/browser-dispatcher/config.toml` on Linux.
pub fn default_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(env!("CARGO_PKG_NAME"))
        .join("config.toml")
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    /// Fallback browser id used when no rule matches.
    pub default_browser: String,
    /// Routing rules, evaluated top to bottom; first match wins.
    #[serde(default, rename = "rule", skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    /// Optional per-browser overrides, keyed by browser id.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub browsers: HashMap<String, BrowserConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Human-readable rule name.
    pub name: String,
    /// Glob pattern(s) matched against the URL host, case-insensitively.
    /// `*.example.com` also matches `example.com` itself.
    #[serde(
        default,
        deserialize_with = "string_or_vec",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub host: Vec<String>,
    /// Optional regex matched against the full URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_regex: Option<String>,
    /// Browser id to route matching URLs to.
    pub browser: String,
    /// Open the URL in a private/incognito window.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub private: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BrowserConfig {
    /// Explicit executable path. When omitted, the browser is auto-detected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Extra CLI arguments passed before the URL.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Firefox profile name (Firefox only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Private-window flag(s) for browsers not known to the built-in catalog.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub private_args: Vec<String>,
}

/// Accept either a single string or a list of strings for `host`.
fn string_or_vec<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum SOrV {
        S(String),
        V(Vec<String>),
    }
    match SOrV::deserialize(d)? {
        SOrV::S(s) => Ok(vec![s]),
        SOrV::V(v) => Ok(v),
    }
}

impl Config {
    pub fn fallback_default() -> Config {
        let preferred = platform::detect_all()
            .first()
            .map(|(def, _)| def.id.to_string())
            .unwrap_or_else(|| "firefox".to_string());
        Config {
            version: CONFIG_VERSION,
            default_browser: preferred,
            rules: Vec::new(),
            browsers: HashMap::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        let cfg: Config = toml::from_str(&text)
            .with_context(|| format!("failed to parse config file {}", path.display()))?;
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("failed to serialize config")?;
        std::fs::write(path, text)
            .with_context(|| format!("failed to write config file {}", path.display()))?;
        Ok(())
    }

    /// Static validation that needs no filesystem access.
    pub fn validate(&self) -> Vec<Check> {
        let mut checks = Vec::new();
        let title = |what: &str| format!("config: {what}");

        if self.version != CONFIG_VERSION {
            checks.push(Check::fail(
                title("schema version"),
                format!("expected version {CONFIG_VERSION}, found {}", self.version),
            ));
        } else {
            checks.push(Check::pass(
                title("schema version"),
                format!("version {CONFIG_VERSION}"),
            ));
        }

        checks.extend(check_browser_id(
            "config: default browser",
            &self.default_browser,
            self,
        ));

        if self.rules.is_empty() {
            checks.push(Check::warn(
                title("rules"),
                "no rules defined; every URL goes to the default browser",
            ));
        }

        for rule in &self.rules {
            let t = format!("rule '{}'", rule.name);
            if rule.host.is_empty() && rule.url_regex.is_none() {
                checks.push(Check::fail(
                    format!("{t}: matcher"),
                    "rule has neither `host` nor `url_regex`; it can never match",
                ));
            }
            if let Some(src) = &rule.url_regex {
                match regex::Regex::new(src) {
                    Ok(_) => checks.push(Check::pass(format!("{t}: url_regex"), "compiles")),
                    Err(e) => {
                        checks.push(Check::fail(format!("{t}: url_regex"), e.to_string()));
                    }
                }
            }
            for pat in &rule.host {
                if pat.contains('/') {
                    checks.push(Check::warn(
                        format!("{t}: host pattern {pat:?}"),
                        "host patterns should not contain '/'; did you mean a host glob?",
                    ));
                }
            }
            checks.extend(check_browser_id(
                &format!("{t}: browser"),
                &rule.browser,
                self,
            ));
        }

        for (id, bc) in &self.browsers {
            if bc.path.is_none() && platform::catalog_def(id).is_none() {
                checks.push(Check::fail(
                    format!("config: browsers.{id}"),
                    "custom browser needs an explicit `path`",
                ));
            }
        }

        checks
    }
}

fn check_browser_id(title: &str, id: &str, cfg: &Config) -> Vec<Check> {
    let id = id.trim();
    if id.is_empty() {
        return vec![Check::fail(title.to_string(), "browser id is empty")];
    }
    if platform::catalog_def(id).is_some() || cfg.browsers.contains_key(id) {
        vec![Check::pass(
            title.to_string(),
            format!("browser id '{id}' is known"),
        )]
    } else {
        vec![Check::fail(
            title.to_string(),
            format!(
                "unknown browser id '{id}'; known: {}",
                platform::known_ids().join(", ")
            ),
        )]
    }
}

/// Require that the config default browser resolves to something usable.
pub fn require_browser(cfg: &Config, id: &str) -> Result<()> {
    if platform::catalog_def(id).is_some() || cfg.browsers.contains_key(id) {
        Ok(())
    } else {
        bail!(
            "unknown browser id '{id}'; known ids: {}",
            platform::known_ids().join(", ")
        );
    }
}

// ---------------------------------------------------------------------------
// Default config template
// ---------------------------------------------------------------------------

/// Render the commented default config written by `install`.
pub fn default_config_toml(default_browser: &str) -> String {
    let template = r#"# Browser Dispatcher configuration
#
# Rules are evaluated top to bottom; the first match wins.
# When nothing matches, `default_browser` is used.
#
# Built-in browser ids: firefox, chrome, chromium, edge, brave, vivaldi,
# opera, safari (macOS only). Custom ids work via [browsers.<id>] below.

version = 1

# Fallback browser when no rule matches.
default_browser = "{default_browser}"

# Optional per-browser overrides (any id, including custom ones):
#
# [browsers.firefox]
# path = "C:/Program Files/Mozilla Firefox/firefox.exe"  # omit to auto-detect
# args = []                 # extra CLI arguments
# profile = "work"          # Firefox profile name (firefox only)
# private_args = ["--incognito"]  # private-window flags for custom browsers
#
# [browsers.librewolf]      # any custom browser
# path = "/usr/bin/librewolf"

# `host` is a glob matched against the URL host (case-insensitive);
# "*.example.com" also matches "example.com" itself.
# `url_regex` (optional) is matched against the full URL.
# `private = true` opens the URL in a private/incognito window.
# Example rules (uncomment and edit):
#
# [[rule]]
# name = "microsoft-login"
# host = ["login.microsoftonline.com", "*.microsoft.com"]
# browser = "edge"
#
# [[rule]]
# name = "local-dev"
# url_regex = '^https://([a-z0-9-]+\.)?localhost(:\d+)?/'
# browser = "firefox"
# private = true
"#;
    template.replace("{default_browser}", default_browser)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_string_or_array_host() {
        let cfg: Config = toml::from_str(
            r#"
            version = 1
            default_browser = "firefox"

            [[rule]]
            name = "one"
            host = "a.com"
            browser = "chrome"

            [[rule]]
            name = "two"
            host = ["b.com", "*.c.com"]
            browser = "edge"
            private = true
            "#,
        )
        .unwrap();
        assert_eq!(cfg.rules[0].host, vec!["a.com".to_string()]);
        assert_eq!(cfg.rules[1].host.len(), 2);
        assert!(cfg.rules[1].private);
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = toml::from_str::<Config>(
            r#"
            version = 1
            default_browser = "firefox"
            browserss = {}
            "#,
        );
        assert!(err.is_err());
    }

    #[test]
    fn default_template_parses() {
        for id in ["firefox", "chrome"] {
            let cfg: Config = toml::from_str(&default_config_toml(id)).unwrap();
            assert_eq!(cfg.default_browser, id);
            assert_eq!(cfg.version, 1);
            assert!(cfg.rules.is_empty());
        }
    }

    #[test]
    fn roundtrip_serialization() {
        let cfg: Config = toml::from_str(&default_config_toml("firefox")).unwrap();
        let text = toml::to_string_pretty(&cfg).unwrap();
        let cfg2: Config = toml::from_str(&text).unwrap();
        assert_eq!(cfg2.default_browser, cfg.default_browser);
        assert_eq!(cfg2.rules.len(), cfg.rules.len());
    }
}

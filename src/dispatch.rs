//! URL matching engine and browser launching.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use url::Url;

use crate::config::{self, BrowserConfig, Config};
use crate::platform;
use crate::util;

// ---------------------------------------------------------------------------
// URL normalization
// ---------------------------------------------------------------------------

/// Parse a URL, assuming `https://` for bare hostnames like `example.com`.
pub fn normalize_url(input: &str) -> Result<Url> {
    let s = input.trim();
    match Url::parse(s) {
        Ok(u) => Ok(u),
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            Url::parse(&format!("https://{s}")).with_context(|| format!("invalid URL {input:?}"))
        }
        Err(e) => bail!("invalid URL {input:?}: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Host matching
// ---------------------------------------------------------------------------

/// Match a host glob pattern against a (lowercased) host.
///
/// - exact match works
/// - `*.example.com` also matches `example.com` and any depth of subdomain
/// - generic glob syntax is supported via `globset`
pub fn host_pattern_matches(pattern: &str, host: &str) -> bool {
    let pat = pattern.trim().to_ascii_lowercase();
    let host = host.trim().to_ascii_lowercase();
    if pat.is_empty() || host.is_empty() {
        return false;
    }
    if pat == host {
        return true;
    }
    if let Some(apex) = pat.strip_prefix("*.")
        && (apex == host || host.ends_with(&format!(".{apex}")))
    {
        return true;
    }
    glob_match(&pat, &host)
}

fn glob_match(pattern: &str, s: &str) -> bool {
    match globset::Glob::new(pattern) {
        Ok(g) => g.compile_matcher().is_match(s),
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Rule engine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Decision {
    pub browser_id: String,
    pub private: bool,
    /// Name of the matched rule, or `None` when the default browser was used.
    pub rule_name: Option<String>,
}

struct CompiledRule {
    rule: config::Rule,
    regex: Option<regex::Regex>,
    hosts: Vec<String>,
}

pub struct Engine {
    default: String,
    rules: Vec<CompiledRule>,
}

impl Engine {
    /// Compile a config; returns the engine plus a list of warnings
    /// (e.g. invalid regexes, which are then ignored).
    pub fn new(cfg: &Config) -> (Engine, Vec<String>) {
        let mut warnings = Vec::new();
        let mut rules = Vec::new();
        for rule in &cfg.rules {
            let regex = match &rule.url_regex {
                Some(src) => match regex::Regex::new(src) {
                    Ok(r) => Some(r),
                    Err(e) => {
                        warnings.push(format!(
                            "rule '{}': ignoring invalid url_regex: {}",
                            rule.name, e
                        ));
                        None
                    }
                },
                None => None,
            };
            if rule.host.is_empty() && regex.is_none() {
                warnings.push(format!(
                    "rule '{}': neither host nor url_regex set; the rule never matches",
                    rule.name
                ));
            }
            let hosts = rule
                .host
                .iter()
                .map(|h| h.trim().to_ascii_lowercase())
                .collect();
            rules.push(CompiledRule {
                rule: rule.clone(),
                regex,
                hosts,
            });
        }
        (
            Engine {
                default: cfg.default_browser.clone(),
                rules,
            },
            warnings,
        )
    }

    pub fn decide(&self, url: &Url) -> Decision {
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        let full = url.as_str();
        for cr in &self.rules {
            let host_ok = cr.hosts.iter().any(|p| host_pattern_matches(p, &host));
            let regex_ok = cr.regex.as_ref().is_some_and(|r| r.is_match(full));
            if host_ok || regex_ok {
                return Decision {
                    browser_id: cr.rule.browser.clone(),
                    private: cr.rule.private,
                    rule_name: Some(cr.rule.name.clone()),
                };
            }
        }
        Decision {
            browser_id: self.default.clone(),
            private: false,
            rule_name: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Browser resolution & launching
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ResolvedBrowser {
    pub id: String,
    pub path: PathBuf,
    /// Extra args from config, passed before the URL.
    pub extra_args: Vec<String>,
    /// Flags that request a private/incognito window.
    pub private_args: Vec<String>,
    /// Firefox profile name (Firefox only).
    pub profile: Option<String>,
}

/// Resolve a browser id to an executable, honoring config overrides.
pub fn resolve_browser(cfg: &Config, id: &str) -> Result<ResolvedBrowser> {
    let bc: BrowserConfig = cfg.browsers.get(id).cloned().unwrap_or_default();

    let mut resolved = if let Some(path) = bc.path.clone() {
        ResolvedBrowser {
            id: id.to_string(),
            path,
            extra_args: bc.args.clone(),
            private_args: bc.private_args.clone(),
            profile: None,
        }
    } else if let Some(path) = platform::find_browser(id) {
        ResolvedBrowser {
            id: id.to_string(),
            path,
            extra_args: bc.args.clone(),
            private_args: Vec::new(),
            profile: None,
        }
    } else {
        bail!(
            "browser '{id}' was not found on this system; add [browsers.{id}] with an explicit \
             `path` to the config file"
        );
    };

    if resolved.private_args.is_empty()
        && let Some(def) = platform::catalog_def(id)
    {
        resolved.private_args = def.private_args.iter().map(|s| s.to_string()).collect();
    }
    if id == "firefox" {
        resolved.profile = bc.profile;
    }
    Ok(resolved)
}

/// Spawn the browser for a URL.
pub fn launch(browser: &ResolvedBrowser, url: &str, private: bool) -> Result<()> {
    if private && browser.private_args.is_empty() {
        eprintln!(
            "warning: browser '{}' has no known private-window flag; opening a normal window",
            browser.id
        );
    }
    let mut cmd = Command::new(&browser.path);
    if let Some(profile) = &browser.profile {
        cmd.arg("-P").arg(profile);
    }
    if private {
        cmd.args(&browser.private_args);
    }
    cmd.args(&browser.extra_args);
    cmd.arg(url);
    cmd.spawn().with_context(|| {
        format!(
            "failed to launch {} ({})",
            browser.id,
            browser.path.display()
        )
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Dispatch command / OS handler entry point
// ---------------------------------------------------------------------------

pub fn run_dispatch(
    urls: &[String],
    dry_run: bool,
    cfg_path: Option<&std::path::Path>,
) -> Result<()> {
    if urls.is_empty() {
        bail!("no URL given");
    }
    let path = cfg_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(config::default_path);
    let cfg = match config::Config::load(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("warning: {e:#}; using built-in defaults for this dispatch");
            Config::fallback_default()
        }
    };
    let (engine, warnings) = Engine::new(&cfg);
    for w in &warnings {
        eprintln!("warning: {w}");
    }

    let mut any_fail = false;
    for raw in urls {
        let url = match normalize_url(raw) {
            Ok(u) => u,
            Err(e) => {
                eprintln!("error: {e:#}");
                util::append_log(
                    &path,
                    &format!(
                        "{} url={} error={}",
                        util::timestamp(),
                        util::single_line(raw),
                        util::single_line(&format!("{e:#}"))
                    ),
                );
                any_fail = true;
                continue;
            }
        };
        let decision = engine.decide(&url);
        let via = decision.rule_name.as_deref().unwrap_or("default");

        if dry_run {
            println!("{} -> {} ({})", url.as_str(), decision.browser_id, via);
            continue;
        }

        let result = resolve_and_launch(&cfg, &decision, url.as_str());
        match &result {
            Ok(actual) => util::append_log(
                &path,
                &format!(
                    "{} url={} -> {} via={} ok",
                    util::timestamp(),
                    url.as_str(),
                    actual,
                    via
                ),
            ),
            Err(e) => {
                eprintln!("error: {e:#}");
                util::append_log(
                    &path,
                    &format!(
                        "{} url={} -> {} via={} error={}",
                        util::timestamp(),
                        url.as_str(),
                        decision.browser_id,
                        via,
                        util::single_line(&format!("{e:#}"))
                    ),
                );
            }
        }
        any_fail |= result.is_err();
    }

    if any_fail {
        bail!("dispatch finished with errors");
    }
    Ok(())
}

/// Try the matched browser, falling back to the configured default.
/// Returns the id of the browser that actually launched.
fn resolve_and_launch(cfg: &Config, decision: &Decision, url: &str) -> Result<String> {
    let attempt = |id: &str, private: bool| -> Result<String> {
        let rb = resolve_browser(cfg, id)?;
        launch(&rb, url, private)?;
        Ok(id.to_string())
    };
    attempt(&decision.browser_id, decision.private).or_else(|first| {
        if decision.browser_id != cfg.default_browser {
            eprintln!(
                "warning: {} failed ({first:#}); falling back to default browser '{}'",
                decision.browser_id, cfg.default_browser
            );
            attempt(&cfg.default_browser, false).map_err(|second| {
                second.context(format!(
                    "matched browser '{}' and fallback '{}' both failed",
                    decision.browser_id, cfg.default_browser
                ))
            })
        } else {
            Err(first)
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Rule;

    fn rule(name: &str, hosts: &[&str], browser: &str) -> config::Rule {
        Rule {
            name: name.into(),
            host: hosts.iter().map(|s| s.to_string()).collect(),
            url_regex: None,
            browser: browser.into(),
            private: false,
        }
    }

    #[test]
    fn host_matching_semantics() {
        assert!(host_pattern_matches("Example.COM", "example.com"));
        assert!(host_pattern_matches("*.example.com", "example.com"));
        assert!(host_pattern_matches("*.example.com", "a.example.com"));
        assert!(host_pattern_matches("*.example.com", "a.b.example.com"));
        assert!(!host_pattern_matches("*.example.com", "badexample.com"));
        assert!(!host_pattern_matches("*.example.com", "example.org"));
        assert!(host_pattern_matches("login.*.com", "login.foo.com"));
        assert!(!host_pattern_matches("", "example.com"));
    }

    #[test]
    fn normalize_urls() {
        assert_eq!(
            normalize_url("example.com").unwrap().as_str(),
            "https://example.com/"
        );
        assert_eq!(
            normalize_url("https://a.b").unwrap().host_str(),
            Some("a.b")
        );
        assert_eq!(normalize_url("mailto:x@y.z").unwrap().scheme(), "mailto");
    }

    #[test]
    fn engine_first_match_wins_with_regex_and_private() {
        let cfg = Config {
            version: 1,
            default_browser: "firefox".into(),
            rules: vec![
                config::Rule {
                    name: "local".into(),
                    host: vec![],
                    url_regex: Some(r"^https://localhost".into()),
                    browser: "chrome".into(),
                    private: true,
                },
                rule("ms", &["*.microsoft.com"], "edge"),
            ],
            browsers: Default::default(),
        };
        let (engine, warnings) = Engine::new(&cfg);
        assert!(warnings.is_empty());

        let d = engine.decide(&Url::parse("https://localhost:8080/x").unwrap());
        assert_eq!(d.browser_id, "chrome");
        assert!(d.private);

        let d = engine.decide(&Url::parse("https://login.microsoft.com/").unwrap());
        assert_eq!(d.browser_id, "edge");
        assert_eq!(d.rule_name.as_deref(), Some("ms"));

        let d = engine.decide(&Url::parse("https://example.org/").unwrap());
        assert_eq!(d.browser_id, "firefox");
        assert!(d.rule_name.is_none());
    }

    #[test]
    fn engine_reports_bad_regex() {
        let cfg = Config {
            version: 1,
            default_browser: "firefox".into(),
            rules: vec![config::Rule {
                name: "bad".into(),
                host: vec![],
                url_regex: Some("([".into()),
                browser: "chrome".into(),
                private: false,
            }],
            browsers: Default::default(),
        };
        let (engine, warnings) = Engine::new(&cfg);
        // Two warnings: invalid regex + rule can never match (no host either).
        assert_eq!(warnings.len(), 2);
        let d = engine.decide(&Url::parse("https://example.com/").unwrap());
        assert_eq!(d.browser_id, "firefox");
    }
}

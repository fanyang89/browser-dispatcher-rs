use std::process::Command;

#[test]
fn invalid_config_stops_dispatch_and_is_logged() {
    let cases = [
        None,
        Some("not valid TOML".to_string()),
        Some("version = 999\ndefault_browser = 'firefox'".to_string()),
        Some("version = 1\ndefault_browser = 'unknown'".to_string()),
        Some("version = 1\ndefault_browser = ' firefox '".to_string()),
        Some("version = 1\ndefault_browser = 'firefox'\n[[rule]]\nname = 'bad'\nbrowser = 'chrome'\nurl_regex = '['".to_string()),
        Some("version = 1\ndefault_browser = 'firefox'\n[[rule]]\nname = 'bad'\nbrowser = 'chrome'\nhost = '['".to_string()),
        Some("version = 1\ndefault_browser = 'firefox'\n[[rule]]\nname = 'bad'\nbrowser = 'chrome'\nhost = ''".to_string()),
    ];
    for content in cases {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("custom config.toml");
        if let Some(content) = content {
            std::fs::write(&config, content).unwrap();
        }
        let output = Command::new(env!("CARGO_BIN_EXE_browser-dispatcher"))
            .arg("--config")
            .arg(&config)
            .args(["dispatch", "--dry-run", "--", "https://example.com/"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let log = std::fs::read_to_string(dir.path().join("dispatch.log")).unwrap();
        assert!(log.contains("config_error="));
    }
}

#[test]
fn valid_custom_config_routes_without_default_config() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("custom config.toml");
    std::fs::write(&config, "version = 1\ndefault_browser = 'firefox'\n[[rule]]\nname = 'work'\nbrowser = 'chrome'\nhost = 'example.com'").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_browser-dispatcher"))
        .arg("--config")
        .arg(&config)
        .args(["dispatch", "--dry-run", "--", "https://example.com/"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("chrome (work)"));
}

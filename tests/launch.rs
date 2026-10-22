use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::{Duration, Instant},
};

fn browser() -> &'static Path {
    static BROWSER: OnceLock<PathBuf> = OnceLock::new();
    BROWSER.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap().keep();
        let executable = dir.join(format!("fake browser{}", std::env::consts::EXE_SUFFIX));
        let status = Command::new("rustc")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/browser.rs"))
            .arg("-o")
            .arg(&executable)
            .status()
            .unwrap();
        assert!(status.success());
        executable
    })
}

fn config(path: &Path, target: &Path, fallback: &Path) {
    let quote = |path: &Path| toml::Value::String(path.to_string_lossy().into_owned()).to_string();
    std::fs::write(
        path,
        format!(
            r#"version = 1
default_browser = "fallback"
[[rule]]
name = "private-work"
host = "example.com"
browser = "target"
private = true
[browsers.target]
path = {}
args = ["--label", "work profile 中文"]
private_args = ["--private"]
[browsers.fallback]
path = {}
args = ["--label", "fallback"]
private_args = ["--private"]
"#,
            quote(target),
            quote(fallback)
        ),
    )
    .unwrap();
}

fn records(dir: &Path, count: usize) -> Vec<Vec<String>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let records: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "args"))
            .map(|entry| {
                std::fs::read_to_string(entry.path())
                    .unwrap()
                    .split('\0')
                    .map(str::to_owned)
                    .collect()
            })
            .collect();
        if records.len() >= count {
            return records;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for browser arguments"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn real_launch_preserves_special_urls_and_private_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("custom config.toml");
    config(&cfg, browser(), browser());
    let urls = [
        "https://example.com/中文?q=a&next=%22quoted%22#section",
        "https://example.com/a'b?q=\"value\"&x=$HOME#fragment",
        &format!("https://example.com/long?q={}", "x".repeat(16000)),
    ];
    let output = Command::new(env!("CARGO_BIN_EXE_browser-dispatcher"))
        .arg("--config")
        .arg(&cfg)
        .args(["dispatch", "--"])
        .args(urls)
        .env("BROWSER_DISPATCHER_TEST_OUTPUT", dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = records(dir.path(), urls.len());
    for url in urls {
        let normalized = url::Url::parse(url).unwrap().to_string();
        assert!(actual.contains(&vec![
            "--private".into(),
            "--label".into(),
            "work profile 中文".into(),
            normalized
        ]));
    }
}

#[test]
fn missing_and_unlaunchable_targets_fall_back_privately() {
    for exists in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("broken.exe");
        if exists {
            std::fs::write(&target, "not an executable").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let cfg = dir.path().join("config.toml");
        config(&cfg, &target, browser());
        let output = Command::new(env!("CARGO_BIN_EXE_browser-dispatcher"))
            .arg("--config")
            .arg(&cfg)
            .arg("https://example.com/")
            .env("BROWSER_DISPATCHER_TEST_OUTPUT", dir.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            records(dir.path(), 1),
            vec![vec![
                "--private",
                "--label",
                "fallback",
                "https://example.com/"
            ]]
        );
    }
}

#[cfg(windows)]
#[test]
fn handler_reports_missing_cli_instead_of_silent_success() {
    let dir = tempfile::tempdir().unwrap();
    let handler = dir.path().join("browser-dispatcher-handler.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_browser-dispatcher-handler"), &handler).unwrap();
    let status = Command::new(handler)
        .arg("https://example.com/")
        .status()
        .unwrap();
    assert!(!status.success());
    assert!(
        std::fs::read_to_string(dir.path().join("handler.log"))
            .unwrap()
            .contains("handler error:")
    );
}

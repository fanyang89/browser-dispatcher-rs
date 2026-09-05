//! Small cross-platform helpers.

use std::io::IsTerminal;
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

pub fn stdout_is_tty() -> bool {
    std::io::stdout().is_terminal()
}

pub fn stdin_is_tty() -> bool {
    std::io::stdin().is_terminal()
}

/// Whether to emit ANSI colors on stdout.
pub fn use_color() -> bool {
    // Respect NO_COLOR and fall back to tty detection.
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    stdout_is_tty()
}

pub fn timestamp() -> String {
    humantime::format_rfc3339_millis(std::time::SystemTime::now()).to_string()
}

/// Lowercase slug suitable for a rule name.
pub fn slug(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "rule".to_string()
    } else {
        out
    }
}

#[cfg(target_os = "linux")]
pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(m) => m.is_file() && m.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(unix)]
pub fn set_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
pub fn set_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Look up `name` on PATH.
#[cfg(target_os = "linux")]
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    if name.contains('/') || name.contains('\\') {
        let p = PathBuf::from(name);
        return if is_executable(&p) { Some(p) } else { None };
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Run a helper command, ignoring failures. Returns stdout (trimmed) on success.
#[cfg(target_os = "linux")]
pub fn try_command(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Copy `src` to `dst` (creating parent dirs, preserving the executable bit).
/// Returns whether the file was actually copied.
pub fn copy_if_needed(src: &Path, dst: &Path) -> std::io::Result<bool> {
    let src_meta = std::fs::metadata(src)?;
    if let Ok(dst_meta) = std::fs::metadata(dst)
        && dst_meta.len() == src_meta.len()
        && dst_meta.modified().ok() >= src_meta.modified().ok()
    {
        return Ok(false);
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src, dst)?;
    set_executable(dst)?;
    Ok(true)
}

/// Append one line to a small log file next to `config_path`, rotating if huge.
pub fn append_log(config_path: &Path, line: &str) {
    let log = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("dispatch.log");
    if let Some(dir) = log.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Rotate when it grows past 256 KiB, keeping the last 64 KiB.
    if let Ok(meta) = std::fs::metadata(&log)
        && meta.len() > 256 * 1024
        && let Ok(content) = std::fs::read_to_string(&log)
    {
        let keep: String = content
            .lines()
            .rev()
            .take(500)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        let _ = std::fs::write(&log, keep + "\n");
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
    {
        let _ = writeln!(f, "{line}");
    }
}

pub fn single_line(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
}

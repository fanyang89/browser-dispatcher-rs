//! Console-free Windows URL handler.
//!
//! Windows decides whether to create a console before `main` runs, based on the
//! executable subsystem recorded in the PE header. Keep the primary executable
//! as a console application for `install`, `doctor`, and other CLI commands, and
//! register this small GUI-subsystem launcher for URL activations instead.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
use std::io;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use std::process::Command;

#[cfg(windows)]
const CLI_EXE_NAME: &str = "browser-dispatcher.exe";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        // Bootstrap failures happen before the CLI can write dispatch.log.
        if let Ok(handler) = std::env::current_exe() {
            use std::io::Write;
            if let Ok(mut log) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(handler.with_file_name("handler.log"))
            {
                let _ = writeln!(log, "handler error: {error}");
            }
        }
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run() -> io::Result<()> {
    let handler = std::env::current_exe()?;
    let cli = handler.with_file_name(CLI_EXE_NAME);
    let status = Command::new(cli)
        .args(std::env::args_os().skip(1))
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "browser dispatcher exited with {status}"
        )))
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("browser-dispatcher-handler is only used on Windows");
}

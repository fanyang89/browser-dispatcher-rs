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
    // There is deliberately nowhere to print an error: this executable has no
    // console. Normal dispatch failures are written to dispatch.log by the CLI.
    let _ = run();
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

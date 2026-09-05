//! browser-dispatcher: a configurable default browser.

mod check;
mod cli;
mod config;
mod dispatch;
mod doctor;
mod install;
mod platform;
mod util;
mod wizard;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();
    let result: Result<()> = match &cli.command {
        Some(Commands::Install {
            no_wizard,
            default_browser,
        }) => install::run(
            install::InstallOptions {
                no_wizard: *no_wizard,
                default_browser: default_browser.as_deref(),
            },
            cli.config.as_deref(),
        ),
        Some(Commands::Doctor) => doctor::run(cli.config.as_deref()).map(|ok| {
            // Doctor already printed its report; exit non-zero without the
            // generic "run doctor" hint.
            if !ok {
                std::process::exit(1);
            }
        }),
        Some(Commands::Dispatch { urls, dry_run }) => {
            dispatch::run_dispatch(urls, *dry_run, cli.config.as_deref())
        }
        Some(Commands::Uninstall) => install::uninstall(),
        None => {
            if cli.urls.is_empty() {
                Cli::command().print_help().ok();
                println!();
                Ok(())
            } else {
                // Invoked by the OS as the default browser: `browser-dispatcher <url>`.
                dispatch::run_dispatch(&cli.urls, false, cli.config.as_deref())
            }
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e:#}");
        eprintln!(
            "\nhint: run `{} doctor` to diagnose problems",
            env!("CARGO_BIN_NAME")
        );
        std::process::exit(1);
    }
}

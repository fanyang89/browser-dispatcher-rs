//! Command-line interface definition (clap derive).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "browser-dispatcher",
    version,
    about = "A configurable default browser that routes each URL to Firefox / Chrome / Edge / ... based on rules",
    after_help = "Examples:\n  \
        browser-dispatcher install                 register as a browser and run the setup wizard\n  \
        browser-dispatcher doctor                  verify configuration and registration\n  \
        browser-dispatcher dispatch --dry-run URL  preview which browser a URL would use\n\n  \
        When installed as the default browser, the OS invokes\n  \
        `browser-dispatcher <url>` directly, which dispatches to the matching browser."
)]
pub struct Cli {
    /// Use this config file instead of the default location
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,

    /// URL(s) to dispatch (used when invoked by the OS as the browser handler)
    pub urls: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Register as a browser, create the default config, and run the setup wizard
    Install {
        /// Skip the interactive wizard
        #[arg(long)]
        no_wizard: bool,

        /// Set the fallback browser id (e.g. firefox); non-interactive setups
        #[arg(long, value_name = "ID")]
        default_browser: Option<String>,
    },

    /// Check that the configuration, browsers, and OS registration are healthy
    Doctor,

    /// Resolve URL(s) against the rules and open them
    #[command(alias = "open")]
    Dispatch {
        /// Show what would happen without opening anything
        #[arg(long)]
        dry_run: bool,

        /// URL(s) to dispatch
        #[arg(required = true)]
        urls: Vec<String>,
    },

    /// Remove the browser registration (keeps the config file)
    Uninstall,
}

mod commands;
mod config;
mod fetch;
mod state;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::commands::run_command;

/// Command-line arguments for rsso
#[derive(Parser, Debug)]
#[command(name = "rsso")]
#[command(about = "A minimal RSS/Atom feed organiser for the command line")]
pub struct Cli {
    /// Maximum number of items to show (overrides config)
    #[arg(short = 'n', long = "limit", global = true)]
    // TODO: should this be called limit or number/something else...
    // Fix in config file too if changed
    pub limit: Option<usize>,

    #[command(subcommand)]
    pub command: Option<Cmd>,
}

/// Subcommands for rsso
#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Subscribe to a new feed
    Sub {
        /// Feed URL (RSS or Atom)
        url: String,

        /// Optional alias for this feed
        #[arg(long)]
        alias: Option<String>,
    },

    /// Unsubscribe from a feed by alias or URL
    Unsub {
        /// Alias or full feed URL
        id_or_url: String,
    },

    /// List subscribed feeds
    List,

    /// Show items from a specific feed
    Feed {
        /// Feed alias or URL
        id_or_url: String,
    },

    /// Force refresh feeds (all or selected)
    Refresh {
        /// Zero or more aliases/URLs. If none given, refresh all feeds.
        ids_or_urls: Vec<String>,
    },

    /// Rename a feed (change its alias)
    Rename {
        /// Feed alias/title/id/url to select which feed to rename
        key: String,

        /// New alias to assign
        #[arg(long)]
        alias: String,
    }, // No subcommand -> default: show recent items from all feeds
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let cfg = config::load_config()?;
    let mut state = state::load_state(&cfg)?;

    run_command(cli, &cfg, &mut state)?;

    state::save_state(&cfg, &state)?;
    Ok(())
}

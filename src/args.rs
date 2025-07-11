use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[clap(version, author, about)]
pub struct Args {
    /// Config file
    #[arg(short, long = "config", default_value = "picodocs.toml")]
    pub config_path: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Builds the site
    Build {},

    /// Preview the site with a live-reloading server
    Serve {
        #[arg(short, long, default_value = "localhost:1809")]
        address: String,

        /// Launch site in default browser
        #[arg(long, short)]
        open: bool,
    },

    /// Dump the default configuration to a file
    Defaults {
        #[arg(short, long, default_value = "picodocs.toml")]
        output_path: PathBuf,

        /// Overwrite the output file if it already exists
        #[arg(short, long)]
        force: bool,
    },
}

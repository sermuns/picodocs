use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[clap(version, author, about)]
pub struct Args {
    /// Config file
    #[arg(short, long = "config", default_value = "picodocs.yml")]
    pub config_path: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Builds the site
    Build {
        /// Where to place rendered site files
        #[arg(short, long)]
        output_dir: Option<PathBuf>,
    },

    /// Preview the site with a live-reloading server
    Serve {
        /// For example 0.0.0.0:1809 will bind to all interfaces
        #[arg(short, long, default_value = "localhost:1809")]
        address: String,

        /// Launch site in default browser
        #[arg(long, short)]
        open: bool,
    },

    /// Dump the default configuration to a file
    Defaults {
        /// Where to write default configuration
        #[arg(short, long, default_value = "picodocs.yml")]
        output_path: PathBuf,

        /// Overwrite configuration if already existing
        #[arg(short, long)]
        force: bool,
    },
}

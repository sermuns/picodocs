use clap::{Parser, Subcommand};
use config::Config;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[clap(version, author, about)]
struct Args {
    /// Config file
    #[arg(short, long = "config", default_value = "picodocs.toml")]
    config_path: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Builds the site
    Build {},

    /// Dump the default configuration to a file
    Defaults {
        #[arg()]
        output_path: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let _args = Args::parse();

    let config = Config::builder()
        .add_source(config::File::with_name("picodocs"))
        .build()?;

    dbg!(config);

    Ok(())
}

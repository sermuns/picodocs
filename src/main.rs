mod args;
mod assets;
mod commands;
mod config;

use clap::Parser;
use confique::Partial;
use confique::{Config, File, FileFormat};

use crate::{
    args::{Args, Command},
    config::{Conf, PartialConf},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let config = Conf::from_partial(
        File::with_format(&args.config_path, FileFormat::Toml)
            .load::<PartialConf>()?
            .with_fallback(PartialConf::default_values()),
    )?;

    match args.command {
        Command::Build {} => commands::build::run(config).await?,
        Command::Serve { address, open } => commands::serve::run(config, address, open).await?,
        Command::Defaults { output_path, force } => {
            commands::defaults::run(output_path, force).await?
        }
    }

    Ok(())
}

mod args;
mod assets;
mod commands;
mod config;

use anyhow::Context;
use clap::Parser;
use confique::Partial;
use confique::{Config, File, FileFormat};

use crate::{
    args::{Args, Command},
    config::Conf,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let config = if args.config_path.exists() {
        let partial_conf: <Conf as Config>::Partial =
            File::with_format(&args.config_path, FileFormat::Toml)
                .required()
                .load()
                .with_context(|| format!("Failed to load config file: {:?}", args.config_path))?;
        Conf::from_partial(partial_conf.with_fallback(<Conf as Config>::Partial::default_values()))
            .context("Failed to merge configuration with defaults")?
    } else {
        Conf::from_partial(<Conf as Config>::Partial::default_values())
            .context("Failed to get default configuration")?
    };

    match args.command {
        Command::Build {} => commands::build::run(config).await?,
        Command::Serve { address, open } => commands::serve::run(config, address, open).await?,
        Command::Defaults { output_path, force } => {
            commands::defaults::run(output_path, force).await?
        }
    }

    Ok(())
}

use anyhow::Context;
use confique::{Config, Partial};
use std::path::PathBuf;

use crate::config::Conf;

pub async fn run(output_path: PathBuf, force: bool) -> anyhow::Result<()> {
    if output_path.exists() && !force {
        anyhow::bail!(
            "{:?} already exists. Aborting. Use --force to overwrite.",
            &output_path
        );
    }

    let default_conf = Conf::from_partial(<Conf as confique::Config>::Partial::default_values())
        .context("Failed to get default configuration for dump")?;

    tokio::fs::write(&output_path, toml::to_string(&default_conf)?)
        .await
        .with_context(|| {
            format!(
                "Failed to write default configuration to {:?}",
                &output_path
            )
        })?;
    println!("Default configuration written to {:?}", &output_path);

    Ok(())
}

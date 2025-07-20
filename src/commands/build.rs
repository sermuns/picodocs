use anyhow::Context;
use confique::{Config, Partial};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use crate::{
    assets,
    config::{Conf, PartialConf},
};

/// Build and write site to output directory.
pub fn run(partial_config: PartialConf, output_dir: Option<PathBuf>) -> anyhow::Result<()> {
    let config = Conf::from_partial(
        PartialConf {
            output_dir,
            ..PartialConf::default_values()
        }
        .with_fallback(partial_config),
    )?;

    let before_build = Instant::now();
    println!("Building site with configuration: {config:?}");

    match fs::remove_dir_all(&config.output_dir) {
        Ok(_) => {}
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => {}
            _ => {
                Err(e).with_context(|| {
                    format!(
                        "Failed to remove output directory: {:?}",
                        &config.output_dir
                    )
                })?;
            }
        },
    }

    for asset in assets::get_all_assets(&config)? {
        match asset {
            assets::Asset::Page(page) => {
                let output_path = config.output_dir.join(&page.url_path).join("index.html");

                if let Some(parent) = output_path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("Failed to create directory: {parent:?}"))?;
                }
                fs::write(&output_path, &page.rendered)
                    .with_context(|| format!("Failed to write HTML file: {output_path:?}"))?;
            }
            assets::Asset::Static(static_asset) => {
                let output_path = config.output_dir.join(&static_asset.url_path);

                if let Some(parent) = output_path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("Failed to create directory: {parent:?}"))?;
                }
                fs::write(&output_path, &static_asset.content)
                    .with_context(|| format!("Failed to write static asset: {output_path:?}"))?;
            }
        }
    }

    println!("Site built in {:?}", before_build.elapsed());
    Ok(())
}

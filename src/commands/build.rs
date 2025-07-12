use anyhow::Context;
use std::time::Instant;

use crate::{assets, config::Conf};

/// Build and write site to output directory.
pub async fn run(config: Conf) -> anyhow::Result<()> {
    let before_build = Instant::now();
    println!("Building site with configuration: {config:?}");

    match tokio::fs::remove_dir_all(&config.output_dir).await {
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

    let (html_pages, static_assets) = assets::get_all_assets(&config).await?;

    for page in html_pages {
        let output_path = config.output_dir.join(&page.url_path).join("index.html");

        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory: {parent:?}"))?;
        }
        tokio::fs::write(&output_path, page.content)
            .await
            .with_context(|| format!("Failed to write HTML file: {output_path:?}"))?;
    }
    for asset in static_assets {
        let output_path = config.output_dir.join(asset.url_path);

        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory: {parent:?}"))?;
        }
        tokio::fs::write(&output_path, &asset.content)
            .await
            .with_context(|| format!("Failed to write static asset: {output_path:?}"))?;
    }
    println!("Site built in {:?}", before_build.elapsed());
    Ok(())
}

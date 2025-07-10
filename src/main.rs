use anyhow::Context;
use clap::{Parser, Subcommand};
use confique::{Config, File, FileFormat, Partial};
use once_cell::sync::Lazy;
use pulldown_cmark::{Parser as MarkdownParser, html};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tera::Tera;
use tokio::task::JoinSet;
use walkdir::WalkDir;

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

    /// Continually watches the docs directory for changes and rebuilds the site
    Serve {
        #[arg(short, long, default_value = "localhost:1809")]
        address: String,
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

#[derive(Config, Clone, Debug, Serialize)]
struct Conf {
    title: Option<String>,

    /// Used as favicon, among other places
    icon: Option<PathBuf>,

    description: Option<String>,

    /// Sitemap will only generate if this is a full/absolute URL e.g. https://www.example.com/
    #[config(default = "/")]
    base_url: String,

    /// Root directory of markdown documentation
    #[config(default = "docs")]
    docs_dir: PathBuf,

    /// Where to place rendered site files
    #[config(default = "public")]
    output_dir: PathBuf,

    /// Follow symbolic links when traversing the docs directory
    #[config(default = false)]
    follow_links: bool,
}

type PartialConf = <Conf as Config>::Partial;

static TERA: Lazy<Tera> =
    Lazy::new(|| Tera::new("templates/*.html").expect("Failed to load templates"));

/// A built HTML file, ready to be dumped into the output directory or served
#[derive(Debug)]
struct Page {
    source_path: PathBuf,
    content: String,
    url_path: String,
}

/// A static file (non-markdown) to be served or copied
#[derive(Debug)]
struct StaticAsset {
    source_path: PathBuf,
    url_path: String,
    content: Vec<u8>,
    mime_type: mime_guess::Mime,
}

async fn get_all_assets(config: &Conf) -> anyhow::Result<(Vec<Page>, Vec<StaticAsset>)> {
    let config_arc = Arc::new(config.clone());
    let mut html_page_tasks = JoinSet::new();
    let mut static_asset_tasks = JoinSet::new();

    for entry in WalkDir::new(&config.docs_dir).follow_links(config.follow_links) {
        let source_path = entry?.into_path();

        if !source_path.is_file() {
            continue;
        }

        let config_for_task = Arc::clone(&config_arc);

        // Clone these early so `path` is no longer borrowed when we move it later.
        let relative_path = source_path.strip_prefix(&config.docs_dir)?.to_owned();

        let url_path = format!("/{}", relative_path.to_string_lossy());

        if source_path.extension() == Some(std::ffi::OsStr::new("md")) {
            html_page_tasks.spawn(async move {
                let markdown_content = tokio::fs::read_to_string(&source_path)
                    .await
                    .with_context(|| format!("Failed to read markdown file: {:?}", source_path))?;

                let mut rendered_content = String::new();
                html::push_html(
                    &mut rendered_content,
                    MarkdownParser::new(&markdown_content),
                );

                let mut context = tera::Context::new();
                context.insert("config", &*config_for_task);
                context.insert("content", &rendered_content);

                let content = TERA.render("base.html", &context).with_context(|| {
                    format!("Failed to render Tera template for file: {:?}", source_path)
                })?;

                let file_stem = relative_path.file_stem().ok_or_else(|| {
                    anyhow::anyhow!("Could not get file stem for {:?}", relative_path)
                })?;

                let url_path = if relative_path == PathBuf::from("index.md") {
                    "".to_string()
                } else {
                    let parent_dir = relative_path.parent().unwrap_or_else(|| "".as_ref());

                    let mut url_path_buf = PathBuf::new();
                    url_path_buf.push(parent_dir);
                    url_path_buf.push(file_stem);
                    url_path_buf.to_string_lossy().into_owned()
                };

                Ok(Page {
                    source_path,
                    content,
                    url_path,
                })
            });
        } else {
            static_asset_tasks.spawn(async move {
                let content = tokio::fs::read(&source_path)
                    .await
                    .with_context(|| format!("Failed to read static file: {:?}", source_path))?;

                let mime_type = mime_guess::from_path(&source_path).first_or_octet_stream();

                Ok(StaticAsset {
                    source_path,
                    url_path,
                    content,
                    mime_type,
                })
            });
        }
    }

    let html_pages: Vec<Page> = html_page_tasks
        .join_all()
        .await
        .into_iter()
        .collect::<Result<_, anyhow::Error>>()?;

    let static_assets: Vec<StaticAsset> = static_asset_tasks
        .join_all()
        .await
        .into_iter()
        .collect::<Result<_, anyhow::Error>>()?;

    Ok((html_pages, static_assets))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let config = if (&args.config_path).exists() {
        let partial_conf: PartialConf = File::with_format(&args.config_path, FileFormat::Toml)
            .required()
            .load()?;
        Conf::from_partial(partial_conf.with_fallback(PartialConf::default_values()))?
    } else {
        Conf::from_partial(PartialConf::default_values())?
    };

    match args.command {
        Command::Build {} => {
            let before_build = Instant::now();
            println!("Building site with configuration: {:?}", config);

            if config.output_dir.exists() {
                tokio::fs::remove_dir_all(&config.output_dir)
                    .await
                    .with_context(|| {
                        format!("Failed to remove output directory: {:?}", config.output_dir)
                    })?;
            }

            let (html_pages, static_assets) = get_all_assets(&config).await?;

            for page in html_pages {
                let output_path = config.output_dir.join(&page.url_path).join("index.html");
                if let Some(parent) = output_path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .with_context(|| format!("Failed to create directory: {:?}", parent))?;
                }
                tokio::fs::write(&output_path, page.content)
                    .await
                    .with_context(|| format!("Failed to write HTML file: {:?}", output_path))?;
            }
            for asset in static_assets {
                let output_path = config
                    .output_dir
                    .join(&asset.source_path.strip_prefix(&config.docs_dir)?);
                if let Some(parent) = output_path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .with_context(|| format!("Failed to create directory: {:?}", parent))?;
                }
                tokio::fs::write(&output_path, &asset.content)
                    .await
                    .with_context(|| format!("Failed to write static asset: {:?}", output_path))?;
            }
            println!("Site built in {:?}", before_build.elapsed());
        }

        Command::Serve { address } => {}

        Command::Defaults { output_path, force } => {
            if output_path.exists() && (force == false) {
                return Err(anyhow::anyhow!(
                    "{:?} already exists. Aborting.",
                    &output_path
                ));
            }

            let default_conf = Conf::from_partial(PartialConf::default_values())?;

            tokio::fs::write(&output_path, toml::to_string(&default_conf)?)
                .await
                .with_context(|| {
                    format!(
                        "Failed to write default configuration to {:?}",
                        &output_path
                    )
                })?;
            println!("Default configuration written to {:?}", &output_path);
        }
    }

    Ok(())
}

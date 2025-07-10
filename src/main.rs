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

/// A built HTML file, ready to be dumped into the output directory
#[derive(Debug)]
struct Page {
    source: PathBuf,
    content: String,
}

async fn get_built_pages(config: &Conf) -> anyhow::Result<Vec<Page>> {
    let config_arc = Arc::new(config.clone());

    let mut tasks = JoinSet::new();

    for entry in WalkDir::new(&config.docs_dir).follow_links(config.follow_links) {
        let path = entry?.into_path();
        if path.extension() != Some(std::ffi::OsStr::new("md")) {
            continue;
        }

        let config_for_task = Arc::clone(&config_arc);
        tasks.spawn(async move {
            let md_content = tokio::fs::read_to_string(&path)
                .await
                .with_context(|| format!("Failed to read markdown file: {:?}", path))?;

            let parser = MarkdownParser::new(&md_content);
            let mut html_output = String::new();
            html::push_html(&mut html_output, parser);

            let mut context = tera::Context::new();
            context.insert("config", &*config_for_task);

            context.insert("content", &html_output);

            let rendered = TERA
                .render("base.html", &context)
                .with_context(|| format!("Failed to render Tera template for file: {:?}", path))?;

            Ok(Page {
                source: path,
                content: rendered,
            })
        });
    }

    let built_pages: Vec<Page> = tasks
        .join_all()
        .await
        .into_iter()
        .collect::<Result<_, anyhow::Error>>()?;

    Ok(built_pages)
}

async fn write_pages_to_output_dir(
    pages: &[Page],
    output_dir: &PathBuf,
    config: &Conf,
) -> anyhow::Result<()> {
    if output_dir.exists() {
        std::fs::remove_dir_all(output_dir).with_context(|| {
            format!(
                "Failed to remove existing output directory: {:?}",
                output_dir
            )
        })?;
    }

    for page in pages {
        let relative_path = page.source.strip_prefix(&config.docs_dir)?;

        let file_stem = relative_path
            .file_stem()
            .ok_or_else(|| anyhow::anyhow!("Could not get file stem for {:?}", relative_path))?;

        let page_output_path = output_dir
            .join(relative_path.parent().unwrap_or_else(|| "".as_ref()))
            .join(file_stem)
            .join("index.html");

        if let Some(parent) = page_output_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }

        tokio::fs::write(&page_output_path, &page.content)
            .await
            .with_context(|| format!("Failed to write page to: {:?}", page_output_path))?;
    }
    Ok(())
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

            let built_pages = get_built_pages(&config).await?;
            let output_dir = &config.output_dir;
            write_pages_to_output_dir(&built_pages, output_dir, &config).await?;

            println!(
                "Site built to {:?} in {:?}",
                output_dir,
                before_build.elapsed()
            );
        }

        Command::Serve { address } => {
            println!("Serving on {}", address);
        }

        Command::Defaults { output_path, force } => {
            if output_path.exists() && (force == false) {
                return Err(anyhow::anyhow!(
                    "{:?} already exists. Aborting.",
                    &output_path
                ));
            }
            let default_conf = Conf::from_partial(PartialConf::default_values())?;
            let toml_string = toml::to_string(&default_conf)?;
            std::fs::write(&output_path, toml_string)?;
            println!("Default configuration written to {:?}", &output_path);
        }
    }

    Ok(())
}

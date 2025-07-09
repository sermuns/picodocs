use clap::{Parser, Subcommand};
use confique::{Config, File, FileFormat, Partial};
use pulldown_cmark::{Parser as MarkdownParser, html};
use serde::Serialize;
use std::fs;
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

    /// Dump the default configuration to a file
    Defaults {
        #[arg(short, long, default_value = "picodocs.toml")]
        output_path: PathBuf,

        /// Overwrite the output file if it already exists
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Config, Debug, Serialize)]
struct Conf {
    title: Option<String>,

    /// Used as favicon, among other places
    icon: Option<PathBuf>,

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
            println!("Building site with configuration: {:?}", config);
            let before_build = Instant::now();

            if config.output_dir.exists() {
                fs::remove_dir_all(&config.output_dir)?;
            }
            fs::create_dir_all(&config.output_dir)?;

            let tera = Arc::new(Tera::new("templates/*.html")?);

            let mut tasks = JoinSet::new();
            for entry in WalkDir::new(&config.docs_dir).follow_links(config.follow_links) {
                let path = entry?.into_path();
                if path.extension() != Some(std::ffi::OsStr::new("md")) {
                    continue;
                }

                let tera = Arc::clone(&tera);
                let output_root = config.output_dir.clone();
                tasks.spawn(async move {
                    let output_path = output_root
                        .join(path.file_stem().unwrap())
                        .join("index.html");

                    let md_content = tokio::fs::read_to_string(&path).await.unwrap();
                    let parser = MarkdownParser::new(&md_content);
                    let mut html_output = String::new();
                    html::push_html(&mut html_output, parser);

                    let mut context = tera::Context::new();
                    context.insert("content", &html_output);

                    let rendered = tera.render("base.html", &context).unwrap();

                    tokio::fs::create_dir_all(output_path.parent().unwrap())
                        .await
                        .unwrap();

                    tokio::fs::write(output_path, rendered).await.unwrap();
                });
            }

            tasks.join_all().await;

            println!("Site built in {:?}", before_build.elapsed());
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

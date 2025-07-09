use clap::{Parser, Subcommand};
use confique::{Config, File, FileFormat, Partial};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Instant;
use tokio::fs as tokio_fs;
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

            let mut tasks = JoinSet::new();
            for entry in WalkDir::new(&config.docs_dir).follow_links((&config).follow_links) {
                let path = entry?.into_path();
                if path.extension() != Some(std::ffi::OsStr::new("md")) {
                    continue;
                }

                let output_path = config
                    .output_dir
                    .join(path.file_stem().unwrap())
                    .join("index.html");

                tasks.spawn(async move {
                    dbg!(path);
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

use clap::{Parser, Subcommand};
use confique::{Config, File, FileFormat, Partial};
use serde::Serialize;
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

    icon: Option<PathBuf>,

    #[config(default = "public")]
    output_dir: PathBuf,
}
type PartialConf = <Conf as Config>::Partial;

fn main() -> anyhow::Result<()> {
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
            println!("Building the site!");
            dbg!(config);
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

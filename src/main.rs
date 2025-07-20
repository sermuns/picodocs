mod args;
mod assets;
mod commands;
mod config;

use clap::{CommandFactory, Parser, crate_name};
use clap_complete::generate;
use confique::Partial;
use confique::{File, FileFormat};

use crate::{
    args::{Args, Command},
    config::PartialConf,
};

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let partial_conf = File::with_format(
        &args.config_path,
        match args.config_path.extension().unwrap().to_str().unwrap() {
            "yml" | "yaml" => FileFormat::Yaml,
            "toml" => FileFormat::Toml,
            _ => {
                anyhow::bail!(
                    "Unsupported file extension : {:?}. Supported extensions are .yml, .yaml, and .toml.",
                    &args.config_path
                );
            }
        },
    )
    .load::<PartialConf>()?
    .with_fallback(PartialConf::default_values());

    match args.command {
        Command::Build { output_dir } => commands::build::run(partial_conf, output_dir)?,
        Command::Serve { address, open } => commands::serve::run(partial_conf, address, open)?,
        Command::Defaults { output_path, force } => commands::defaults::run(output_path, force)?,
        Command::Completion { shell } => {
            let mut cmd = Args::command();
            generate(shell, &mut cmd, crate_name!(), &mut std::io::stdout());
        }
    }

    Ok(())
}

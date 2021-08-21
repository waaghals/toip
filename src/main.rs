use std::env;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use anyhow::{Context, Result};
use config::RuntimeConfig;
use rand::distributions::Alphanumeric;
use rand::{self, Rng};
use structopt::StructOpt;

use crate::image::manager::ImageManager;
use crate::oci::runtime::{OciCliRuntime, Runtime};
use crate::runtime::generator::{RunGenerator, RuntimeBundleGenerator};
mod config;
mod dirs;
mod image;
mod init;
mod logger;
mod metadata;
mod oci;
mod runtime;

#[derive(StructOpt, Debug)]
#[structopt(about = "Tools to allow separate containers to call each other")]
struct Cli {
    #[structopt(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,
    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt, Debug)]
enum Command {
    #[structopt(help = "Add the current configured aliases into the shell")]
    Inject {},

    #[structopt(help = "Acts as a containers init process")]
    Init {
        #[structopt(help = "Command to start")]
        cmd: String,

        #[structopt(help = "Arguments to pass to starting process")]
        args: Vec<String>,
    },

    #[structopt(help = "Run a container for a given alias")]
    Run {
        #[structopt(help = "Alias to run")]
        alias: String,
        #[structopt(help = "Arguments to call the container with")]
        args: Vec<String>,
    },

    #[structopt(help = "Run a linked container from a runtime config")]
    Exec {
        #[structopt(
            parse(from_os_str),
            help = "Script with run configuration to interpret"
        )]
        file: PathBuf,
        #[structopt(help = "Arguments to call the container with")]
        args: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::from_args();
    logger::init(cli.verbosity.log_level()).context("could not initialize logger")?;
    match cli.command {
        Command::Init { cmd, args } => init::spawn(cmd, args)?,
        Command::Run { alias, args } => {
            let dir = env::current_dir().unwrap();
            // config.validate()?;
            let config = config::from_dir(&dir).unwrap();
            let container = config.get_container_by_alias(&alias)?;

            let container_id: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(30)
                .map(char::from)
                .collect();
            log::info!("running container `{}`", container_id);

            let runtime_generator = RunGenerator::default();
            let bundle_path = runtime_generator
                .build(&container_id, container, args)
                .await?;

            let runtime = OciCliRuntime::default();
            runtime.run(&container_id, &bundle_path)?;
            fs::remove_dir_all(&bundle_path).with_context(|| {
                format!(
                    "could not remove directory `{}`",
                    bundle_path.to_str().unwrap()
                )
            })?;
        }
        Command::Exec { file, args: _ } => {
            let file = OpenOptions::new().read(true).write(true).open(file)?;

            let lines = BufReader::new(file)
                .lines()
                .skip(1)
                .map(|x| x.unwrap())
                .collect::<Vec<String>>()
                .join("\n");

            let config: RuntimeConfig =
                serde_json::from_str(&lines).context("could not parse exec information")?;

            let _container = config
                .config
                .get_container_by_name(&config.container_name)
                .unwrap();

            todo!();
        }
        Command::Inject {} => {
            let dir = env::current_dir().unwrap();
            let config = config::from_dir(&dir).unwrap();
            let mut image_manager = ImageManager::default();
            for container in config.containers() {
                image_manager.prepare(&container.image).await?;
            }
        }
    }

    Ok(())

    //TODO https://github.com/riboseinc/riffol
    //TODO https://crates.io/crates/atty
    //TODO https://github.com/cyphar/initrs/blob/master/src/main.rs
}

// https://www.joshmcguigan.com/blog/build-your-own-shell-rust/
// https://github.com/myfreeweb/interactor/tree/master/src

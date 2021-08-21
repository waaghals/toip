use std::env::current_dir;
use std::fs::{OpenOptions, remove_dir_all};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use anyhow::{Context, Result};
use config::RuntimeConfig;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use structopt::StructOpt;

use crate::image::manager::ImageManager;
use crate::oci::runtime::{OciCliRuntime, Runtime};
use crate::runtime::generator::{RunGenerator, RuntimeBundleGenerator};
mod config;
mod container;
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
            let dir = current_dir().unwrap();
            // config.validate()?;
            let config = config::from_dir(&dir).unwrap();
            let container = config.get_container_by_alias(&alias)?;

            let container_id: String = thread_rng()
                .sample_iter(&Alphanumeric)
                .take(30)
                .map(char::from)
                .collect();
            log::info!("running container `{}`", container_id);

            let runtime_generator = RunGenerator::default();
            let bundle_path = runtime_generator.build(&container_id, container, args).await?;

            let runtime = OciCliRuntime::default();
            runtime.run(&container_id, &bundle_path)?;
            remove_dir_all(bundle_path)?;

            // let runtime = CommandRuntime::new("runc");

            // let runtime = container::Runtime::new();
            // let manager = container::Manager {
            //     workdir: dir,
            //     config,
            //     runtime,
            // };

            // let args = args.iter().map(|a| a.as_str()).collect();
            // manager.run(&alias, args).await?
        }
        Command::Exec { file, args } => {
            let file = OpenOptions::new().read(true).write(true).open(file)?;

            let lines = BufReader::new(file)
                .lines()
                .skip(1)
                .map(|x| x.unwrap())
                .collect::<Vec<String>>()
                .join("\n");

            let config: RuntimeConfig =
                serde_json::from_str(&lines).context("could not parse exec information")?;

            let container = config
                .config
                .get_container_by_name(&config.container_name)
                .unwrap();

            todo!();
            // runtime
            //     .run_container(&container.image, &container.cmd, &Some(args), &None, &None)
            //     .await?
        }
        Command::Inject {} => {
            let dir = current_dir().unwrap();
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

    //TODO extract/unpack to oci runtime bundle:
    // https://fly.io/blog/docker-without-docker/
    // https://github.com/daikimiura/rocker/blob/master/src/image.rs
    // https://github.com/opencontainers/umoci/blob/758044fc26ad65eb900143e90d1e22c2d6e4484d/oci/layer/unpack.go#L161
    // https://github.com/opencontainers/umoci/blob/758044fc26ad65eb900143e90d1e22c2d6e4484d/oci/layer/unpack.go#L55

    // match config.get_container_by_alias(&command) {
    //     Some(container) => {
    //         println!("{:#?}", container)
    //     },
    //     None => {
    //         println!("No such container. \nOnly available containers:\n{:#?}", config.available_aliases());
    //         return Ok(());
    //     }
    // }

    // match run_container().await {
    //     Ok(_) => println!("done"),
    //     Err(err) => println!("{:#?}", err),
    // };

    // return Ok(());
}

// https://www.joshmcguigan.com/blog/build-your-own-shell-rust/
// https://github.com/myfreeweb/interactor/tree/master/src

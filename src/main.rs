#![feature(async_stream)]
#![feature(unix_socket_ancillary_data)]
#![feature(const_mut_refs)]
#![feature(ready_macro)]

use std::env;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{self};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use serve::CallInfo;

use crate::command::call::call;
use crate::command::inject::inject;
use crate::command::prepare::prepare;
use crate::command::run::run;
use crate::oci::runtime::{OciCliRuntime, Runtime};
use crate::runtime::generator::{RunGenerator, RuntimeBundleGenerator};
use crate::serve::Serve;

mod command;
mod config;
mod dirs;
mod image;
mod logger;
mod metadata;
mod oci;
mod progress_bar;
mod runtime;
mod script;
mod serve;

#[derive(Parser, Debug)]
#[clap(version, author, about)]
struct Cli {
    #[clap(flatten)]
    verbose: Verbosity,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// add the current configured aliases into the shell
    Inject {},

    /// build and or pull containers
    Prepare {
        /// container name
        #[clap(short, long)]
        container: Option<String>,
    },

    /// run a container for a given alias
    Run {
        /// alias to run
        alias: String,
        /// argument to call the container with
        args: Vec<String>,
    },

    /// run a linked container
    Call {
        #[clap(parse(from_os_str))]
        file_path: PathBuf,
        /// argument to call the container with
        args: Vec<String>,
    },

    /// remove cache and/or containers
    Clean {
        /// remove containers
        #[clap(short, long)]
        containers: bool,
    },
}

#[tokio::main()]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    logger::init(cli.verbose.log_level()).context("could not initialize logger")?;
    log::trace!("current pid is `{}`", process::id());

    match cli.command {
        Command::Run { alias, args } => run(alias, args).await,
        Command::Call { file_path, args } => {
            let file = OpenOptions::new().read(true).write(true).open(&file_path)?;

            let container_name = BufReader::new(file).lines().last().with_context(|| {
                format!(
                    "could not read container name from file `{}`",
                    file_path.display()
                )
            })??;

            let socket_path = env::var("TOIP_SOCK")
                .context("environment variable `TOIP_SOCK` does not exists")?;
            call(socket_path, &container_name, args)
                .with_context(|| format!("could not call container `{}`", container_name))
        }
        Command::Prepare { container } => prepare(container).await,
        Command::Inject {} => inject(),
        _ => todo!(),
    }
}

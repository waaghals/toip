#![feature(async_stream)]
#![feature(unix_socket_ancillary_data)]
#![feature(const_mut_refs)]

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, IoSlice};
use std::os::unix::io::FromRawFd;
use std::os::unix::net::{SocketAncillary, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{self, Stdio};
use std::{env, fs};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use itertools::join;
use rand::distributions::Alphanumeric;
use rand::{self, Rng};
use serve::CallInfo;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::commands::call::call;
use crate::commands::prepare::prepare;
use crate::commands::run::run;
use crate::image::manager::ImageManager;
use crate::oci::runtime::{OciCliRuntime, Runtime};
use crate::runtime::generator::{RunGenerator, RuntimeBundleGenerator};
use crate::serve::Serve;

mod commands;
mod config;
mod dirs;
mod image;
mod logger;
mod metadata;
mod oci;
mod runtime;
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
    Inject {
        /// prepare containers
        #[clap(short, long)]
        prepare: Option<bool>,
    },
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
        _ => Ok(()),
    }
}

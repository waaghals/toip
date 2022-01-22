#![feature(async_stream)]
#![feature(unix_socket_ancillary_data)]
#![feature(const_mut_refs)]
#![feature(ready_macro)]
// #![deny(missing_docs)]

use std::env;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader};
use std::process::{self};

use anyhow::{Context, Result};
use clap::Parser;
use serve::CallInfo;

use crate::cli::{Cli, Command};
use crate::command::{call, inject, install, prepare, run};
use crate::oci::runtime::{OciCliRuntime, Runtime};
use crate::runtime::generator::{RunGenerator, RuntimeBundleGenerator};
use crate::serve::Serve;

mod cli;
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
        Command::Install {} => install(),
        Command::Inject { shell } => inject(shell),
        _ => todo!(),
    }
}

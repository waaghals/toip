// #![feature(unix_socket_ancillary_data)]

use std::env;
use std::process::{self};

use anyhow::{anyhow, Context, Result};
use backend::script;
use clap::Parser;
use server::CallInfo;

use crate::cli::{Arguments, Cli, Command};
use crate::command::{call, inject, install, prepare, run};
use crate::config::{find_config_file, Config};

mod backend;
mod cli;
mod command;
mod config;
mod dirs;
mod dotenv;
mod logger;
mod metadata;
mod server;

#[tokio::main()]
async fn main() -> Result<()> {
    dotenv::load().context("could not load environment variables")?;

    let cli = Cli::parse();
    logger::init(cli.verbose.log_level()).context("could not initialize logger")?;
    log::trace!("current pid is `{}`", process::id());

    match cli.command {
        Command::Run { script, args } => {
            let actual_args = match args {
                Some(Arguments::Arguments(arg)) => arg,
                None => vec![],
            };
            run(script, actual_args).await
        }
        Command::Call { script, args } => {
            let container_name = script::read_container(script)?;
            let socket_path = env::var("TOIP_SOCK")
                .context("environment variable `TOIP_SOCK` does not exists")?;

            let actual_args = match args {
                Some(Arguments::Arguments(arg)) => arg,
                None => vec![],
            };
            call(socket_path, &container_name, actual_args)
                .with_context(|| format!("could not call container `{}`", container_name))
        }
        Command::Prepare {
            container,
            ignore_missing,
        } => prepare(ignore_missing, container).await,
        Command::Install { ignore_missing } => install(ignore_missing),
        Command::Inject { shell } => inject(shell),
        Command::Debug {} => {
            let current_dir = env::current_dir()?;
            let config_path = find_config_file(current_dir)
                .ok_or_else(|| anyhow!("Unable to find config file"))?;
            let config_dir = config_path.parent().unwrap().to_path_buf();
            let config = Config::new_from_dir(&config_dir)?;
            dbg!(config);
            Ok(())
        }
        _ => todo!(),
    }
}

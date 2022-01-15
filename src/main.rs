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
use itertools::join;
use rand::distributions::Alphanumeric;
use rand::{self, Rng};
use serve::CallInfo;
use structopt::StructOpt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::image::manager::ImageManager;
use crate::oci::runtime::{OciCliRuntime, Runtime};
use crate::runtime::generator::{RunGenerator, RuntimeBundleGenerator};
use crate::serve::Serve;

mod config;
mod dirs;
mod image;
mod init;
mod logger;
mod metadata;
mod oci;
mod runtime;
mod serve;

#[derive(StructOpt, Debug)]
#[structopt(about = "Tool to allow separate containers to call each other")]
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
    Call {
        #[structopt(
            parse(from_os_str),
            help = "Script with run configuration to interpret"
        )]
        file_path: PathBuf,
        #[structopt(help = "Arguments to call the container with")]
        args: Vec<String>,
    },

    #[structopt(help = "Run a linked container from a runtime config")]
    Serve {},
}

fn call<S, C, A>(socket_path: S, alias: C, args: A) -> Result<()>
where
    S: AsRef<Path>,
    C: Into<String>,
    A: IntoIterator<Item = String>,
{
    let call_info = CallInfo {
        name: alias.into(),
        arguments: args.into_iter().collect(),
        envargs: HashMap::new(),
    };

    let socket_path = socket_path.as_ref();

    let json =
        serde_json::to_string(&call_info).context("could not serialize call info to json")?;
    let data = json.as_bytes();
    let size = data.len() as u32;

    let socket = UnixStream::connect(&socket_path)
        .with_context(|| format!("could not connect to socket `{}`", socket_path.display()))?;

    let buf1 = size.to_be_bytes();
    let bufs = &[IoSlice::new(&buf1), IoSlice::new(data)][..];
    let fds = [0, 1, 2];
    let mut ancillary_buffer = [0; 128];
    let mut ancillary = SocketAncillary::new(&mut ancillary_buffer[..]);
    ancillary.add_fds(&fds[..]);
    log::debug!(
        "sending ancillary information over socket `{:#?}` with file descriptors `{}`",
        &socket_path,
        join(fds, ", ")
    );
    socket
        .send_vectored_with_ancillary(bufs, &mut ancillary)
        .with_context(|| {
            format!(
                "could not send ancillary data to socket `{}`",
                socket_path.display()
            )
        })?;

    Ok(())
}

#[tokio::main()]
async fn main() -> Result<()> {
    let cli = Cli::from_args();
    logger::init(cli.verbosity.log_level()).context("could not initialize logger")?;
    log::trace!("current pid is `{}`", process::id());
    match cli.command {
        Command::Init { cmd, args } => init::spawn(cmd, args)?,
        Command::Run { alias, args } => {
            let dir = env::current_dir()?;
            // config.validate()?;
            let config = config::from_dir(&dir)?;

            let runtime = OciCliRuntime::default();
            let runtime_generator = RunGenerator::default();

            let (tx, rx) = mpsc::channel(100);

            // Start listening for incoming calls
            let socket = dirs::socket_path().context("could not determine socket path")?;
            let server = Serve::new(&socket, tx);

            let socket_dir = socket.parent().with_context(|| {
                format!(
                    "could not determine socket directory `{}`",
                    socket.display()
                )
            })?;
            fs::create_dir_all(socket_dir).with_context(|| {
                format!("could not create directory `{}`", socket_dir.display())
            })?;
            // TODO improve error handling in the threads below
            let server_handle = tokio::spawn(async move {
                let res = server.listen().await;
                res
            });

            // Call the setup listener to start the initial container
            let call_socket = socket.clone();
            let call_handle = tokio::spawn(async move {
                // TODO pass a 'ready' signal through the receiverStream and send the call when it is received.
                std::thread::sleep(std::time::Duration::from_millis(100));
                // TODO find container name for alias
                log::debug!("calling `{}` with arguments `{}`", alias, args.join(", "));
                call(&call_socket, &alias, args)
                    .with_context(|| format!("could not call container `{}`", alias))
            });

            // Handle each call instruction
            let mut call_instruction_stream = ReceiverStream::new(rx);
            // Iteration will stop when tx is dropped
            // tx is dropped whenever server is dropped
            while let Some(instruction) = call_instruction_stream.next().await {
                let runtime_generator = runtime_generator.clone();
                let runtime = runtime.clone();
                let config = config.clone();
                log::trace!(
                    "received file descriptors `{}`",
                    join(&instruction.file_descriptors, ", ")
                );

                let ci_socket = socket.clone();
                tokio::spawn(async move {
                    log::debug!("received call for container `{}`", instruction.info.name);
                    let container_option = config.get_container_by_name(&instruction.info.name);

                    match container_option {
                        Some(container) => {
                            let container_id: String = rand::thread_rng()
                                .sample_iter(&Alphanumeric)
                                .take(30)
                                .map(char::from)
                                .collect();

                            log::info!(
                                "running `{:#?}` in container `{}`",
                                container.cmd,
                                container_id
                            );

                            let bundle_path = runtime_generator
                                .build(
                                    &container_id,
                                    &container,
                                    instruction.info.arguments,
                                    ci_socket,
                                )
                                .await
                                .unwrap();

                            // Ensure the the new Stdio instance are the sole owners of the file descriptors.
                            // i.e. no other code must consume the instructions.file_descriptors
                            unsafe {
                                let stdin = Stdio::from_raw_fd(instruction.file_descriptors[0]);
                                let stdout = Stdio::from_raw_fd(instruction.file_descriptors[1]);
                                let stderr = Stdio::from_raw_fd(instruction.file_descriptors[2]);

                                // Drop file_descriptors from above so they cannot be used elsewhere
                                drop(instruction.file_descriptors);

                                runtime
                                    .run(&container_id, &bundle_path, stdin, stdout, stderr)
                                    .unwrap();

                                // TODO close file descriptors
                            }

                            log::info!("removing bundle path `{}`", bundle_path.display());

                            // TODO find out why work directory within the workdir is non executable
                            rm_rf::remove(&bundle_path)
                                .with_context(|| {
                                    format!(
                                        "could not remove directory `{}`",
                                        bundle_path.display()
                                    )
                                })
                                .unwrap();
                        }
                        None => todo!(),
                    }
                });
            }

            server_handle.await??;
            call_handle.await??;

            log::info!("removing socket `{}`", socket.display());
            fs::remove_file(&socket)
                .with_context(|| format!("could not delete socket `{}`", socket.display()))?;
        }
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
                .with_context(|| format!("could not call container `{}`", container_name))?;
        }
        Command::Inject {} => {
            let dir = env::current_dir().context("could not determine current directory")?;
            let config = config::from_dir(&dir).with_context(|| {
                format!("could not parse config from directory `{}`", dir.display())
            })?;
            let image_manager =
                ImageManager::new().context("could not construct `ImageManager`")?;
            for container in config.containers() {
                image_manager.prepare(&container.image).await?;
            }
        }
        Command::Serve {} => {
            todo!();
        }
    }

    Ok(())
}

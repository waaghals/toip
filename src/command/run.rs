use std::fs;
use std::os::unix::io::FromRawFd;
use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use futures_util::stream::FuturesUnordered;
use itertools::join;
use rand::distributions::Alphanumeric;
use rand::{self, Rng};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::command::call::call;
use crate::config::Config;
use crate::{dirs, script, server, OciCliRuntime, RunGenerator, Runtime, RuntimeBundleGenerator};

pub async fn run<P>(script_path: P, args: Vec<String>) -> Result<()>
where
    P: AsRef<Path>,
{
    let script_path = script_path.as_ref();
    let container_name = script::read_container(script_path)
        .with_context(|| format!("could not read script file `{}`", script_path.display()))?;

    let script_dir = script_path.parent().with_context(|| {
        format!(
            "could not determine config directory from script file `{}`",
            script_path.display()
        )
    })?;
    let config = Config::new_from_dir(script_dir)?;
    let runtime = OciCliRuntime::default();
    let runtime_generator = RunGenerator::default();

    let (tx, rx) = mpsc::channel(100);

    // Start listening for incoming calls
    let socket = dirs::socket_path().context("could not determine socket path")?;
    let cancellation_token = CancellationToken::new();
    let socket_dir = socket.parent().with_context(|| {
        format!(
            "could not determine socket directory `{}`",
            socket.display()
        )
    })?;
    fs::create_dir_all(socket_dir)
        .with_context(|| format!("could not create directory `{}`", socket_dir.display()))?;
    let serve_socket = socket.clone();
    let server = server::create(serve_socket, tx, cancellation_token.clone())
        .context("could not setup call listener")?;

    // Call the setup listener to start the initial container
    let call_socket = socket.clone();
    let origin_container_name = &container_name.clone();
    let call_handle = tokio::spawn(async move {
        log::debug!(
            "calling `{}` with arguments `{}`",
            &container_name,
            args.join(", ")
        );
        call(&call_socket, &container_name, args)
            .with_context(|| format!("could not call container `{}`", container_name))
    });
    let server_handle = tokio::spawn(async move {
        let res = server.listen().await;
        res
    });

    let mut container_handles = FuturesUnordered::new();
    // Handle each call instruction
    let mut call_instruction_stream = ReceiverStream::new(rx);
    call_handle
        .await
        .context("could not join call thread")?
        .context("could not perform call")?;

    let mut cancellation_handle = None;

    // Iteration will stop when tx is dropped
    // tx is dropped whenever server is dropped
    while let Some(instruction) = call_instruction_stream.next().await {
        let call_container_name = instruction.info.name.clone();
        let runtime_generator = runtime_generator.clone();
        let runtime = runtime.clone();
        let config = config.clone();
        log::trace!(
            "received file descriptors `{}`",
            join(&instruction.file_descriptors, ", ")
        );

        let ci_socket = socket.clone();
        let container_handle = tokio::spawn(async move {
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
                        .context("was not able to generate runtime bundle")?;

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
                            .with_context(|| {
                                format!("could not run container from `{}`", bundle_path.display())
                            })?;
                        log::debug!("runtime finished running container `{}`", container_id);
                    }

                    log::info!("removing bundle path `{}`", bundle_path.display());

                    // TODO find out why work directory within the workdir is non executable
                    rm_rf::remove(&bundle_path).with_context(|| {
                        format!("could not remove directory `{}`", bundle_path.display())
                    })
                }
                None => todo!(),
            }
        });

        // Store the container threads somewhere. The origin container (which made the first call) will
        // be stored separately, because when that thread is joined, we can stop the whole application
        if &call_container_name == origin_container_name && cancellation_handle.is_none() {
            let cancellation_token = cancellation_token.clone();
            // Await the origin handle in a separate thread so we don't block the instructions loop
            let handle = tokio::spawn(async move {
                let result = container_handle
                    .await
                    .context("could not join origin container thread")?
                    .context("failure during origin container invocation");
                // Wait for the origin container to complete, then stop the listener.
                // When the listener is stopped, it will also terminate the instruction stream
                // which breaks this while loop and allows us to tear everything down
                cancellation_token.cancel();
                result
            });
            cancellation_handle = Some(handle);
        } else {
            container_handles.push(container_handle);
        }
    }

    if let Some(handle) = cancellation_handle {
        handle
            .await
            .context("could not join cancellation thread")?
            .context("failure during cancellation thread")?;
    }

    log::debug!("Instruction stream ended");
    server_handle
        .await
        .context("could not join server thread")?
        .context("could not initialize call listener")?;

    log::info!("removing socket `{}`", socket.display());
    fs::remove_file(&socket)
        .with_context(|| format!("could not delete socket `{}`", socket.display()))?;

    while let Some(finished_container) = container_handles.next().await {
        finished_container
            .context("could not join container thread")?
            .context("failure from container thread")?;

        log::info!("Container finished executing");
    }
    log::debug!("All containers threads finished executing");

    Ok(())
}

use std::fs;
use std::os::unix::io::FromRawFd;
use std::process::Stdio;

use anyhow::{Context, Result};
use itertools::join;
use rand::distributions::Alphanumeric;
use rand::{self, Rng};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::commands::call::call;
use crate::{config, dirs, OciCliRuntime, RunGenerator, Runtime, RuntimeBundleGenerator, Serve};

pub async fn run(alias: String, args: Vec<String>) -> Result<()> {
    let config = config::from_current_dir()?;
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
    fs::create_dir_all(socket_dir)
        .with_context(|| format!("could not create directory `{}`", socket_dir.display()))?;
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
                            format!("could not remove directory `{}`", bundle_path.display())
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

    Ok(())
}

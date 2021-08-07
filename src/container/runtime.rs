use anyhow::Result;
use bollard::container::{Config as BollardConfig, RemoveContainerOptions};
use bollard::exec::{CreateExecOptions, ResizeExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::models::{HostConfig, Mount, MountTypeEnum};
use bollard::Docker;
use futures_util::{StreamExt, TryStreamExt};
use std::collections::HashMap;
use std::io::{stdout, Read, Write};
use std::time::Duration;
#[cfg(not(windows))]
use termion::raw::IntoRawMode;
#[cfg(not(windows))]
use termion::{async_stdin, terminal_size};
use tokio::io::AsyncWriteExt;
use tokio::task::spawn;
use tokio::time::sleep;

pub struct Runtime {
    docker: Docker,
}


impl Runtime {
    pub fn new() -> Self {
        Runtime {
            docker: Docker::connect_with_socket_defaults().unwrap(),
        }
    }

    async fn create_image(&self, image: &str) -> Result<()> {
        self.docker
            .create_image(
                Some(CreateImageOptions {
                    from_image: image,
                    ..Default::default()
                }),
                None,
                None,
            )
            .try_collect::<Vec<_>>()
            .await?;
        Ok(())
    }

    pub async fn run_container<'a>(
        &self,
        image: &String,
        cmd: &String,
        args: &Option<Vec<String>>,
        mounts: &Option<HashMap<String, String>>,
        envvars: &Option<HashMap<String, String>>,
    ) -> Result<()> {
        self.create_image(&image).await?;
        #[cfg(not(windows))]
        let tty_size = terminal_size()?;

        let host_mounts = match mounts {
            Some(mounts) => mounts
                .iter()
                .map(|(target, source)| Mount {
                    target: Some(target.to_string()),
                    source: Some(source.to_string()),
                    typ: Some(MountTypeEnum::BIND),
                    ..Default::default()
                })
                .collect(),
            None => Vec::<Mount>::new(),
        };
        let host_config = HostConfig {
            mounts: Some(host_mounts),
            ..Default::default()
        };
        let container_config = BollardConfig::<String> {
            image: Some(image.to_string()),
            tty: Some(true),
            // volumes: Some(volumes),
            host_config: Some(host_config),
            ..Default::default()
        };

        let id = self
            .docker
            .create_container::<String, String>(None, container_config)
            .await?
            .id;
        self.docker.start_container::<String>(&id, None).await?;

        let env = match envvars {
            Some(envvars) => envvars
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<String>>(),
            None => Vec::<String>::new(),
        };

        let mut full_cmd = Vec::<String>::new();
        full_cmd.push(cmd.to_string());
        match args {
            Some(args) => {
                for arg in args {
                    full_cmd.push(arg.to_string());
                }
            }
            None => {}
        }

        let exec = self
            .docker
            .create_exec(
                &id,
                CreateExecOptions::<String> {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    attach_stdin: Some(true),
                    tty: Some(true),
                    cmd: Some(full_cmd),
                    env: Some(env),
                    ..Default::default()
                },
            )
            .await?
            .id;
        #[cfg(not(windows))]
        if let StartExecResults::Attached {
            mut output,
            mut input,
        } = self.docker.start_exec(&exec, None).await?
        {
            // pipe stdin into the docker exec stream input
            spawn(async move {
                let mut stdin = async_stdin().bytes();
                loop {
                    if let Some(Ok(byte)) = stdin.next() {
                        input.write(&[byte]).await.ok();
                    } else {
                        sleep(Duration::from_nanos(10)).await;
                    }
                }
            });
            self.docker
                .resize_exec(
                    &exec,
                    ResizeExecOptions {
                        height: tty_size.1,
                        width: tty_size.0,
                    },
                )
                .await?;
            // set stdout in raw mode so we can do tty stuff
            let stdout = stdout();
            let mut stdout = stdout.lock().into_raw_mode()?;
            // pipe docker exec output into stdout
            while let Some(Ok(output)) = output.next().await {
                stdout.write(output.into_bytes().as_ref())?;
                stdout.flush()?;
            }
        }
        self.docker
            .remove_container(
                &id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;
        return Ok(());
    }
}

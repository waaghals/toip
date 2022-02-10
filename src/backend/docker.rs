use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use tokio::process::Command;

use crate::backend::{Backend, Mount, MountType};
use crate::config::{ContainerConfig, ImageSource, RegistrySource};

pub struct Docker {
    binary: PathBuf,
    socket: Option<PathBuf>,
}

pub enum SshSockets {
    Default,
    Named { id: String, path: PathBuf },
}

impl Docker {
    pub fn new() -> Self {
        Docker {
            binary: PathBuf::from("docker"),
            socket: None,
        }
    }
}

impl Docker {
    pub fn create_run_command(
        &self,
        image: String,
        mounts: Option<Vec<Mount>>,
        entrypoint: Option<String>,
        cmd: Option<String>,
        args: Option<Vec<String>>,
        env_vars: Option<HashMap<String, String>>,
        env_files: Option<Vec<PathBuf>>,
        workdir: Option<PathBuf>,
        init: Option<bool>,
    ) -> Command {
        let mut command = Command::new(&self.binary);
        command.env_clear();

        command.arg("run");

        command.arg("--rm");
        command.arg("-it");

        command.arg("--pull");
        command.arg("never");

        if let Some(vars) = env_vars {
            for (name, value) in vars {
                command.arg("--env");
                command.arg(format!("{}={}", name.to_uppercase(), value));
            }
        }

        if let Some(files) = env_files {
            for file in files {
                command.arg("--env-file");
                command.arg(file);
            }
        }

        if let Some(mounts) = mounts {
            for mount in mounts {
                command.arg("--mount");
                match mount.mount_type {
                    MountType::Volume { source } => {
                        let mut arg = format!("type=volume,target={}", mount.target.display());
                        if let Some(source) = source {
                            arg.push_str(format!(",source={}", source).as_str())
                        }
                        command.arg(arg);
                    }
                    MountType::Bind {
                        source,
                        consistency,
                        bind_propagation,
                        bind_nonrecursive,
                    } => {
                        let mut arg = format!(
                            "type=bind,target={},source={}",
                            mount.target.display(),
                            source.display(),
                        );
                        if let Some(consistency) = consistency {
                            arg.push_str(format!(",consistency={}", consistency).as_str())
                        }
                        if let Some(bind_propagation) = bind_propagation {
                            arg.push_str(
                                format!(",bind-propagation={}", bind_propagation).as_str(),
                            );
                        }
                        if let Some(bind_nonrecursive) = bind_nonrecursive {
                            arg.push_str(
                                format!(
                                    ",bind-nonrecursive={}",
                                    if bind_nonrecursive { "true" } else { "false" }
                                )
                                .as_str(),
                            );
                        }
                    }
                    MountType::Tmpfs => {
                        command.arg("type=tmpfs");
                    }
                }
            }
        }

        if let Some(workdir) = workdir {
            command.arg("--workdir");
            command.arg(workdir);
        }

        if let Some(entrypoint) = entrypoint {
            command.arg("--entrypoint");
            command.arg(entrypoint);
        }

        if let Some(init) = init {
            if init {
                command.arg("--init");
            }
        }

        command.arg(image);

        if let Some(cmd) = cmd {
            command.arg(cmd);
            if let Some(args) = args {
                for arg in args {
                    command.arg(arg);
                }
            }
        }

        command
    }

    pub async fn create_build_command(
        &self,
        context: PathBuf,
        build_args: Option<HashMap<String, String>>,
        file: Option<PathBuf>,
        secrets: Option<HashMap<String, PathBuf>>,
        target: Option<String>,
        ssh_sockets: Option<Vec<SshSockets>>,
    ) -> Result<String> {
        let mut command = Command::new(&self.binary);
        command.env_clear();
        command.env("DOCKER_BUILDKIT", "1");

        command.arg("build");

        if let Some(vars) = build_args {
            for (name, value) in vars {
                command.arg("--build-arg");
                command.arg(format!("{}={}", name.to_uppercase(), value));
            }
        }

        if let Some(file) = file {
            command.arg("--file");
            command.arg(file);
        }

        if let Some(secrets) = secrets {
            for (id, path) in secrets {
                command.arg("--secret");
                command.arg(format!("id={},src={}", id, path.display()));
            }
        }

        if let Some(ssh_sockets) = ssh_sockets {
            for socket in ssh_sockets {
                command.arg("--ssh");
                match socket {
                    SshSockets::Default => {
                        command.arg("default");
                    }
                    SshSockets::Named { id, path } => {
                        command.arg(format!("{}={}", id, path.display()));
                    }
                }
            }
        }

        if let Some(target) = target {
            command.arg("--target");
            command.arg(target);
        }

        command.arg("--quiet");
        command.arg(context);

        let output = command
            .output()
            .await
            .context("could not run prepare command")?;

        if !output.status.success() {
            bail!("prepare command failed");
        }

        let container_id = String::from_utf8_lossy(&output.stdout);

        Ok(container_id.to_string())
    }

    pub async fn create_pull_command(&self, reference: &RegistrySource) -> Result<String> {
        let image = format!("{}", reference);
        let mut command = Command::new(&self.binary);
        command.env_clear();
        command.arg("pull");
        command.arg(&image);

        let output = command
            .output()
            .await
            .context("could not run prepare command")?;

        if !output.status.success() {
            bail!("prepare command failed");
        }

        Ok(image)
    }
}

#[async_trait]
impl Backend for Docker {
    type Image = (String, ContainerConfig);

    async fn prepare(&self, config: ContainerConfig) -> anyhow::Result<Self::Image> {
        let image_reference = match config.image {
            ImageSource::Registry(ref registry_image) => {
                self.create_pull_command(registry_image).await
            }
            ImageSource::Build(ref build_image) => {
                self.create_build_command(
                    build_image.context.clone(),
                    None,
                    build_image.container_file.clone(),
                    None,
                    None,
                    None,
                )
                .await
            }
        }
        .context("could not prepare image")?;

        Ok((image_reference, config))
    }

    async fn spawn(
        &self,
        image: Self::Image,
        stdin: Stdio,
        stdout: Stdio,
        stderr: Stdio,
    ) -> anyhow::Result<()> {
        let mut command =
            self.create_run_command(image.0, None, None, None, None, None, None, None, None);
        command
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .context("could not start run command")?
            .wait()
            .await
            .context("could not run run command")?;

        Ok(())
    }
}

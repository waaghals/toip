use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use regex::Regex;
use tokio::process::Command;
use which::which;

use crate::backend::{BuildArg, Driver, EnvVar, Image, Mount, Secret, Ssh};
use crate::config::{Reference, RegistrySource};

pub struct DockerCliCompatible {
    binary: PathBuf,
    argument: Option<PathBuf>,
    socket: Option<PathBuf>,
}

pub struct DockerImage(String);

impl Image for DockerImage {
    fn id(&self) -> String {
        self.0.clone()
    }
}

impl DockerCliCompatible {
    pub fn resolve_with_supported_binary() -> Result<Self> {
        // TODO, make this more robust
        // Should also configure docker's context (where applicable)
        let clients = vec!["colima", "lima", "nerdctl", "docker", "podman"];
        let first_supported = clients
            .into_iter()
            .map(|client| (client, which(client)))
            .find(|(_client, binary)| binary.is_ok());

        let (client, binary) =
            first_supported.ok_or_else(|| anyhow!("No supported driver installed in $PATH"))?;
        log::info!("using client `{}`", client);

        Ok(match client {
            "colima" => DockerCliCompatible {
                binary: binary.unwrap(),
                argument: Some("nerdctl".into()),
                socket: None,
            },
            "lima" => DockerCliCompatible {
                binary: binary.unwrap(),
                argument: Some("nerdctl".into()),
                socket: None,
            },
            _ => DockerCliCompatible {
                binary: binary.unwrap(),
                argument: None,
                socket: None,
            },
        })
    }
}

// TODO remove impl as resolve_with_supported_binary is fallible
impl Default for DockerCliCompatible {
    fn default() -> Self {
        DockerCliCompatible::resolve_with_supported_binary().unwrap()
    }
}

#[async_trait]
impl Driver for DockerCliCompatible {
    async fn path(&self, repository: &str, reference: &Reference) -> Result<Option<String>> {
        let mut command = Command::new(&self.binary);
        if let Some(argument) = &self.argument {
            command.arg(argument);
        }

        command.arg("inspect");
        command.arg("--format={{json .Config.Env}}");
        match reference {
            Reference::Digest(digest) => command.arg(format!("{}@{}", repository, digest)),
            Reference::Tag(tag) => command.arg(format!("{}:{}", repository, tag)),
        };

        command.stdin(Stdio::null());
        command.stderr(Stdio::null());

        let output = command
            .output()
            .await
            .context("could not run inspect command to determine path")?;

        let output_utf8 = String::from_utf8_lossy(&output.stdout);
        let regex = Regex::new(r#"PATH=([^"]+)"#).unwrap();
        let captures = regex.captures(&output_utf8);
        let path = captures
            .and_then(|captures| {
                let capture = captures.get(1);
                capture.map(|capture| capture.as_str())
            })
            .map(|path| path.to_string());

        Ok(path)
    }

    async fn pull(&self, image: &RegistrySource) -> Result<()> {
        let mut pull_command = Command::new(&self.binary);
        if let Some(argument) = &self.argument {
            pull_command.arg(argument);
        }
        pull_command.env_clear();
        pull_command.arg("pull");
        pull_command.arg(format!("{}", image));

        pull_command.stdin(Stdio::null());
        pull_command.stdout(Stdio::null());
        pull_command.stderr(Stdio::null());

        let status = pull_command
            .status()
            .await
            .context("could not run pull command")?;

        if !status.success() {
            bail!("pull command failed");
        }

        Ok(())
    }

    async fn build<C, F>(
        &self,
        context: C,
        file: F,
        build_args: Vec<BuildArg>,
        secrets: Vec<Secret>,
        ssh_sockets: Vec<Ssh>,
        target: Option<String>,
        repository: &str,
        reference: &Reference,
    ) -> Result<()>
    where
        C: AsRef<Path> + Send,
        F: AsRef<Path> + Send,
    {
        let mut command = Command::new(&self.binary);
        command.env_clear();
        command.env("DOCKER_BUILDKIT", "1");
        if let Some(argument) = &self.argument {
            command.arg(argument);
        }
        command.arg("build");

        for build_arg in build_args {
            command.arg("--build-arg");
            command.arg(format!(
                "{}={}",
                build_arg.name.to_uppercase(),
                build_arg.value
            ));
        }

        command.arg("--file");
        command.arg(file.as_ref());

        for secret in secrets {
            command.arg("--secret");
            command.arg(format!("id={},src={}", secret.id, secret.path.display()));
        }

        for socket in ssh_sockets {
            command.arg("--ssh");
            command.arg(format!("{}={}", socket.id, socket.path.display()));
        }

        if let Some(target) = target {
            command.arg("--target");
            command.arg(target);
        }

        command.arg("--tag");
        match reference {
            Reference::Digest(digest) => command.arg(format!("{}@{}", repository, digest)),
            Reference::Tag(tag) => command.arg(format!("{}:{}", repository, tag)),
        };

        command.arg("--quiet");
        command.arg(context.as_ref());
        command.stdin(Stdio::null());
        command.stderr(Stdio::null());

        let output = command
            .output()
            .await
            .context("could not run prepare command")?;

        if !output.status.success() {
            println!("{}", String::from_utf8_lossy(&output.stderr));
            bail!("prepare command failed");
        }

        Ok(())
    }

    async fn run(
        &self,
        repository: &str,
        reference: &Reference,
        mounts: Vec<Mount>,
        entrypoint: Option<String>,
        cmd: Option<String>,
        args: Option<Vec<String>>,
        env_vars: Vec<EnvVar>,
        env_files: Vec<PathBuf>,
        workdir: Option<PathBuf>,
        init: Option<bool>,
        stdin: Stdio,
        stdout: Stdio,
        stderr: Stdio,
    ) -> Result<()> {
        let mut command = Command::new(&self.binary);
        command.env_clear();

        command.arg("run");
        command.arg("--rm");
        command.arg("-it");

        command.arg("--pull");
        command.arg("never");

        for env_var in env_vars {
            command.arg("--env");
            command.arg(format!("{}={}", env_var.name.to_uppercase(), env_var.value));
        }

        for env_file in env_files {
            command.arg("--env-file");
            command.arg(env_file);
        }

        for mount in mounts {
            command.arg("--mount");

            let mut arg = format!(
                "type=bind,target={},source={}",
                mount.target.display(),
                mount.source.display(),
            );
            arg.push_str(format!(",consistency={}", mount.consistency).as_str());
            arg.push_str(format!(",bind-propagation={}", mount.propagation).as_str());
            arg.push_str(
                format!(
                    ",bind-nonrecursive={}",
                    if mount.non_recursive.is_non_recursive() {
                        "true"
                    } else {
                        "false"
                    }
                )
                .as_str(),
            );
            command.arg(arg);
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

        match reference {
            Reference::Digest(digest) => command.arg(format!("{}@{}", repository, digest)),
            Reference::Tag(tag) => command.arg(format!("{}:{}", repository, tag)),
        };

        if let Some(cmd) = cmd {
            command.arg(cmd);
        }
        if let Some(args) = args {
            for arg in args {
                command.arg(arg);
            }
        }

        log::trace!("{:#?}", command);
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

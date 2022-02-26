use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use regex::Regex;
use tokio::process::Command;
use which::which;

use crate::backend::{BuildArg, Driver, EnvVar, Image, Mount, MountType, Secret, Ssh};
use crate::oci::image::Reference;

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
            first_supported.ok_or(anyhow!("No supported driver installed in $PATH"))?;
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
    type Image = DockerImage;

    async fn path(&self, image: &Self::Image) -> Result<Option<String>> {
        let mut command = Command::new(&self.binary);
        if let Some(argument) = &self.argument {
            command.arg(argument);
        }

        command.arg("inspect");
        command.arg("--format={{json .Config.Env}}");
        command.arg(image.id());

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
            .map(|captures| {
                let capture = captures.get(1);
                capture.map(|capture| capture.as_str())
            })
            .flatten()
            .map(|path| path.to_string());

        Ok(path)
    }

    async fn pull(
        &self,
        registry: &str,
        repository: &str,
        reference: &Reference,
    ) -> Result<Self::Image> {
        let name = match reference {
            Reference::Digest(digest) => format!("{}/{}@{}", registry, repository, digest),
            Reference::Tag(tag) => format!("{}/{}:{}", registry, repository, tag),
        };
        let mut pull_command = Command::new(&self.binary);
        if let Some(argument) = &self.argument {
            pull_command.arg(argument);
        }
        pull_command.env_clear();
        pull_command.arg("pull");
        pull_command.arg(&name);

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

        let mut inspect_command = Command::new(&self.binary);
        if let Some(argument) = &self.argument {
            inspect_command.arg(argument);
        }
        inspect_command.arg("inspect");
        inspect_command.arg("--format={{.Id}}");
        inspect_command.arg(&name);

        inspect_command.stdin(Stdio::null());
        inspect_command.stderr(Stdio::null());

        let output = inspect_command
            .output()
            .await
            .context("could not run inspect command")?;

        if !output.status.success() {
            bail!("inspect command failed");
        }

        let output_utf8 = String::from_utf8_lossy(&output.stdout);
        let image_id = match output_utf8.strip_suffix("\n") {
            None => output_utf8,
            Some(trimmed) => Cow::Borrowed(trimmed),
        };

        Ok(DockerImage(image_id.to_string()))
    }

    async fn build<C, F>(
        &self,
        context: C,
        file: F,
        build_args: Vec<BuildArg>,
        secrets: Vec<Secret>,
        ssh_sockets: Vec<Ssh>,
        target: Option<String>,
    ) -> Result<Self::Image>
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

        let output_utf8 = String::from_utf8_lossy(&output.stdout);
        let image_id = match output_utf8.strip_suffix("\n") {
            None => output_utf8,
            Some(trimmed) => Cow::Borrowed(trimmed),
        };

        Ok(DockerImage(image_id.to_string()))
    }

    async fn run(
        &self,
        image: Self::Image,
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
        dbg!(&args);

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

        command.arg(image.id());

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

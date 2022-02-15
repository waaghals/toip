pub mod driver;
pub mod script;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::{env, fmt};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use const_format::formatcp;
use tokio::process::Command;

use crate::backend::driver::{DockerCliCompatible, Driver};
use crate::config::{ContainerConfig, ImageSource, RegistrySource};
use crate::dirs;
use crate::metadata::APPLICATION_NAME;
use crate::oci::image::Reference;

const CONTAINER_BIN_DIR: &str = formatcp!("/usr/bin/{}", APPLICATION_NAME);
const CONTAINER_BINARY: &str = formatcp!("{}/{}", CONTAINER_BIN_DIR, APPLICATION_NAME);
const CONTAINER_SOCKET: &str = formatcp!("/run/{}/sock", APPLICATION_NAME);

pub enum BindPropagation {
    Shared,
    Slave,
    Private,
    Rshared,
    Rslave,
    Rprivate,
}

impl Default for BindPropagation {
    fn default() -> Self {
        BindPropagation::Rprivate
    }
}

impl fmt::Display for BindPropagation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BindPropagation::Shared => write!(f, "shared"),
            BindPropagation::Slave => write!(f, "slave"),
            BindPropagation::Private => write!(f, "private"),
            BindPropagation::Rshared => write!(f, "rshared"),
            BindPropagation::Rslave => write!(f, "rslave"),
            BindPropagation::Rprivate => write!(f, "rprivate"),
        }
    }
}

pub enum BindConsistency {
    Consistent,
    Cached,
    Delegated,
}

impl Default for BindConsistency {
    fn default() -> Self {
        BindConsistency::Consistent
    }
}

impl fmt::Display for BindConsistency {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BindConsistency::Consistent => write!(f, "consistent"),
            BindConsistency::Cached => write!(f, "cached"),
            BindConsistency::Delegated => write!(f, "delegated"),
        }
    }
}

pub struct BindNonRecursive(bool);

impl Default for BindNonRecursive {
    fn default() -> Self {
        BindNonRecursive(false)
    }
}

impl From<BindNonRecursive> for bool {
    fn from(bind_non_recursive: BindNonRecursive) -> bool {
        bind_non_recursive.0
    }
}

impl BindNonRecursive {
    fn is_non_recursive(&self) -> bool {
        self.0
    }
}

enum MountType {
    Volume {
        source: Option<String>,
    },
    Bind {
        source: PathBuf,
        consistency: BindConsistency,
        bind_propagation: BindPropagation,
        bind_nonrecursive: BindNonRecursive,
    },
    Tmpfs,
}

pub struct Mount {
    mount_type: MountType,
    target: PathBuf,
    readonly: bool,
}

pub struct Secret {
    id: String,
    path: PathBuf,
}

pub struct Ssh {
    id: String,
    path: PathBuf,
}

pub struct BuildArg {
    name: String,
    value: String,
}

#[derive(Debug)]
pub struct EnvVar {
    name: String,
    value: String,
}

pub struct Backend<D>
where
    D: Driver,
{
    driver_name: String,
    current_exe: PathBuf,
    socket: PathBuf,
    driver: D,
}

pub trait Image {
    fn id(&self) -> String;
}

// TODO allow driver to be configured
// TODO allow driver to have custom configuration
impl<D> Default for Backend<D>
where
    D: Default + Driver,
{
    fn default() -> Self {
        let current_exe = env::current_exe().unwrap();

        Backend {
            driver_name: String::from("docker"),
            current_exe,
            socket: "".into(),
            driver: D::default(),
        }
    }
}

impl<D> Backend<D>
where
    D: Driver,
{
    pub fn new<N, S>(driver_name: N, socket: S, driver: D) -> Self
    where
        N: Into<String>,
        S: Into<PathBuf>,
    {
        let current_exe = env::current_exe().unwrap();

        Backend {
            driver_name: driver_name.into(),
            current_exe,
            socket: socket.into(),
            driver,
        }
    }

    fn image_bin_dir(&self, image_id: String) -> Result<PathBuf> {
        // TODO has image_id as it can be quite large
        let image_dir = dirs::image(&self.driver_name, image_id)?;
        let mut bin_dir = image_dir.clone();
        bin_dir.push("bin");

        Ok(bin_dir)
    }

    pub async fn prepare(&self, config: &ContainerConfig) -> anyhow::Result<D::Image> {
        let image = match config.image {
            ImageSource::Registry(ref registry_image) => {
                self.driver
                    .pull(
                        &registry_image.registry,
                        &registry_image.repository,
                        &registry_image.reference,
                    )
                    .await
            }
            ImageSource::Build(ref build_image) => {
                let file = match &build_image.container_file {
                    None => PathBuf::from("Dockerfile"),
                    Some(file) => file.clone(),
                };
                self.driver
                    .build(&build_image.context, file, vec![], vec![], vec![], None)
                    .await
            }
        }
        .context("could not prepare image")?;

        let image_id = image.id();
        let bin_dir = self.image_bin_dir(image_id)?;

        // TODO if image_dir exists, skip creation of scripts
        dirs::create(&bin_dir)
            .with_context(|| format!("could not create directory `{}`", bin_dir.display()))?;

        log::trace!("adding linked container to bin directory");
        if let Some(links) = &config.links {
            for (name, target) in links {
                let mut script_path = bin_dir.clone();
                script_path.push(&name);

                log::debug!(
                    "creating binary `{}` linked to container `{}` at `{}`",
                    name,
                    target,
                    script_path.to_str().unwrap()
                );
                script::create_call(&script_path, CONTAINER_BINARY, target.as_str())
                    .context("could not create call script")?;
            }
        };

        Ok(image)
    }

    fn create_mounts(&self, image_bin_dir: PathBuf) -> Vec<Mount> {
        vec![
            Mount {
                mount_type: MountType::Bind {
                    source: image_bin_dir,
                    consistency: BindConsistency::default(),
                    bind_propagation: BindPropagation::default(),
                    bind_nonrecursive: BindNonRecursive::default(),
                },
                target: CONTAINER_BIN_DIR.into(),
                readonly: true,
            },
            Mount {
                mount_type: MountType::Bind {
                    source: self.current_exe.clone(),
                    consistency: BindConsistency::default(),
                    bind_propagation: BindPropagation::default(),
                    bind_nonrecursive: BindNonRecursive::default(),
                },
                target: CONTAINER_BINARY.into(),
                readonly: true,
            },
            Mount {
                mount_type: MountType::Bind {
                    source: self.socket.clone(),
                    consistency: BindConsistency::default(),
                    bind_propagation: BindPropagation::default(),
                    bind_nonrecursive: BindNonRecursive::default(),
                },
                target: CONTAINER_SOCKET.into(),
                readonly: true,
            },
        ]
    }

    fn create_env_vars(&self, path: &String, config: &ContainerConfig) -> Vec<EnvVar> {
        let mut envs = vec![];
        if let Some(env_vars) = &config.env {
            for (name, value) in env_vars {
                envs.push(EnvVar {
                    name: name.clone(),
                    value: value.clone(),
                });
            }
        }

        envs.push(EnvVar {
            name: "TOIP_SOCK".to_string(),
            value: CONTAINER_SOCKET.to_string(),
        });

        envs.push(EnvVar {
            name: "path".to_string(),
            value: path.clone(),
        });

        envs
    }

    pub async fn spawn(
        &self,
        image: D::Image,
        config: &ContainerConfig,
        args: Vec<String>,
        stdin: Stdio,
        stdout: Stdio,
        stderr: Stdio,
    ) -> anyhow::Result<()> {
        let image_id = image.id();
        let image_bin_dir = self.image_bin_dir(image_id)?;
        let mounts = self.create_mounts(image_bin_dir);

        let path = self
            .driver
            .path(&image)
            .await
            .context("could not determine PATH")?
            .map_or(CONTAINER_BINARY.into(), |some| {
                format!("{}:{}", CONTAINER_BIN_DIR, &some)
            });

        let env_vars = self.create_env_vars(&path, &config);

        let cmd = config.cmd.clone();
        // TODO decide what to do with arguments? Does it make sense to configure them?
        // let args = args.or(config.args.clone());
        let entrypoint = config.entrypoint.clone();
        let workdir = config.workdir.clone();

        self.driver
            .run(
                image,
                mounts,
                entrypoint,
                cmd,
                Some(args),
                env_vars,
                vec![],
                workdir,
                None,
                stdin,
                stdout,
                stderr,
            )
            .await?;

        Ok(())
    }
}

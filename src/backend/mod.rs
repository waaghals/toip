pub mod driver;
pub mod script;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::{env, fmt, fs};

use anyhow::{anyhow, bail, Context, Result};
use const_format::formatcp;
use sha2::{Digest, Sha256};

use crate::backend::driver::Driver;
use crate::config::{Config, ContainerConfig, Reference, Volume};
use crate::metadata::APPLICATION_NAME;
use crate::{config, dirs};

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

#[derive(Default, Debug, Clone)]
pub struct BindNonRecursive(bool);

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
    // Volume {
    //     source: Option<String>,
    // },
    Bind {
        source: PathBuf,
        consistency: BindConsistency,
        bind_propagation: BindPropagation,
        bind_nonrecursive: BindNonRecursive,
    },
    // Tmpfs,
}

pub struct Mount {
    source: PathBuf,
    consistency: BindConsistency,
    propagation: BindPropagation,
    non_recursive: BindNonRecursive,
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
    D: Driver + std::marker::Sync,
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

    fn image_bin_dir<C>(&self, config_dir: C) -> Result<PathBuf>
    where
        C: AsRef<OsStr>,
    {
        let image_dir = dirs::image(&self.driver_name, config_dir)?;
        let mut bin_dir = image_dir;
        bin_dir.push("bin");

        Ok(bin_dir)
    }

    fn image_id<P>(&self, config_dir: P, container_name: &str) -> Result<String>
    where
        P: AsRef<OsStr>,
    {
        let digest = config::hash(config_dir)?;
        Ok(format!("{}-{}", digest, container_name))
    }

    pub async fn prepare<P>(
        &self,
        container_name: &str,
        config: &ContainerConfig,
        config_dir: P,
    ) -> anyhow::Result<()>
    where
        P: AsRef<OsStr>,
    {
        if let Some(build) = &config.build {
            // TODO tag using image when defined
            let file = match &build.file {
                None => {
                    let mut path = build.context.clone();
                    path.push("Dockerfile");
                    path
                }
                Some(file) => file.clone(),
            };

            let build_args = build
                .build_args
                .iter()
                .map(|(key, value)| BuildArg {
                    name: key.clone(),
                    value: value.clone().into_inner(),
                })
                .collect();

            let secrets = build
                .secrets
                .iter()
                .map(|(key, value)| Secret {
                    id: key.clone(),
                    path: value.clone().into_inner(),
                })
                .collect();

            let ssh = build
                .ssh
                .iter()
                .map(|(key, value)| Ssh {
                    id: key.clone(),
                    path: value.clone().into_inner(),
                })
                .collect();

            let reference = match &config.image {
                None => Reference::default(),
                Some(image) => image.reference.clone(),
            };

            let repository = match &config.image {
                None => self.image_id(&config_dir, container_name)?,
                Some(image) => image.repository.clone(),
            };

            self.driver
                .build(
                    &build.context,
                    file,
                    build_args,
                    secrets,
                    ssh,
                    build.target.clone(),
                    &repository,
                    &reference,
                )
                .await
                .with_context(|| {
                    format!(
                        "could not build image from build context `{}`",
                        &build.context.display()
                    )
                })?;
        } else if let Some(image) = &config.image {
            self.driver
                .pull(image)
                .await
                .with_context(|| format!("could not pull image `{}`", &image))?;
        } else {
            bail!("missing image or build config");
        };

        let bin_dir = self.image_bin_dir(&config_dir)?;

        // TODO if image_dir exists, skip creation of scripts
        dirs::create(&bin_dir)
            .with_context(|| format!("could not create directory `{}`", bin_dir.display()))?;

        log::trace!("adding linked container to bin directory");

        for (name, target) in &config.links {
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

        Ok(())
    }

    fn create_mounts<P>(
        &self,
        image_bin_dir: PathBuf,
        volumes: HashMap<PathBuf, Volume>,
        config_dir: P,
    ) -> Result<Vec<Mount>>
    where
        P: Into<PathBuf>,
    {
        let mut mounts = vec![
            Mount {
                source: image_bin_dir,
                consistency: Default::default(),
                propagation: Default::default(),
                non_recursive: Default::default(),
                target: CONTAINER_BIN_DIR.into(),
                readonly: true,
            },
            Mount {
                source: self.current_exe.clone(),
                consistency: Default::default(),
                propagation: Default::default(),
                non_recursive: Default::default(),
                target: CONTAINER_BINARY.into(),
                readonly: true,
            },
            Mount {
                source: self.socket.clone(),
                consistency: Default::default(),
                propagation: Default::default(),
                non_recursive: Default::default(),
                target: CONTAINER_SOCKET.into(),
                readonly: true,
            },
        ];

        let config_dir = config_dir.into();
        for (destination, volume) in volumes {
            match volume {
                Volume::Anonymous(anonymous) => {
                    let seed = if anonymous.external {
                        None
                    } else {
                        Some(config_dir.clone())
                    };
                    let directory = dirs::volume(anonymous.name, seed.as_ref())?;
                    fs::create_dir_all(&directory).with_context(|| {
                        format!(
                            "could not create volume directory `{}`",
                            directory.display()
                        )
                    })?;
                    mounts.push(Mount {
                        source: directory,
                        consistency: Default::default(),
                        propagation: Default::default(),
                        non_recursive: Default::default(),
                        target: destination.clone(),
                        readonly: false,
                    });
                }
                Volume::Bind(bind) => {
                    let path = bind.source.as_ref();
                    let source = if path.is_absolute() {
                        path.to_path_buf()
                    } else {
                        let mut config_dir = config_dir.clone();
                        config_dir.push(path);
                        config_dir
                    };
                    mounts.push(Mount {
                        source,
                        consistency: Default::default(),
                        propagation: Default::default(),
                        non_recursive: Default::default(),
                        target: destination.clone(),
                        readonly: false,
                    });
                }
            }
        }

        Ok(mounts)
    }

    fn create_env_vars(&self, path: String, config: &ContainerConfig) -> Vec<EnvVar> {
        let mut envs = vec![];
        for (name, value) in &config.env {
            envs.push(EnvVar {
                name: name.clone(),
                value: value.clone().into_inner(),
            });
        }

        envs.push(EnvVar {
            name: "TOIP_SOCK".to_string(),
            value: CONTAINER_SOCKET.to_string(),
        });

        envs.push(EnvVar {
            name: "path".to_string(),
            value: path,
        });

        envs
    }

    pub async fn spawn(
        &self,
        config: &Config,
        container_name: &str,
        container_config: &ContainerConfig,
        config_dir: &Path,
        args: Vec<String>,
        stdin: Stdio,
        stdout: Stdio,
        stderr: Stdio,
    ) -> anyhow::Result<()> {
        let image_bin_dir = self.image_bin_dir(&config_dir)?;

        let mut volumes = HashMap::new();
        for (destination, volume_name) in &container_config.volumes {
            let volume = config
                .volumes
                .get(volume_name.as_str())
                .ok_or_else(|| anyhow!("missing volume `{}` in config", volume_name))?;
            volumes.insert(destination.clone(), volume.clone());
        }

        let mounts = self
            .create_mounts(image_bin_dir, volumes, config_dir)
            .context("could not configure mounts")?;

        let reference = match &container_config.image {
            None => Reference::default(),
            Some(image) => image.reference.clone(),
        };

        let repository = match &container_config.image {
            None => self.image_id(config_dir, container_name)?,
            Some(image) => image.repository.clone(),
        };

        let path = self
            .driver
            .path(&repository, &reference)
            .await
            .context("could not determine PATH")?
            .map_or(CONTAINER_BINARY.into(), |some| {
                format!("{}:{}", CONTAINER_BIN_DIR, &some)
            });

        let env_vars = self.create_env_vars(path, container_config);

        let cmd = container_config.cmd.clone();
        let mut all_args = container_config.args.clone();
        all_args.extend(args);
        let entrypoint = container_config.entrypoint.clone();
        let workdir = container_config.workdir.clone();

        log::info!(
            "Running container from image `{}/{}`",
            repository,
            reference
        );
        self.driver
            .run(
                &repository,
                &reference,
                mounts,
                entrypoint,
                cmd,
                Some(all_args),
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

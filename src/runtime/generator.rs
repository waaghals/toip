use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use async_trait::async_trait;
use const_format::formatcp;
use nix::unistd::{self, Gid, Group, Uid, User};
use oci_spec::{
    Linux,
    LinuxIdMapping,
    LinuxNamespace,
    LinuxNamespaceType,
    Mount,
    Process,
    Root,
    Spec,
};

use crate::backend::script;
use crate::config::ContainerConfig;
use crate::dirs;
use crate::image::manager::ImageManager;
use crate::metadata::APPLICATION_NAME;
use crate::oci::image::Image;

const CONTAINER_BIN_DIR: &str = formatcp!("/usr/bin/{}", APPLICATION_NAME);
const CONTAINER_BINARY: &str = formatcp!("{}/{}", CONTAINER_BIN_DIR, APPLICATION_NAME);
const CONTAINER_SOCKET: &str = formatcp!("/run/{}/sock", APPLICATION_NAME);

#[async_trait]
pub trait RuntimeBundleGenerator {
    async fn build<C, A, S>(
        &self,
        container_id: C,
        config: &ContainerConfig,
        arguments: A,
        socket_path: S,
    ) -> Result<PathBuf>
    where
        C: Into<String> + Send,
        A: IntoIterator<Item = String> + Send,
        S: Into<PathBuf> + Send;
}

#[derive(Debug, Clone, Default)]
pub struct RunGenerator {}

fn container_path(container_dir: &PathBuf, path: &str) -> Result<PathBuf> {
    let mut dir = container_dir.clone();
    dir.push(path);
    dirs::create(&dir)?;
    Ok(dir)
}

fn overlay_option<S>(kind: S, paths: &[PathBuf]) -> String
where
    S: Into<String> + Send,
{
    let mut option = kind.into();
    option.push('=');
    let str_paths: Vec<String> = paths
        .iter()
        .cloned() // TODO cleanup this mess
        .map(|path| path.into_os_string())
        .map(|os_string| os_string.into_string().unwrap())
        .collect();
    let joined = str_paths.join(":");
    option.push_str(&joined);

    option
}

// TODO improve error handling here
// TODO cleanup nesting
fn resolve_user(image: Image) -> (Uid, Gid) {
    let root_user = Uid::from_raw(0);
    let root_group = Gid::from_raw(0);

    match image.config {
        Some(config) => match config.user {
            Some(user) => {
                let mut parts = user.split(':');
                let _user_name_or_id = parts.next().unwrap();
                let group_name_or_id = parts.next();

                let user_obj: Option<User> = None;
                // TODO fix without nix. Nix is build against glibc which does not work when statically compiled.
                // let user_obj = User::from_name(user_name_or_id).expect("Could not get user");

                let group_obj: Option<Group> = None;
                // if let Some(group_name_or_id) = group_name_or_id {
                //     group_obj = Group::from_name(group_name_or_id).unwrap();
                // }

                let uid = user_obj.map(|u| u.uid).map_or(root_user, |u| u);
                let gid = group_obj
                    .map(|g| g.gid)
                    .or_else(|| group_name_or_id.map(|g| Gid::from_raw(g.parse::<u32>().unwrap())))
                    .map_or(root_group, |g| g);

                (uid, gid)
            }
            None => (root_user, root_group),
        },
        None => (root_user, root_group),
    }
}

fn build_linux(image: Image) -> Linux {
    let host_uid = unistd::getuid();
    let host_gid = unistd::getgid();

    let image_user = resolve_user(image);

    Linux {
        uid_mappings: Some(vec![LinuxIdMapping {
            container_id: image_user.0.into(),
            host_id: host_uid.into(),
            size: 1,
        }]),
        gid_mappings: Some(vec![LinuxIdMapping {
            container_id: image_user.1.into(),
            host_id: host_gid.into(),
            size: 1,
        }]),
        namespaces: Some(vec![
            LinuxNamespace {
                typ: LinuxNamespaceType::Mount,
                path: None,
            },
            LinuxNamespace {
                typ: LinuxNamespaceType::Uts,
                path: None,
            },
            LinuxNamespace {
                typ: LinuxNamespaceType::Ipc,
                path: None,
            },
            LinuxNamespace {
                typ: LinuxNamespaceType::User,
                path: None,
            },
            LinuxNamespace {
                typ: LinuxNamespaceType::Pid,
                path: None,
            },
        ]),
        ..Default::default()
    }
}

fn build_mounts(
    lower_dirs: Vec<PathBuf>,
    upper_dir: PathBuf,
    work_dir: PathBuf,
    bin_dir: PathBuf,
    executable_path: PathBuf,
    socket_path: PathBuf,
) -> Vec<Mount> {
    let mounts = vec![
        Mount {
            destination: "/".into(),
            typ: Some("overlay".into()),
            source: Some("overlay".into()),
            options: Some(vec![
                overlay_option("lowerdir", &lower_dirs),
                overlay_option("upperdir", &[upper_dir]),
                overlay_option("workdir", &[work_dir]),
            ]),
        },
        Mount {
            destination: "/proc".into(),
            typ: Some("proc".into()),
            source: Some("proc".into()),
            options: None,
        },
        Mount {
            destination: "/dev".into(),
            typ: Some("tmpfs".into()),
            source: Some("tmpfs".into()),
            options: Some(vec![
                "nosuid".into(),
                "strictatime".into(),
                "mode=755".into(),
                "size=65536k".into(),
            ]),
        },
        Mount {
            destination: "/dev/pts".into(),
            typ: Some("devpts".into()),
            source: Some("devpts".into()),
            options: Some(vec![
                "nosuid".into(),
                "noexec".into(),
                "newinstance".into(),
                "ptmxmode=0666".into(),
                "mode=0620".into(),
            ]),
        },
        Mount {
            destination: "/dev/shm".into(),
            typ: Some("tmpfs".into()),
            source: Some("shm".into()),
            options: Some(vec![
                "nosuid".into(),
                "noexec".into(),
                "nodev".into(),
                "nodev".into(),
                "mode=1777".into(),
                "size=65536k".into(),
            ]),
        },
        Mount {
            destination: "/dev/mqueue".into(),
            typ: Some("mqueue".into()),
            source: Some("mqueue".into()),
            options: Some(vec!["nosuid".into(), "noexec".into(), "nodev".into()]),
        },
        Mount {
            destination: "/sys".into(),
            typ: Some("none".into()),
            source: Some("/sys".into()),
            options: Some(vec![
                "rbind".into(),
                "nosuid".into(),
                "noexec".into(),
                "nodev".into(),
                "ro".into(),
            ]),
        },
        Mount {
            destination: "/sys/fs/cgroup".into(),
            typ: Some("cgroup".into()),
            source: Some("cgroup".into()),
            options: Some(vec![
                "nosuid".into(),
                "noexec".into(),
                "nodev".into(),
                "relatime".into(),
                "ro".into(),
            ]),
        },
        Mount {
            destination: CONTAINER_BIN_DIR.into(),
            typ: Some("bind".into()),
            source: bin_dir.into(),
            options: Some(vec!["rbind".into(), "rw".into()]),
        },
        Mount {
            destination: CONTAINER_BINARY.into(),
            typ: Some("bind".into()),
            source: executable_path.into(),
            options: Some(vec!["rbind".into(), "rw".into()]),
        },
        Mount {
            destination: CONTAINER_SOCKET.into(),
            typ: Some("bind".into()),
            source: socket_path.into(),
            options: Some(vec!["rbind".into(), "rw".into()]),
        },
    ];
    mounts
}

// TODO cleanup this clone mess, clone is required, because we need owned values, but could be improved I think.
fn build_cmd<I>(image: Image, config: ContainerConfig, arguments: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let image_config = image.config;
    let image_entrypoint = image_config
        .as_ref()
        .map(|ic| ic.entrypoint.clone())
        .flatten();
    let entrypoint = config.entrypoint.or(image_entrypoint);

    let mut actual_cmd = Vec::new();

    if let Some(entrypoint) = entrypoint {
        actual_cmd.extend(entrypoint);
    }

    let image_cmd = image_config.map(|image_config| image_config.cmd).flatten();
    let cmd = config.cmd.or(image_cmd);

    if let Some(cmd) = cmd {
        actual_cmd.extend(cmd);
    }

    if actual_cmd.is_empty() {
        actual_cmd.push("sh".to_string());
    }

    actual_cmd.extend(arguments);

    actual_cmd
}

fn build_env(image: Image, config: ContainerConfig) -> Vec<String> {
    // Convert image keys=value to hashmap
    let mut map = HashMap::new();
    if let Some(envs) = image.config.map(|c| c.env).flatten() {
        for env in envs {
            let mut parts = env.splitn(2, '=');
            map.insert(
                parts.next().unwrap().to_string(),
                parts.next().unwrap().to_string(),
            );
        }
    }

    if let Some(envs) = config.env {
        map.extend(envs);
    }

    // TODO add inherted envvars from calling process
    let bin_dir = CONTAINER_BIN_DIR.to_string();
    let new_path = match map.get("PATH") {
        Some(current_path) => format!("{}:{}", current_path, bin_dir),
        None => bin_dir,
    };

    map.insert("PATH".to_string(), new_path);
    map.insert("TOIP_SOCK".to_string(), CONTAINER_SOCKET.to_string());

    map.into_iter()
        .map(|env| format!("{}={}", env.0, env.1))
        .collect()
}

fn build_rootless_runtime_bundle<A>(
    image: &Image,
    config: &ContainerConfig,
    mounts: Vec<Mount>,
    arguments: A,
) -> Spec
where
    A: IntoIterator<Item = String>,
{
    let root = Root {
        readonly: Some(false),
        ..Default::default()
    };

    // TODO overuse of clone, make this more efficient?
    let process = Process {
        args: build_cmd(image.clone(), config.clone(), arguments).into(),
        env: build_env(image.clone(), config.clone()).into(),
        ..Default::default()
    };

    Spec {
        version: "1.0.2-dev".into(),
        root: Some(root),
        mounts: Some(mounts),
        process: Some(process),
        hostname: None,
        hooks: None,
        annotations: None,
        linux: Some(build_linux(image.clone())),
        solaris: None,
        windows: None,
        vm: None,
    }
}

#[async_trait]
impl RuntimeBundleGenerator for RunGenerator {
    async fn build<C, A, S>(
        &self,
        container_name: C,
        config: &ContainerConfig,
        arguments: A,
        socket_path: S,
    ) -> Result<PathBuf>
    where
        C: Into<String> + Send,
        A: IntoIterator<Item = String> + Send,
        S: Into<PathBuf> + Send,
    {
        let container_id: String = container_name.into();
        log::debug!("building runtime bundle for `{}`", container_id);
        let manager = ImageManager::new().context("could not construct `ImageManager`")?;

        log::debug!("prepping image `{}`", &config.image);
        let image = manager.prepare(&config.image).await?;

        let lower_dirs = image
            .rootfs
            .diff_ids
            .iter()
            .map(|diff_id| dirs::layer_dir(&diff_id.algorithm.to_string(), &diff_id.encoded))
            .collect::<Result<Vec<PathBuf>>>()
            .context("could not determin lower dirs")?;

        log::trace!("ensuring container directories exists");
        let container_dir = dirs::container(&container_id)?;
        let upper_dir = container_path(&container_dir, "upper")?;
        let work_dir = container_path(&container_dir, "work")?;
        let bin_dir = container_path(&container_dir, "bin")?;
        container_path(&container_dir, "rootfs")?;

        let mut config_file = container_dir.clone();
        config_file.push("config.json");

        log::trace!("adding linked containers to bin directory");
        if let Some(links) = &config.links {
            for (name, container) in links {
                let mut script_path = bin_dir.clone();
                script_path.push(name);

                log::debug!(
                    "creating binary `{}` linked to container `{}` at `{}`",
                    name,
                    container,
                    script_path.to_str().unwrap()
                );
                script::create_call(&script_path, CONTAINER_BINARY, container)
                    .context("could not create call script")?;
            }
        };

        let current = env::current_exe().context("could not determin current executable")?;

        log::trace!("building mounts");
        let mounts = build_mounts(
            lower_dirs,
            upper_dir,
            work_dir,
            bin_dir,
            current,
            socket_path.into(),
        );

        let spec = build_rootless_runtime_bundle(&image, config, mounts, arguments);
        log::debug!(
            "saving container `{}` runtime bundle spec in `{:?}`",
            container_id,
            config_file
        );
        spec.save(&config_file)?;

        Ok(container_dir)
    }
}

pub struct CallGenerator {}

impl Default for CallGenerator {
    fn default() -> Self {
        CallGenerator {}
    }
}

use std::convert::TryInto;
use std::fs::create_dir_all;
use std::str::FromStr;
use std::{collections::VecDeque, path::PathBuf};

use crate::dirs::container_dir;
use crate::image::manager::ImageManager;
use crate::{config::Container, dirs::layer_dir};
use anyhow::Result;
use async_trait::async_trait;
use oci_spec::{
    Linux, LinuxIdMapping, LinuxNamespace, LinuxNamespaceType, Mount, Process, Root, Spec,
};

// TODO create overlay fs https://www.itopstimes.com/contain/demystifying-a-docker-image/
#[async_trait]
pub trait RuntimeBundleGenerator {
    async fn build(&self, config: &Container) -> Result<PathBuf>;
}

pub struct RunGenerator {
    layer_dir: PathBuf,
    container_dir: PathBuf,
}

impl RunGenerator {
    fn container_dir(&self, container_name: &str) -> PathBuf {
        let mut directory = self.container_dir.clone();
        directory.push(container_name);
        directory
    }

    fn location(&self, container_name: &str, sub_directory: &str) -> PathBuf {
        let mut directory = self.container_dir(container_name);
        directory.push(sub_directory);
        directory
    }
}

impl Default for RunGenerator {
    fn default() -> Self {
        RunGenerator {
            layer_dir: layer_dir(),
            container_dir: container_dir(),
        }
    }
}

fn overlay_option<S>(kind: S, paths: &[PathBuf]) -> String
where
    S: Into<String>,
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

fn create_directories(directories: &[PathBuf]) -> Result<()> {
    for directory in directories {
        create_dir_all(directory)?;
    }
    Ok(())
}

fn build_linux(host: (u32, u32), container: (u32, u32)) -> Linux {
    let mut linux = Linux::default();
    linux.uid_mappings = Some(vec![LinuxIdMapping {
        container_id: container.0,
        host_id: host.0,
        size: 1,
    }]);
    linux.gid_mappings = Some(vec![LinuxIdMapping {
        container_id: container.1,
        host_id: host.1,
        size: 1,
    }]);
    linux.namespaces = Some(vec![
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
    ]);
    linux
}

fn build_mounts(lower_dirs: Vec<PathBuf>, upper_dir: PathBuf, work_dir: PathBuf) -> Vec<Mount> {
    let mounts = vec![Mount {
        destination: "/".into(),
        typ: Some("overlay".into()),
        source: Some("overlay".into()),
        options: Some(vec![
            overlay_option("lowerdir", &lower_dirs),
            overlay_option("upperdir", &[upper_dir]),
            overlay_option("workdir", &[work_dir]),
        ]),
    },Mount {
        destination: "/proc".into(),
        typ: Some("proc".into()),
        source: Some("proc".into()),
        options: None,
    },Mount {
        destination: "/dev".into(),
        typ: Some("tmpfs".into()),
        source: Some("tmpfs".into()),
        options: Some(vec![
            "nosuid".into(),
            "strictatime".into(),
            "mode=755".into(),
            "size=65536k".into(),
        ]),
    },Mount {
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
    },Mount {
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
    },Mount {
        destination: "/dev/mqueue".into(),
        typ: Some("mqueue".into()),
        source: Some("mqueue".into()),
        options: Some(vec![
            "nosuid".into(),
            "noexec".into(),
            "nodev".into(),
        ]),
    },Mount {
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
    },Mount {
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
    }];
    mounts
}

fn build_rootless_runtime_bundle(
    mounts: Vec<Mount>,
    host_user: (u32, u32),
    container_user: (u32, u32),
) -> Spec {
    let mut root = Root::default();
    root.readonly = Some(false);
    Spec {
        version: "1.0.2-dev".into(),
        root: Some(root),
        mounts: Some(mounts),
        process: Some(Process::default()),
        hostname: None,
        hooks: None,
        annotations: None,
        linux: Some(build_linux(host_user, container_user)),
        solaris: None,
        windows: None,
        vm: None,
    }
}

#[async_trait]
impl RuntimeBundleGenerator for RunGenerator {
    async fn build(&self, config: &Container) -> Result<PathBuf> {
        let mut manager = ImageManager::default();
        let image = manager.prepare(&config.image).await?;

        let lower_dirs: Vec<PathBuf> = image
            .rootfs
            .diff_ids
            .iter()
            .map(|diff_id| {
                let mut layer_dir = self.layer_dir.clone();
                layer_dir.push(diff_id.algorithm.to_string());
                layer_dir.push(diff_id.encoded.to_string());
                layer_dir
            })
            .collect();

        create_directories(&lower_dirs)?;

        // TODO move upper dir outside of cache directory
        let container_name = "test-container";
        let upper_dir = self.location(container_name, "upper");
        let work_dir = self.location(container_name, "work");
        let root_fs = self.location(container_name, "rootfs");
        let config_file = self.location(container_name, "config.json");
        create_directories(&[upper_dir.clone(), work_dir.clone(), root_fs])?;

        let mounts = build_mounts(lower_dirs, upper_dir, work_dir);
        let spec = build_rootless_runtime_bundle(mounts, (1000, 1000), (0, 0));
        spec.save(&config_file)?;

        Ok(self.container_dir(container_name))

        // 5. Modify Runtime Bundle config
        // 7. Generate scripts for container's links
        // 8. Mount scripts and current binary
        // 9. Overwrite with any user defined configuration
    }
}

pub struct CallGenerator {}

impl Default for CallGenerator {
    fn default() -> Self {
        CallGenerator {}
    }
}

#[async_trait]
impl RuntimeBundleGenerator for CallGenerator {
    async fn build(&self, config: &Container) -> Result<PathBuf> {
        todo!()
    }
}

use std::convert::TryInto;
use std::str::FromStr;
use std::{collections::VecDeque, path::PathBuf};

use crate::dirs::container_dir;
use crate::image::manager::ImageManager;
use crate::{config::Container, dirs::layer_dir};
use anyhow::Result;
use async_trait::async_trait;
use oci_spec::{Linux, LinuxIdMapping, LinuxNamespace, LinuxNamespaceType, Mount, Spec};

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
    fn sub_dir(&self, container_name: &str, sub_directory: &str) -> String {
        let mut container_dir = self.container_dir.clone();
        container_dir.push(container_name);
        container_dir.push(sub_directory);

        container_dir.into_os_string().into_string().unwrap()
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

#[async_trait]
impl RuntimeBundleGenerator for RunGenerator {
    async fn build(&self, config: &Container) -> Result<PathBuf> {
        let mut manager = ImageManager::default();
        let image = manager.prepare(&config.image).await?;

        // Convert Image config to Runtime Bundle config
        let mut runtime_config = Spec::default();
        let mut linux = Linux::default();
        let host_id = 1000;
        let container_id = 0;
        let size = 1;
        linux.uid_mappings = Some(vec![LinuxIdMapping {
            container_id,
            host_id,
            size,
        }]);
        linux.gid_mappings = Some(vec![LinuxIdMapping {
            container_id,
            host_id,
            size,
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
        runtime_config.linux = Some(linux);

        // Mount rootfs using overlay fs
        let mut mounts = VecDeque::new();
        for mount in runtime_config.mounts.unwrap().into_iter() {
            mounts.push_back(mount);
        }

        let lower: Vec<String> = image
            .rootfs
            .diff_ids
            .iter()
            .map(|diff_id| {
                let mut layer_dir = self.layer_dir.clone();
                layer_dir.push(diff_id.algorithm.to_string());
                layer_dir.push(diff_id.encoded.to_string());
                layer_dir
            })
            .map(|path| path.into_os_string())
            .map(|os_str| os_str.into_string().unwrap())
            .collect();

        let upper = self.sub_dir("some-container-name", "upper");
        let work = self.sub_dir("some-container-name", "work");

        let mut lower_option = String::from_str("lowerdir=")?;
        lower_option.push_str(&lower.join(":"));
        let mut upper_option = String::from_str("upperdir=")?;
        upper_option.push_str(&upper);
        let mut work_option = String::from_str("workdir=")?;
        work_option.push_str(&work);

        mounts.push_front(Mount {
            destination: "/".into(),
            typ: Some("overlay".to_string()),
            source: Some("overlay".into()),
            options: Some(vec![lower_option, upper_option, work_option]),
        });

        runtime_config.mounts = Some(mounts.into());

        let mut config_location = self.container_dir.clone();
        config_location.push("some-container-name");
        config_location.push("bundle");
        runtime_config.save(&config_location);

        Ok(config_location)

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

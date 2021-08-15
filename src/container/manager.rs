use std::collections::HashMap;
use std::env::current_exe;
use std::fs::{create_dir_all, hard_link, metadata, File};
use std::io::prelude::*;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};

use crate::config::{Config, Container, RuntimeConfig};
use crate::container::Runtime;

pub struct Manager {
    pub workdir: PathBuf,
    pub config: Config,
    pub runtime: Runtime,
}

impl Manager {
    fn write_container_script(&self, name: &str, path: &PathBuf, target: &str) -> Result<()> {
        let doe_bin_path = "#!/usr/bin/doe/doe exec\n";
        create_dir_all(&path).with_context(|| format!("Could not create directory {:?}", &path))?;

        let mut path = path.clone();
        path.push(name);

        let mut file =
            File::create(&path).with_context(|| format!("Could not create file {:?}", &path))?;

        let config = RuntimeConfig {
            config: self.config.clone(),
            container_name: target.to_string(),
        };

        let serialized = serde_json::to_string(&config).context("Could not serialize config")?;
        file.write(doe_bin_path.as_bytes())
            .with_context(|| format!("Could not write to file {:?}", &file))?;
        file.write_all(serialized.as_bytes())
            .with_context(|| format!("Could not write to file {:?}", &file))?;

        let mut perms = metadata(&path)
            .with_context(|| format!("Could not read metadata for file {:?}", &path))?
            .permissions();
        perms.set_mode(0o777); //TODO change mode
        Ok(())
    }

    fn generate_scripts(&self, binaries_path: &PathBuf, container: &Container) -> Result<()> {
        if let Some(links) = container.links.clone() {
            for (name, container) in links {
                log::debug!(
                    "Creating binary `{}` linked to container `{}` ",
                    name,
                    container
                );

                self.write_container_script(&name, binaries_path, &container)?
            }
        };
        Ok(())
    }

    fn add_current_exe(&self, binaries_path: &PathBuf) -> Result<()> {
        let mut path = binaries_path.clone();
        path.push("doe");
        let current = current_exe()?;
        hard_link(current, path)?;
        Ok(())
    }

    pub async fn run(&self, alias: &str, args: Vec<&str>) -> Result<()> {
        match self.config.get_container_by_alias(alias) {
            Some(container) => {
                let (name, container) = container?;

                let mut binaries_path = self.workdir.clone();
                binaries_path.push(".doe");
                binaries_path.push(name);

                self.generate_scripts(&binaries_path, container)?;
                self.add_current_exe(&binaries_path)?;
                let mut mounts: HashMap<String, String> =
                    container.volumes.clone().map_or(HashMap::new(), |f| f);

                let path = binaries_path.to_str().unwrap();
                mounts.insert("/usr/bin/doe".to_string(), path.to_string());
                mounts.insert(
                    "/var/run/docker.sock".to_string(),
                    "/var/run/docker.sock".to_string(),
                );

                let mut _args: Vec<String> = args.clone().iter().map(|s| s.to_string()).collect();
                // let mut init_args = vec!["-vvv".to_string(),"init".to_string(), container.cmd.clone()];
                // init_args.append(&mut args);

                self.runtime
                    .run_container(
                        &"".to_string(),
                        // &container.image,
                        // &"bash".to_string(),
                        &String::from("bash"),
                        &None,
                        // &container.cmd,
                        // &Some(args),
                        // &"/usr/bin/doe/doe".to_string(),
                        // &Some(init_args),
                        &Some(mounts),
                        &container.envvars,
                    )
                    .await
            }
            None => Err(anyhow!("No alias `{}` in config", alias)),
        }
    }
}

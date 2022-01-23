use std::env;
use std::fs::File;

use anyhow::{bail, Context, Result};

use crate::config::{find_config_file, Config};
use crate::image::manager::ImageManager;

async fn prepare_config(config: &Config, container: Option<String>) -> Result<()> {
    let image_manager = ImageManager::new().context("could not construct `ImageManager`")?;
    match container {
        Some(name) => {
            let container = config
                .get_container_by_name(&name.as_str())
                .with_context(|| {
                    format!(
                        "container with name `{}` does not exists in configuration",
                        name
                    )
                })?;
            image_manager
                .prepare(&container.image)
                .await
                .with_context(|| format!("could not prepare container `{}`", name))?;
        }
        None => {
            for (name, container) in &config.containers {
                image_manager
                    .prepare(&container.image)
                    .await
                    .with_context(|| format!("could not prepare container `{}`", name))?;
            }
        }
    };

    Ok(())
}

pub async fn prepare(ignore_missing_config: bool, container: Option<String>) -> Result<()> {
    let current_dir = env::current_dir()?;
    let config_path = find_config_file(current_dir);

    match config_path {
        None => {
            if ignore_missing_config {
                Ok(())
            } else {
                bail!("Missing config file");
            }
        }
        Some(file) => {
            let config_file = File::open(&file).with_context(|| {
                format!(
                    "could not open config file `{}` for reading",
                    file.display()
                )
            })?;

            let config = Config::new(config_file).with_context(|| {
                format!("could not create config from file `{}`", file.display())
            })?;

            prepare_config(&config, container).await
        }
    }
}

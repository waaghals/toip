use anyhow::{Context, Result};

use crate::config;
use crate::image::manager::ImageManager;

pub async fn prepare(container: Option<String>) -> Result<()> {
    let config = config::from_current_dir()?;

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
            for (name, container) in config.containers {
                image_manager
                    .prepare(&container.image)
                    .await
                    .with_context(|| format!("could not prepare container `{}`", name))?;
            }
        }
    };

    Ok(())
}

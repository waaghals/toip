use anyhow::{Context, Result};

use crate::config;
use crate::image::manager::ImageManager;

pub async fn prepare(_container: Option<String>) -> Result<()> {
    let config = config::from_current_dir()?;
    let image_manager = ImageManager::new().context("could not construct `ImageManager`")?;
    for container in config.containers() {
        image_manager.prepare(&container.image).await?;
    }

    Ok(())
}

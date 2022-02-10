use anyhow::{Context, Result};

use self::build::BuildManager;
use self::path::PathManager;
use self::registry::RegistryManager;
use crate::config::ImageSource;
use crate::oci::image::Image;

mod build;
mod path;
mod registry;

pub struct ImageManager {
    registry: RegistryManager,
    path: PathManager,
    build: BuildManager,
}

impl ImageManager {
    pub fn new() -> Result<Self> {
        Ok(ImageManager {
            registry: RegistryManager::new().context("could not create registry manager")?,
            path: PathManager::default(),
            build: BuildManager::default(),
        })
    }

    pub async fn prepare(&self, source: &ImageSource) -> Result<Image> {
        log::info!("preparing image `{}`", source);
        match source {
            ImageSource::Registry(source) => {
                let image = self.registry.pull(source).await?;
                Ok(image)
            }
            ImageSource::Build(source) => {
                let image = self.build.build(source).await?;
                Ok(image)
            }
        }
    }
}

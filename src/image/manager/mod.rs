use anyhow::{Context, Result};

use self::registry::RegistryManager;
use crate::config::ImageSource;
use crate::oci::image::Image;

mod registry;

pub struct ImageManager {
    registry: RegistryManager,
}

impl ImageManager {
    pub fn new() -> Result<Self> {
        Ok(ImageManager {
            registry: RegistryManager::new().context("could not create registry manager")?,
        })
    }

    pub async fn prepare(&self, source: &ImageSource) -> Result<Image> {
        log::info!("preparing image `{}`", source);
        match source {
            ImageSource::Registry(source) => {
                let image = self.registry.pull(source).await?;
                Ok(image)
            }
            ImageSource::Build(_source) => {
                todo!()
            }
        }
    }
}

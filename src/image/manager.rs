use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::config::{ImageReference, RegistryImage};
use crate::oci::distribution::{build_registry, Registry};
use crate::oci::image::{Descriptor, Image};
use crate::dirs::project_directories;

#[async_trait]
pub trait ImageManager {
    async fn prepare(&self, reference: &ImageReference) -> Result<()>;
}

pub struct Manager {
    // client: Box<dyn Registry>,
}

impl Manager {
    async fn pull(&self, image: &RegistryImage) -> Result<Image> {
        let client = build_registry(&image.registry);
        let manifest = client.manifest(&image.repository, &image.reference).await?;
        let config = client.image(&image.repository, &manifest.config).await?;
        
        Ok(config)
    }
}

#[async_trait]
impl ImageManager for Manager {
    async fn prepare(&self, reference: &ImageReference) -> Result<()> {
        match reference {
            ImageReference::Registry(registry) => {
                self.pull(registry).await?;
                Ok(())
            },
            _ => todo!(),
        }
    }
}

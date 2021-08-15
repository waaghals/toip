use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;

use crate::config::{ImageReference, RegistryImage};
use crate::metadata::BLOBS_DIR;
use crate::oci::distribution::{build_registry, Registry};
use crate::oci::image::Descriptor;

#[async_trait]
pub trait ImageManager {
    async fn prepare(&self, reference: &ImageReference) -> Result<()>;
}

pub struct Manager {
    // client: Box<dyn Registry>,
}

impl Manager {
    fn write_blob(&self, descriptor: &Descriptor, data: Vec<u8>) -> Result<()> {
        let mut location = PathBuf::from(BLOBS_DIR);
        location.push(descriptor.digest.algorithm.to_string());
        location.push(descriptor.digest.encoded.to_string());

        fs::write(location, data)?;
        Ok(())
    }

    async fn pull(&self, image: &RegistryImage) -> Result<()> {
        let client = build_registry(&image.registry);
        let manifest = client.manifest(&image.repository, &image.reference).await?;

        // TODO skip already downloaded files
        for descriptor in manifest.layers {
            let layer = client.layer(image.repository.as_str(), &descriptor).await?;
            self.write_blob(&descriptor, layer)?;
        }

        let config = client.image(&image.repository, &manifest.config).await?;
        // self.write_blob(&manifest.config, config)?;

        Ok(())
    }
}

#[async_trait]
impl ImageManager for Manager {
    async fn prepare(&self, reference: &ImageReference) -> Result<()> {
        match reference {
            ImageReference::Registry(registry) => self.pull(registry).await,
            _ => todo!(),
        }
    }
}

use std::io::Cursor;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tar::Archive;

use crate::config::RegistrySource;
use crate::dirs::layer_dir;
use crate::oci::distribution::{OciRegistry, Registry};
use crate::oci::image::{Digest, Image};

pub struct RegistryManager {
    layer_dir: PathBuf,
    client: OciRegistry,
}

impl Default for RegistryManager {
    fn default() -> Self {
        let client = OciRegistry::default();
        let layer_dir = layer_dir();
        RegistryManager::new(client, layer_dir)
    }
}

impl RegistryManager {
    pub fn new(client: OciRegistry, layer_dir: PathBuf) -> Self {
        RegistryManager { client, layer_dir }
    }

    fn destination(&self, digest: &Digest) -> PathBuf {
        let mut path = self.layer_dir.clone();
        path.push(&digest.algorithm.to_string());
        path.push(&digest.encoded.to_string());
        path
    }

    fn verify_layers(&self, _image: &Image) -> Result<()> {
        // TODO
        Ok(())
    }

    pub async fn pull(&self, source: &RegistrySource) -> Result<Image> {
        let manifest = self
            .client
            .manifest(&source.registry, &source.repository, &source.reference)
            .await?;
        let image = self
            .client
            .image(&source.registry, &source.repository, &manifest.config)
            .await?;

        let mut diff_ids = image.rootfs.diff_ids.iter();

        for layer_descriptor in manifest.layers.iter() {
            // TODO check if layer already extracted
            let diff_id = diff_ids.next().with_context(|| {
                format!(
                    "manifest `{}` references more layer then the image configuration contains",
                    manifest.config.digest
                )
            })?;
            let blob = self
                .client
                .layer(&source.registry, &source.repository, layer_descriptor)
                .await?;
            let buffer = Cursor::new(blob);
            let mut tar = Archive::new(buffer);
            let destination = self.destination(diff_id);

            tar.unpack(&destination).with_context(|| {
                format!(
                    "could not extract layer `{}` to `{}`",
                    layer_descriptor,
                    destination.display()
                )
            })?;
        }

        self.verify_layers(&image)?;

        Ok(image)
    }
}

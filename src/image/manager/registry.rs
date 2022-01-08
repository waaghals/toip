use std::io::Cursor;
use std::path::PathBuf;

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use tar::Archive;

use crate::config::RegistrySource;
use crate::dirs::layers_dir;
use crate::oci::distribution::{OciRegistry, Registry};
use crate::oci::image::{Digest, Image};

pub struct RegistryManager {
    layers_dir: PathBuf,
    client: OciRegistry,
}

impl RegistryManager {
    pub fn new() -> Result<Self> {
        let client = OciRegistry::new().context("could not construct `OciRegistry`")?;
        let layers_dir = layers_dir()?;
        Ok(RegistryManager { client, layers_dir })
    }

    fn destination(&self, digest: &Digest) -> PathBuf {
        let mut path = self.layers_dir.clone();
        path.push(&digest.algorithm.to_string());
        path.push(&digest.encoded.to_string());
        path
    }

    fn verify_layers(&self, _image: &Image) -> Result<()> {
        // TODO
        Ok(())
    }

    pub async fn pull(&self, source: &RegistrySource) -> Result<Image> {
        log::debug!("Downloading manifest");
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

            // TODO find out why tars need to be decoded here, while they cannot seem to be decoded during downloading
            let buffer = Cursor::new(blob);
            let decompressed = GzDecoder::new(buffer);
            let mut tar = Archive::new(decompressed);
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

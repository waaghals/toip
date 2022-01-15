use std::io::Cursor;
use std::path::PathBuf;

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use tar::Archive;

use crate::config::RegistrySource;
use crate::dirs;
use crate::oci::distribution::{OciRegistry, Registry};
use crate::oci::image::{Digest, Image};

pub struct RegistryManager {
    client: OciRegistry,
}

fn destination(digest: &Digest) -> Result<PathBuf> {
    dirs::layer_dir(&digest.algorithm.to_string(), &digest.encoded)
}

impl RegistryManager {
    pub fn new() -> Result<Self> {
        let client = OciRegistry::new().context("could not construct `OciRegistry`")?;
        Ok(RegistryManager { client })
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
            let destination = destination(diff_id).with_context(|| {
                format!(
                    "could not determin layer destination for diff `{}`",
                    diff_id
                )
            })?;
            if destination.exists() {
                log::debug!(
                    "skipping extraction of diff `{}` as layer already exists",
                    diff_id
                );
                continue;
            }

            log::debug!("downloading blob for diff `{}`", diff_id);
            let blob = self
                .client
                .layer(&source.registry, &source.repository, layer_descriptor)
                .await?;

            // TODO find out why tars need to be decoded here, while they cannot seem to be decoded during downloading
            let buffer = Cursor::new(blob);
            let decompressed = GzDecoder::new(buffer);
            let mut tar = Archive::new(decompressed);

            log::debug!(
                "extracting diff `{}` to layer `{}`",
                diff_id,
                destination.display()
            );
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

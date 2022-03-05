use anyhow::{Context, Result};

use self::registry::RegistryManager;
use crate::oci::image::Image;

mod registry;

#[deprecated]
pub struct ImageManager {
    registry: RegistryManager,
}

impl ImageManager {
    pub fn new() -> Result<Self> {
        Ok(ImageManager {
            registry: RegistryManager::new().context("could not create registry manager")?,
        })
    }
}

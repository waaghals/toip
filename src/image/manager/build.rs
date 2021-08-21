use anyhow::Result;

use crate::{config::BuildSource, oci::image::Image};

pub struct BuildManager {}

impl Default for BuildManager {
    fn default() -> Self {
        BuildManager {}
    }
}

impl BuildManager {
    pub async fn build(&self, _source: &BuildSource) -> Result<Image> {
        todo!()
    }
}

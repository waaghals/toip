use anyhow::Result;

use crate::config::BuildSource;
use crate::oci::image::Image;

#[derive(Default)]
pub struct BuildManager {}

impl BuildManager {
    pub async fn build(&self, _source: &BuildSource) -> Result<Image> {
        todo!()
    }
}

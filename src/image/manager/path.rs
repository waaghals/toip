use anyhow::Result;

use crate::config::PathSource;
use crate::oci::image::Image;

pub struct PathManager {

}

impl Default for PathManager {
    fn default() -> Self {
        PathManager {}
    }
}

impl PathManager {
    pub async fn convert(&self, source: &PathSource) -> Result<Image> {
        todo!()
    }
}
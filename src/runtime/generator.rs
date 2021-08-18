use anyhow::Result;
use crate::oci::runtime::Config;
use crate::config::Container;
use async_trait::async_trait;

// TODO create overlay fs https://www.itopstimes.com/contain/demystifying-a-docker-image/
#[async_trait]
pub trait RuntimeBundleGenerator {
    async fn build(&self, config: &Container) -> Result<Config>;
}

pub struct RunGenerator {

}

#[async_trait]
impl RuntimeBundleGenerator for RunGenerator {
    async fn build(&self, config: &Container) -> Result<Config> {
        // 1. Retrieve image (pull)

        // 4. Convert Image config to Runtime Bundle config
        // 5. Modify Runtime Bundle config
        // 6. Mount rootfs using overlay fs
        // 7. Generate scripts for container's links
        // 8. Mount scripts and current binary
        // 9. Overwrite with any user defined configuration
        todo!()
    }
}

pub struct CallGenerator {

}

#[async_trait]
impl RuntimeBundleGenerator for CallGenerator {
    async fn build(&self, config: &Container) -> Result<Config> {
        todo!()
    }
}
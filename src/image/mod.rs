use anyhow::{Result, Context};
use async_trait::async_trait;
use regex::Regex;

pub mod docker;
pub mod oci;

const PATTERN: &str = r"^(?:(?P<registry>[a-zA-Z0-9][a-zA-Z0-9.]+?)/)?(?P<name>[a-z0-9][a-z0-9._-]*(?:/[a-z0-9][a-z0-9._-]*)?)(?:[:@](?P<reference>[a-zA-Z0-9_][a-zA-Z0-9._-]{0,127}))?$";

#[async_trait]
pub trait ContainerRegistry {
    async fn download(&self, name: &str, reference: &str) -> Result<Image>;
}

#[derive(Debug)]
struct ImageRef {
    original: String,
    registry: String,
    name: String,
    reference: String,
}

impl ImageRef {
    fn parse(image_ref: &str) -> Result<ImageRef> {
        let regex = Regex::new(PATTERN).unwrap();
        let captures = regex
            .captures(image_ref)
            .with_context(|| format!("Image reference `{}` could not be parsed.", image_ref))?;
        let registry = match captures.name("registry") {
            Some(registry_match) => registry_match.as_str(),
            None => "index.docker.io",
        };
        let reference = match captures.name("reference") {
            Some(reference_match) => reference_match.as_str(),
            None => "latest",
        };
        let name = captures.name("name").unwrap().as_str();

        Ok(ImageRef {
            original: image_ref.to_string(),
            registry: registry.to_string(),
            name: name.to_string(),
            reference: reference.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Layer {
    // digest: OciDigest,
    // pub size: usize,
    pub reference: String,
    pub bytes: Vec<u8>,
}

// impl VerifyDigest for Layer {
//     fn algorithm(&self) -> &crate::verify::Algorithm {
//         &self.digest.algorithm
//     }

//     fn digest(&self) -> &str {
//         &self.digest.encoded
//     }

//     fn data(&self) -> &[u8] {
//         &self.bytes
//     }
// }

#[derive(Debug, Clone)]
pub struct Image {
    pub reference: String,
    // pub registry: String,
    // pub name: String,
    // pub size: u64,
    pub layers: Vec<Layer>,
    // digest: OciDigest,
}

// impl VerifyDigest for Image {
//     fn algorithm(&self) -> &crate::verify::Algorithm {
//         &self.digest.algorithm
//     }

//     fn digest(&self) -> &str {
//         &self.digest.encoded
//     }

//     fn data(&self) -> &bytes::Bytes {
//         todo!()
//     }
// }

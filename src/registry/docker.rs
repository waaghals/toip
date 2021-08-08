use crate::metadata::{HOMEPAGE, NAME, VERSION};
use crate::oci::image::{Manifest as OciManifest, ManifestList as OciManifestList};
use crate::verify::Algorithm;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use regex::Regex;
use reqwest::header::ACCEPT;
use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use serde::de;
use serde::de::{Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};
use serde_derive::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::env::consts;
use std::error;
use std::fmt;

const DIGEST_PATTERN: &str = "^(?P<algorithm>[A-Fa-f0-9_+.-]+):(?P<hex>[A-Fa-f0-9]+)$";
use super::{ContainerRegistry, Image};

#[derive(Debug)]
struct Digest {
    algorithm: Algorithm,
    hex: String,
}

impl TryFrom<&str> for Digest {
    type Error = ParseDigestError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let regex = Regex::new(DIGEST_PATTERN).unwrap();
        let captures = regex
            .captures(&value)
            .ok_or(ParseDigestError)
            .with_context(|| format!("Digest `{}` could not be parsed.", &value))?;

        let algorithm = captures.name("algorithm").unwrap().as_str();
        let hex = captures.name("hex").unwrap().as_str();

        let algorithm = match algorithm {
            "sha256" => Ok(Algorithm::SHA256),
            "sha512" => Ok(Algorithm::SHA512),
            _ => Err(ParseDigestError),
        }
        .with_context(|| {
            format!(
                "Unsupported algorithm `{}` in digest `{}`.",
                &algorithm, &value
            )
        })?;

        Ok(Digest {
            algorithm,
            hex: hex.to_string(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParseDigestError;

impl fmt::Display for ParseDigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid digest format")
    }
}

impl error::Error for ParseDigestError {}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", &self.algorithm, &self.hex)
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string = String::deserialize(deserializer)?;
        Digest::try_from(string.as_str()).map_err(de::Error::custom)
    }
}

impl Serialize for Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let val = format!("{}:{}", &self.hex, &self.algorithm);
        serializer.serialize_str(val.as_str())
    }
}

#[derive(Debug)]
pub struct Registry {
    client: Client,
}

enum Manifest {
    OciImage(OciManifest),
    OciList(OciManifestList),
    Legacy(LegacyManifest),
}

#[derive(Serialize, Deserialize, Debug)]
struct LegacyManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,

    #[serde(rename = "fsLayers")]
    pub fs_layers: Vec<Layer>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Layer {
    #[serde(rename = "blobSum")]
    pub blob_sum: Digest,
}

#[async_trait]
impl ContainerRegistry for Registry {
    async fn download(&self, host: &str, name: &str, reference: &str) -> Result<super::Image> {
        let manifest = download_manifest(&self.client, &host, &name, &reference).await?;
        let layers = download_blobs(&self.client, host, name, &manifest).await?;
        // let layers = download_blobs(&self.client, &host, &name, &manifest).await?;

        let image = Image {
            registry: host,
            name,
            size: manifest.size,
            layers,
            digest: manifest.digest,
        };
        image.verify()?;
        Ok(image)
    }
}

impl Registry {
    pub fn new() -> Self {
        Registry::default()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Registry {
            client: Client::builder().user_agent(format!("{}/{} ({})", NAME, VERSION, HOMEPAGE)),
        }
    }
}

async fn download_manifest(
    client: &Client,
    host: &str,
    name: &str,
    reference: &str,
) -> Result<Manifest> {
    let manifest_uri = format!("https://{}/v2/{}/manifests/{}", host, name, reference);

    // TODO check if manifest is old manifest type
    // TODO check if manifest is an index

    let request = client
        .request("GET", manifest_uri)
        // TODO support image lists
        // .header(ACCEPT, "application/vnd.oci.image.index.v1+json")
        .header(ACCEPT, "application/vnd.oci.image.manifest.v1+json")
        .build();

    let response = client.execute(request).await?;
    let content_type = response.headers().get(CONTENT_TYPE);
    match content_type {
        Some("application/vnd.docker.distribution.manifest.v1+json") => {
            let manifest = response.json::<LegacyManifest>().await?;
            Ok(Manifest::Legacy(manifest))
        }
        Some("application/vnd.docker.distribution.manifest.v2+json") => {
            let manifest = response.json::<OciManifest>().await?;
            Ok(Manifest::OciImage(manifest))
        }
        Some("application/vnd.docker.distribution.manifest.list.v2+json") => {
            let manifest = response.json::<OciManifestList>().await?;
            Ok(Manifest::OciList(manifest));
        }
        None => Err(anyhow!("Missing content type for `{}`", &request)),
        _ => Err(anyhow!(
            "Received unsupported content type `{}` for request `{}`",
            &content_type,
            &request
        )),
    }
}

fn platform_manifest(manifests: &Vec<OciManifest>) -> Option<&OciManifest> {
    manifests
        .iter()
        .find(|manifest| manifest.platform.supported())
}

async fn download_blobs(
    client: &Client,
    host: &str,
    name: &str,
    manifest: &Manifest,
) -> Result<Vec<super::Layer>> {
    let mut layers = Vec::new();

    let digests = match manifest {
        Manifest::OciList(list) => match platform_manifest(list.manifests) {
            Some(manifest) => Ok(manifest
                .layers
                .iter()
                .map(|layer| (layer.digest, Some(layer.size)))),
            None => Err(anyhow!(
                "Image `{}` does not support `{}/{}`.",
                &name,
                consts::OS,
                consts::ARCH
            )),
        },
        Manifest::OciImage(manifest) => Ok(manifest
            .layers
            .iter()
            .map(|layer| (layer.digest, Some(layer.size)))),
        Manifest::Legacy(manifest) => Ok(manifest
            .fs_layers
            .iter()
            .map(|layer| (layer.blob_sum, None))),
    }?;
    for (digest, size) in digests.iter() {
        let layer = download_blob(&client, &host, &name, &digest, &size).await?;
        layers.push(layer);
    }

    Ok(layers)
}

async fn download_blob(
    client: &Client,
    host: &str,
    name: &str,
    digest: &Digest,
    expected_size: &Option<u64>,
) -> Result<super::Layer> {
    let blob_uri = format!("https://{}/v2/{}/blobs/{}", &host, &name, &digest);

    let response = reqwest::get(&blob_uri).await?;
    let blob = response.bytes().await?;
    let size = blob.len();

    if let Some(expected) = expected_size {
        if size != expected {
            return Err(anyhow!(
                "Received `{}` bytes, expected `{}`.",
                &size,
                &expected,
            ));
        }
    }

    Ok(super::Layer {
        /*
           "To ensure security, the content should be verified against the digest used to fetch the content.
           At times, the returned digest may differ from that used to initiate a request.
           Such digests are considered to be from different domains, meaning they have different values for algorithm.
           In such a case, the client may choose to verify the digests in both domains or ignore the server’s digest.
           To maintain security, the client must always verify the content against the digest used to fetch the content."
           - https://docs.docker.com/registry/spec/api/#content-digests
        */
        digest,
        size,
        bytes: blob.to_vec(),
    })
}

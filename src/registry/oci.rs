use crate::metadata::{HOMEPAGE, NAME, VERSION};
use crate::oci::image::{Discriptor, Manifest};
use anyhow::Result;
use async_trait::async_trait;
use reqwest::{header::ACCEPT, Client};

use super::{ContainerRegistry, Image, Layer};

pub struct Registry {
    client: Client,
}

#[async_trait]
impl ContainerRegistry for Registry {
    async fn download(&self, host: &str, name: &str, reference: &str) -> Result<Image> {
        let manifest = download_manifest(&host, &name, &reference).await?;
        let layers = download_blobs(&host, &name, &manifest).await?;

        let image = Image {
            registry: reference.registry,
            name: reference.name,
            size: manifest.config.size,
            layers,
            digest: manifest.config.digest,
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

    let request = client
        .request("GET", manifest_uri)
        // TODO support image lists
        // .header(ACCEPT, "application/vnd.oci.image.index.v1+json")
        .header(ACCEPT, "application/vnd.oci.image.manifest.v1+json")
        .build();

    let manifest = client.execute(request).await?.json::<Manifest>().await?;

    Ok(manifest)
}

async fn download_blobs(host: &str, name: &str, manifest: &Manifest) -> Result<Vec<Layer>> {
    let mut layers = Vec::new();
    for layer_info in manifest.layers.iter() {
        let layer = download_blob(&host, &name, &layer_info).await?;
        layers.push(layer);
    }

    Ok(layers)
}

async fn download_blob(
    client: &Client,
    host: &str,
    name: &str,
    layer_info: &Discriptor,
) -> Result<Layer> {
    let blob_uri = format!(
        "https://{}/v2/{}/blobs/{}",
        &host, &name, &layer_info.digest
    );

    let request = client.request("GET", blob_uri).build();
    let blob = client.execute(&request).await?.bytes().await?;

    Ok(Layer {
        digest: layer_info.digest.clone(),
        size: layer_info.size,
        bytes: blob.to_vec(),
    })
}

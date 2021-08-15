use std::io::Read;

use crate::metadata::{HOMEPAGE, NAME, VERSION};

use super::image::{Algorithm, Descriptor, Image, Manifest};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use flate2::read::GzDecoder;
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT},
    Client, IntoUrl, Method,
};
use sha2::{Digest, Sha256, Sha512};

#[async_trait]
trait Downloader {
    async fn download<U>(
        &self,
        url: U,
        accept_headers: Vec<&String>,
        descriptor: Option<&Descriptor>,
    ) -> Result<Vec<u8>>
    where
        U: IntoUrl + Send;
}

#[async_trait]
pub trait Registry {
    async fn manifest(&self, name: &str, reference: &str) -> Result<Manifest>;
    async fn image(&self, name: &str, descriptor: &Descriptor) -> Result<Image>;
    async fn layer(&self, name: &str, descriptor: &Descriptor) -> Result<Vec<u8>>;
}

#[derive(Debug)]
struct OciRegistry<D> {
    host: String,
    downloader: D,
}

// TODO use builder pattern
pub fn build_registry<H>(host: H) -> impl Registry
where
    H: Into<String>,
{
    OciRegistry {
        host: host.into(),
        downloader: DecompressDownloader {
            inner: ValidatingDownloader {
                inner: ReqwestDownloader::default(),
            },
        },
    }
}

impl<D> OciRegistry<D>
where
    D: Downloader,
{
    fn blob_url(&self, name: &str, descriptor: &Descriptor) -> String {
        format!(
            "https://{}/v2/{}/blobs/{}",
            self.host, name, descriptor.digest
        )
    }
}

#[async_trait]
impl<D> Registry for OciRegistry<D>
where
    D: Downloader + Sync + Send,
{
    async fn manifest(&self, name: &str, reference: &str) -> Result<Manifest> {
        let url = format!("https://{}/v2/{}/manifests/{}", self.host, name, reference);

        let bytes = self
            .downloader
            .download(
                url,
                vec![
                    &"application/vnd.oci.image.manifest.v1+json".to_string(),
                    &"application/vnd.docker.distribution.manifest.v2+json".to_string(),
                ],
                None,
            )
            .await?;
        let manifest = serde_json::from_slice(&bytes)?;
        Ok(manifest)
    }

    async fn image(&self, name: &str, descriptor: &Descriptor) -> Result<Image> {
        let uri = self.blob_url(name, descriptor);
        let bytes = self
            .downloader
            .download(uri, vec![&descriptor.media_type], Some(descriptor))
            .await?;

        let image = serde_json::from_slice(&bytes)?;
        Ok(image)
    }

    async fn layer(&self, name: &str, descriptor: &Descriptor) -> Result<Vec<u8>> {
        let uri = self.blob_url(name, descriptor);
        let bytes = self
            .downloader
            .download(uri, vec![], Some(descriptor))
            .await?;
        Ok(bytes)
    }
}

#[derive(Debug)]
struct ReqwestDownloader {
    client: Client,
}

impl ReqwestDownloader {
    fn new(user_agent: &str) -> Self {
        ReqwestDownloader {
            client: Client::builder().user_agent(user_agent).build().unwrap(),
        }
    }
}

impl Default for ReqwestDownloader {
    fn default() -> Self {
        ReqwestDownloader::new(format!("{}/{} ({})", NAME, VERSION, HOMEPAGE).as_str())
    }
}

#[async_trait]
impl Downloader for ReqwestDownloader {
    async fn download<U>(
        &self,
        url: U,
        accept_headers: Vec<&String>,
        _: Option<&Descriptor>,
    ) -> Result<Vec<u8>>
    where
        U: IntoUrl + Send,
    {
        let mut headers = HeaderMap::new();
        for header in accept_headers {
            let value = HeaderValue::from_str(header)?;
            headers.append(ACCEPT, value);
        }

        let request = self
            .client
            .request(Method::GET, url)
            .headers(headers)
            .build()?;

        let response = self.client.execute(request).await?;
        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }
}

#[derive(Debug)]
struct ValidatingDownloader<T> {
    inner: T,
}

#[async_trait]
impl<T> Downloader for ValidatingDownloader<T>
where
    T: Downloader + Sync + Send,
{
    async fn download<U>(
        &self,
        url: U,
        accept_headers: Vec<&String>,
        descriptor: Option<&Descriptor>,
    ) -> Result<Vec<u8>>
    where
        U: IntoUrl + Send,
    {
        let bytes = self.inner.download(url, accept_headers, descriptor).await?;
        if let Some(descriptor) = descriptor {
            // Copy bytes without consuming the response
            // let mut bytes = Vec::new();
            // while let Some(chunk) = response.chunk().await? {
            //     for byte in chunk.to_vec() {
            //         bytes.push(byte);
            //     }
            // }

            // let bytes = response.bytes().await?;
            let actual_size = bytes.len() as u64;
            if descriptor.size != actual_size {
                return Err(anyhow!(
                    "Expected size `{}` is not equal to the calculated size `{}`.",
                    descriptor.size,
                    actual_size
                ));
            }

            let calculated_digest = match descriptor.digest.algorithm {
                Algorithm::SHA256 => {
                    format!("{:x}", Sha256::digest(&bytes))
                }
                Algorithm::SHA512 => {
                    format!("{:x}", Sha512::digest(&bytes))
                }
            };

            let expected_digest = &descriptor.digest.encoded;
            if *expected_digest != calculated_digest {
                return Err(anyhow!(
                    "Expected digest `{}` is not equal to the calculated digest `{}`.",
                    expected_digest,
                    calculated_digest
                ));
            }
        }
        Ok(bytes)
    }
}

#[derive(Debug)]
struct DecompressDownloader<T> {
    inner: T,
}

#[async_trait]
impl<T> Downloader for DecompressDownloader<T>
where
    T: Downloader + Sync + Send,
{
    async fn download<U>(
        &self,
        url: U,
        accept_headers: Vec<&String>,
        descriptor: Option<&Descriptor>,
    ) -> Result<Vec<u8>>
    where
        U: IntoUrl + Send,
    {
        let data = self.inner.download(url, accept_headers, descriptor).await?;

        match descriptor {
            Some(descriptor) => {
                if !descriptor.media_type.ends_with("+gzip") {
                    return Ok(data);
                }

                let decoder = GzDecoder::new(&data[..]);
                let decompressed = decoder.bytes().map(|byte| byte.unwrap()).collect();
                return Ok(decompressed);
            }
            None => {
                return Ok(data);
            }
        }
    }
}

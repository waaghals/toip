use std::io::Read;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use flate2::read::GzDecoder;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use reqwest::{Client, IntoUrl, Method};
use sha2::{Digest as ShaDigest, Sha256, Sha512};

use super::image::{Algorithm, Descriptor, Digest, Image, Manifest, Reference};
use crate::metadata::{HOMEPAGE, NAME, VERSION};

enum Context {
    Descriptor(Descriptor),
    Digest(Digest),
    None,
}

#[async_trait]
trait Downloader {
    async fn download<U>(
        &self,
        url: U,
        accept_headers: Vec<&String>,
        context: &Context,
    ) -> Result<Vec<u8>>
    where
        U: IntoUrl + Send;
}

#[async_trait]
pub trait Registry {
    async fn manifest(&self, name: &str, reference: &Reference) -> Result<Manifest>;
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
            inner: VerifyingDownloader {
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
    async fn manifest(&self, name: &str, reference: &Reference) -> Result<Manifest> {
        let url = format!("https://{}/v2/{}/manifests/{}", self.host, name, reference);
        let context = match reference {
            Reference::Digest(digest) => Context::Digest(digest.clone()),
            Reference::Tag(_) => Context::None,
        };

        let bytes = self
            .downloader
            .download(
                &url,
                vec![
                    &"application/vnd.oci.image.manifest.v1+json".to_string(),
                    &"application/vnd.docker.distribution.manifest.v2+json".to_string(),
                ],
                &context,
            )
            .await?;
        println!("{}", &url);
        // println!("{}", from_utf8(&bytes).unwrap());
        // TODO handle fat manifest
        let manifest = serde_json::from_slice(&bytes)?;
        Ok(manifest)
    }

    async fn image(&self, name: &str, descriptor: &Descriptor) -> Result<Image> {
        let uri = self.blob_url(name, descriptor);
        let bytes = self
            .downloader
            .download(uri, vec![&descriptor.media_type], &Context::Descriptor(descriptor.clone()))
            .await?;

        let image = serde_json::from_slice(&bytes)?;
        Ok(image)
    }

    async fn layer(&self, name: &str, descriptor: &Descriptor) -> Result<Vec<u8>> {
        let uri = self.blob_url(name, descriptor);
        let bytes = self
            .downloader
            .download(uri, vec![], &Context::Descriptor(descriptor.clone()))
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
        _: &Context,
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
struct VerifyingDownloader<T> {
    inner: T,
}

fn verify_size(bytes: &[u8], expected_size: u64) -> Result<()> {
    let actual_size = bytes.len() as u64;
    if expected_size != actual_size {
        return Err(anyhow!(
            "Expected size `{}` is not equal to the calculated size `{}`.",
            expected_size,
            actual_size
        ));
    }
    Ok(())
}

fn verify_digest(bytes: &[u8], expected_digest: &Digest) -> Result<()> {
    let calculated_digest = match expected_digest.algorithm {
        Algorithm::SHA256 => {
            format!("{:x}", Sha256::digest(&bytes))
        }
        Algorithm::SHA512 => {
            format!("{:x}", Sha512::digest(&bytes))
        }
    };

    let expected_digest = &expected_digest.encoded;
    if *expected_digest != calculated_digest {
        return Err(anyhow!(
            "Expected digest `{}` is not equal to the calculated digest `{}`.",
            expected_digest,
            calculated_digest
        ));
    }

    Ok(())
}

#[async_trait]
impl<T> Downloader for VerifyingDownloader<T>
where
    T: Downloader + Sync + Send,
{
    async fn download<U>(
        &self,
        url: U,
        accept_headers: Vec<&String>,
        context: &Context,
    ) -> Result<Vec<u8>>
    where
        U: IntoUrl + Send,
    {
        let bytes = self.inner.download(url, accept_headers, context).await?;

        if let Context::Descriptor(descriptor) = context {
            verify_size(&bytes, descriptor.size)?;
        }

        match context {
            Context::Descriptor(descriptor) => verify_digest(&bytes, &descriptor.digest)?,
            Context::Digest(digest) => verify_digest(&bytes, digest)?,
            Context::None => {},
        };

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
        context: &Context,
    ) -> Result<Vec<u8>>
    where
        U: IntoUrl + Send,
    {
        let data = self.inner.download(url, accept_headers, context).await?;

        match context {
            Context::Descriptor(descriptor) => {
                if !descriptor.media_type.ends_with("+gzip") {
                    return Ok(data);
                }

                let decoder = GzDecoder::new(&data[..]);
                let decompressed = decoder.bytes().map(|byte| byte.unwrap()).collect();
                return Ok(decompressed);
            }
            Context::Digest(_) => Ok(data),
            Context::None => Ok(data),
        }
    }
}

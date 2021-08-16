use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::fmt;

use anyhow::{anyhow, Context as AnyhowContext, Result};
use async_trait::async_trait;
use flate2::read::GzDecoder;
use log;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE};
use reqwest::{Client, IntoUrl, Method};
use sha2::{Digest as ShaDigest, Sha256, Sha512};
use std::env::consts::{ARCH, OS};
use thiserror::Error;

use super::image::{Algorithm, Descriptor, Digest, Image, Manifest, ManifestItem, Reference};
use crate::dirs::project_directories;
use crate::metadata::{HOMEPAGE, NAME, VERSION};
use crate::oci::image::ManifestList;

#[derive(Debug)]
enum Context<'a> {
    Descriptor(&'a Descriptor),
    Digest(&'a Digest),
    ManifestItem(&'a ManifestItem),
    None,
}

impl fmt::Display for Context<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

struct Response {
    bytes: Vec<u8>,
    content_type: String,
}

#[async_trait]
trait Downloader {
    async fn download<U>(
        &self,
        url: U,
        accept_headers: Vec<&String>,
        context: &Context,
    ) -> Result<Response>
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
    log::trace!("constructing Registry client");
    let cache_dir = project_directories().cache_dir().to_path_buf();
    OciRegistry {
        host: host.into(),
        downloader: DecompressDownloader {
            inner: VerifyingDownloader {
                context: "cache",
                inner: CachingDownloader {
                    cache_dir,
                    inner: VerifyingDownloader {
                        context: "download",
                        inner: ReqwestDownloader::default(),
                    },
                },
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
        log::debug!(
            "fetching manifest for `{}` with reference `{}`",
            name,
            reference
        );
        let url = format!("https://{}/v2/{}/manifests/{}", self.host, name, reference);
        let context = match reference {
            Reference::Digest(digest) => Context::Digest(digest),
            Reference::Tag(_) => Context::None,
        };

        let response = self
            .downloader
            .download(
                &url,
                vec![
                    &"application/vnd.oci.image.manifest.v1+json".to_string(),
                    &"application/vnd.oci.image.index.v1+json".to_string(),
                    &"application/vnd.docker.distribution.manifest.v2+json".to_string(),
                    &"application/vnd.docker.distribution.manifest.list.v2+json".to_string(),
                ],
                &context,
            )
            .await?;

        let content_type = response.content_type.as_str();
        match content_type {
            "application/vnd.oci.image.manifest.v1+json"
            | "application/vnd.docker.distribution.manifest.v2+json" => {
                log::debug!("deserializing manifest `{}`", name);
                let manifest: Manifest = serde_json::from_slice(&response.bytes)?;
                Ok(manifest)
            }
            "application/vnd.oci.image.index.v1+json"
            | "application/vnd.docker.distribution.manifest.list.v2+json" => {
                log::debug!("deserializing manifest list `{}`", name);
                let manifest_list: ManifestList = serde_json::from_slice(&response.bytes)?;
                log::info!("resolving supported manifest for platform {}/{}", OS, ARCH);
                let item = manifest_list.supported().with_context(|| {
                    format!(
                        "no manifest found for image `{}` on current platform `{}/{}`",
                        name, OS, ARCH
                    )
                })?;
                log::info!("found matching manifest `{}` for `{}`", item.digest, name);

                let manifest_url = format!(
                    "https://{}/v2/{}/manifests/{}",
                    self.host, name, item.digest
                );
                let manifest_response = self
                    .downloader
                    .download(
                        &manifest_url,
                        vec![
                            &"application/vnd.oci.image.manifest.v1+json".to_string(),
                            &"application/vnd.docker.distribution.manifest.v2+json".to_string(),
                        ],
                        &Context::ManifestItem(item),
                    )
                    .await?;

                log::debug!("deserializing manifest `{}`", name);
                let manifest: Manifest = serde_json::from_slice(&manifest_response.bytes)?;
                Ok(manifest)
            }
            _ => Err(anyhow!(
                "received unsupported content type `{}` for request `{} {}`",
                &content_type,
                Method::GET,
                &url
            )),
        }
    }

    async fn image(&self, name: &str, descriptor: &Descriptor) -> Result<Image> {
        log::debug!(
            "fetching image config for `{}` with digest `{}`",
            name,
            descriptor.digest
        );
        let uri = self.blob_url(name, descriptor);
        let response = self
            .downloader
            .download(
                uri,
                vec![&descriptor.media_type],
                &Context::Descriptor(descriptor),
            )
            .await?;

        log::debug!("deserializing image config `{}`", name);
        let image = serde_json::from_slice(&response.bytes)?;
        Ok(image)
    }

    async fn layer(&self, name: &str, descriptor: &Descriptor) -> Result<Vec<u8>> {
        log::debug!(
            "fetching layer for `{}` with digest `{}`",
            name,
            descriptor.digest
        );
        let uri = self.blob_url(name, descriptor);
        let response = self
            .downloader
            .download(uri, vec![], &Context::Descriptor(descriptor))
            .await?;

        Ok(response.bytes)
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
    ) -> Result<Response>
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
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .context("response is missing Content-Type header")?
            .to_str()
            .context("could not convert Content-Type header to string")?
            .to_string()
            .clone();

        let status = response.status();
        if !status.is_success() {
            return Err(anyhow!("received unsucessful response status `{}`", status));
        }

        let bytes = response.bytes().await?;

        Ok(Response {
            bytes: bytes.to_vec(),
            content_type,
        })
    }
}

#[derive(Debug)]
struct VerifyingDownloader<T> {
    context: &'static str,
    inner: T,
}

impl<T> VerifyingDownloader<T> {
    fn verify_size(&self, bytes: &[u8], expected_size: u64) -> Result<(), VerifyError> {
        let actual_size = bytes.len() as u64;
        if expected_size != actual_size {
            return Err(VerifyError::InvalidSize {
                context: self.context,
                actual: actual_size,
                expected: actual_size,
            });
        }
        Ok(())
    }

    fn verify_digest(&self, bytes: &[u8], expected_digest: &Digest) -> Result<(), VerifyError> {
        let calculated_digest = match expected_digest.algorithm {
            Algorithm::SHA256 => {
                format!("{:x}", Sha256::digest(bytes))
            }
            Algorithm::SHA512 => {
                format!("{:x}", Sha512::digest(bytes))
            }
        };

        let expected_digest = &expected_digest.encoded;
        if *expected_digest != calculated_digest {
            return Err(VerifyError::InvalidDigest {
                context: self.context,
                expected: expected_digest.to_string(),
                actual: calculated_digest,
            });
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("expected size `{expected}` is not equal to the calculated size `{actual}` when verifying within context `{context}`")]
    InvalidSize {
        context: &'static str,
        expected: u64,
        actual: u64,
    },
    #[error("expected digest `{expected}` is not equal to the calculated digest `{actual}` when verifying within context `{context}`")]
    InvalidDigest {
        context: &'static str,
        expected: String,
        actual: String,
    },
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
    ) -> Result<Response>
    where
        U: IntoUrl + Send,
    {
        let response = self.inner.download(url, accept_headers, context).await?;

        match *context {
            Context::Descriptor(descriptor) => {
                log::info!("verifying size for descriptor `{}`", descriptor);
                self.verify_size(&response.bytes, descriptor.size)?
            }
            Context::ManifestItem(item) => {
                log::info!("verifying size for manifest item `{}`", item);
                self.verify_size(&response.bytes, item.size)?
            }
            Context::Digest(_) | Context::None => {
                log::debug!("skipping size verification");
            }
        }

        match *context {
            Context::Descriptor(descriptor) => {
                log::info!("verifying digest for descriptor `{}`", descriptor);
                self.verify_digest(&response.bytes, &descriptor.digest)?
            }
            Context::Digest(digest) => {
                log::info!("verifying digest for digest `{}`", digest);
                self.verify_digest(&response.bytes, digest)?
            }
            Context::ManifestItem(item) => {
                log::info!("verifying manifest item for digest `{}`", item);
                self.verify_digest(&response.bytes, &item.digest)?
            }
            Context::None => {
                log::debug!("skipping digest verification");
            }
        };

        Ok(response)
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
    ) -> Result<Response>
    where
        U: IntoUrl + Send,
    {
        let mut response = self.inner.download(url, accept_headers, context).await?;

        if response.content_type.ends_with("+gzip") {
            log::info!("decompressing `{}`", context);
            let decoder = GzDecoder::new(&response.bytes[..]);
            let decompressed = decoder.bytes().map(|byte| byte.unwrap()).collect();
            response.bytes = decompressed;
        }

        Ok(response)
    }
}

struct CachingDownloader<T> {
    cache_dir: PathBuf,
    inner: T,
}

impl<T> CachingDownloader<T> {
    fn paths(&self, digest: &Digest) -> (PathBuf, PathBuf, PathBuf) {
        let mut location = self.cache_dir.clone();
        location.push(digest.algorithm.to_string());
        location.push(digest.encoded.to_string());

        let mut data_path = location.clone();
        data_path.push("data".to_string());

        let mut type_path = location.clone();
        type_path.push("type".to_string());

        (location, type_path, data_path)
    }

    fn get(&self, digest: &Digest) -> Result<Option<Response>> {
        let (_location, type_path, data_path) = self.paths(digest);

        if !data_path.exists() || !type_path.exists() {
            return Ok(None);
        }

        let content_type = fs::read_to_string(&type_path)
            .with_context(|| format!("could not read cache type file {:?}", type_path))?;
        let bytes = fs::read(&data_path)
            .with_context(|| format!("could not read cache data file {:?}", data_path))?;

        Ok(Some(Response {
            content_type,
            bytes,
        }))
    }

    fn save(&self, digest: &Digest, response: &Response) -> Result<()> {
        let (location, type_path, data_path) = self.paths(digest);
        fs::create_dir_all(&location)
            .with_context(|| format!("could not create cache directory `{:?}`", location))?;
        fs::write(&type_path, &response.content_type)
            .with_context(|| format!("could not write cache type file {:?}", type_path))?;
        fs::write(&data_path, &response.bytes)
            .with_context(|| format!("could not write cache data file {:?}", data_path))?;
        Ok(())
    }
}

#[async_trait]
impl<T> Downloader for CachingDownloader<T>
where
    T: Downloader + Sync + Send,
{
    async fn download<U>(
        &self,
        url: U,
        accept_headers: Vec<&String>,
        context: &Context,
    ) -> Result<Response>
    where
        U: IntoUrl + Send,
    {
        let digest = match *context {
            Context::Descriptor(descriptor) => Some(&descriptor.digest),
            Context::Digest(digest) => Some(digest),
            Context::ManifestItem(manifest) => Some(&manifest.digest),
            Context::None => None,
        };

        if let Some(digest) = digest {
            log::debug!("checking for existing cache for `{}`", digest);
            let cache_response = self
                .get(digest)
                .with_context(|| format!("could not read cache for digest `{}`", digest))?;
            if let Some(response) = cache_response {
                log::info!("found cached data for `{}`", digest);
                return Ok(response);
            }
            log::info!("no cached data for `{}`", digest);
        }

        let response = self.inner.download(url, accept_headers, context).await?;
        if let Some(digest) = digest {
            log::info!("caching response for `{}`", digest);
            self.save(digest, &response)
                .with_context(|| format!("could not save cache for digest `{}`", digest))?;
        }

        Ok(response)
    }
}

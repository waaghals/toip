use std::env::consts::{ARCH, OS};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::{fmt, fs};

use anyhow::{anyhow, Context as AnyhowContext, Result};
use async_trait::async_trait;
use const_format::formatcp;
use flate2::read::GzDecoder;
use http::header::{ACCEPT, CONTENT_TYPE};
use http::{header, HeaderValue, Method, Request};
use hyper::Client;
use hyper_trust_dns_connector::{new_async_http_connector, AsyncHyperResolver};
use log;
use metadata::{APPLICATION_NAME, HOMEPAGE, VERSION};
use sha2::{Digest as ShaDigest, Sha256, Sha512};
use thiserror::Error;
use tokio::sync::Mutex;
use tower::{Service, ServiceBuilder, ServiceExt};
use tower_http::classify::StatusInRangeAsFailures;
use tower_http::decompression::DecompressionLayer;
use tower_http::follow_redirect::FollowRedirectLayer;
use tower_http::set_header::SetRequestHeaderLayer;
use tower_http::trace::{Trace, TraceLayer};

use super::image::{
    Algorithm,
    Descriptor,
    Digest,
    Image,
    Manifest,
    ManifestItem,
    ManifestList,
    Reference,
};
use crate::dirs::blobs_dir;
use crate::metadata;

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
    async fn download(
        &self,
        url: &String,
        accept_headers: Vec<&String>,
        context: &Context,
    ) -> Result<Response>;
}

#[async_trait]
pub trait Registry {
    async fn manifest(&self, host: &str, name: &str, reference: &Reference) -> Result<Manifest>;
    async fn image(&self, host: &str, name: &str, descriptor: &Descriptor) -> Result<Image>;
    async fn layer(&self, host: &str, name: &str, descriptor: &Descriptor) -> Result<Vec<u8>>;
}

pub struct OciRegistry {
    // TODO cleanup this type. Cannot use Box<dyn ...> because of generic arguments in a method
    downloader: DecompressDownloader<
        VerifyingDownloader<CachingDownloader<VerifyingDownloader<TowerDownloader>>>,
    >,
}

impl OciRegistry {
    pub fn new() -> Result<Self> {
        let cache_dir = blobs_dir().context("could not determin blob directory")?;
        let downloader = DecompressDownloader {
            inner: VerifyingDownloader {
                context: "cache",
                inner: CachingDownloader {
                    cache_dir,
                    inner: VerifyingDownloader {
                        context: "download",
                        // inner: ReqwestDownloader::default(),
                        inner: TowerDownloader::default(),
                    },
                },
            },
        };
        Ok(OciRegistry { downloader })
    }

    fn blob_url(&self, host: &str, name: &str, descriptor: &Descriptor) -> String {
        format!("https://{}/v2/{}/blobs/{}", host, name, descriptor.digest)
    }
}

#[async_trait]
impl Registry for OciRegistry {
    async fn manifest(&self, host: &str, name: &str, reference: &Reference) -> Result<Manifest> {
        log::debug!(
            "fetching manifest for `{}` with reference `{}`",
            name,
            reference
        );
        let url = format!("https://{}/v2/{}/manifests/{}", host, name, reference);
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

                let manifest_url =
                    format!("https://{}/v2/{}/manifests/{}", host, name, item.digest);
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

    async fn image(&self, host: &str, name: &str, descriptor: &Descriptor) -> Result<Image> {
        log::debug!(
            "fetching image config for `{}` with digest `{}`",
            name,
            descriptor.digest
        );
        let uri = self.blob_url(host, name, descriptor);
        let response = self
            .downloader
            .download(
                &uri,
                vec![&descriptor.media_type],
                &Context::Descriptor(descriptor),
            )
            .await?;

        log::debug!("deserializing image config `{}`", name);
        let image = serde_json::from_slice(&response.bytes)?;
        Ok(image)
    }

    async fn layer(&self, host: &str, name: &str, descriptor: &Descriptor) -> Result<Vec<u8>> {
        log::debug!(
            "fetching layer for `{}` with digest `{}`",
            name,
            descriptor.digest
        );
        let uri = self.blob_url(host, name, descriptor);
        let response = self
            .downloader
            .download(&uri, vec![], &Context::Descriptor(descriptor))
            .await?;

        Ok(response.bytes)
    }
}

#[derive(Debug)]
pub struct TowerDownloader {
    client: Arc<
        Mutex<
            Trace<
                tower_http::set_header::request::SetRequestHeader<
                    tower_http::decompression::Decompression<
                        tower_http::follow_redirect::FollowRedirect<
                            // hyper::client::Client<hyper::client::connect::HttpsConnector>,
                            hyper::client::Client<
                                // hyper_tls::HttpsConnector<hyper::client::connect::HttpConnector>,
                                hyper_rustls::HttpsConnector<
                                    hyper::client::connect::HttpConnector<AsyncHyperResolver>,
                                >,
                            >,
                        >,
                    >,
                    http::header::HeaderValue,
                >,
                tower_http::classify::SharedClassifier<
                    tower_http::classify::StatusInRangeAsFailures,
                >,
            >,
        >,
    >,
}

const USER_AGENT: &str = formatcp!("{}/{} ({})", APPLICATION_NAME, VERSION, HOMEPAGE);
impl Default for TowerDownloader {
    fn default() -> Self {
        let mut http = new_async_http_connector().unwrap();
        http.enforce_http(false);
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .wrap_connector(http);

        let hyper = Client::builder().build::<_, hyper::Body>(https);

        let client = ServiceBuilder::new()
            .layer(TraceLayer::new(
                StatusInRangeAsFailures::new(400..=599).into_make_classifier(),
            ))
            .layer(SetRequestHeaderLayer::overriding(
                header::USER_AGENT,
                HeaderValue::from_static(USER_AGENT),
            ))
            .layer(DecompressionLayer::new())
            .layer(FollowRedirectLayer::new())
            .service(hyper);

        TowerDownloader {
            client: Arc::new(Mutex::new(client)),
        }
    }
}

#[async_trait]
impl Downloader for TowerDownloader {
    async fn download(
        &self,
        url: &String,
        accept_headers: Vec<&String>,
        _: &Context,
    ) -> Result<Response> {
        let mut builder = Request::builder().uri(url).method(Method::GET);
        {
            let headers = builder.headers_mut().unwrap();
            for header in accept_headers {
                headers.insert(ACCEPT, HeaderValue::from_str(header)?);
            }
        }

        let request = builder.body(hyper::Body::empty())?;

        log::debug!("downloading `{} {}`", request.method(), request.uri());
        let mut client = self.client.lock().await;
        let ready_client = client.ready().await?;
        let response = ready_client.call(request).await?;

        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .context("response is missing Content-Type header")?
            .to_str()
            .context("could not convert Content-Type header to string")?
            .to_string()
            .clone();
        log::trace!("received context type `{}`", content_type);

        let status = response.status();
        if !status.is_success() {
            return Err(anyhow!("received unsucessful response status `{}`", status));
        }

        let bytes = hyper::body::to_bytes(response.into_body()).await;
        match bytes {
            Ok(bytes) => Ok(Response {
                bytes: bytes.to_vec(),
                content_type,
            }),
            Err(_err) => todo!(),
        }
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
                expected: expected_size,
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
    async fn download(
        &self,
        url: &String,
        accept_headers: Vec<&String>,
        context: &Context,
    ) -> Result<Response> {
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
    async fn download(
        &self,
        url: &String,
        accept_headers: Vec<&String>,
        context: &Context,
    ) -> Result<Response> {
        let mut response = self.inner.download(url, accept_headers, context).await?;

        if response.content_type.ends_with("+gzip") {
            let mut decoder = GzDecoder::new(&response.bytes[..]);

            let mut decompressed: Vec<u8> = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
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
        data_path.push("data");

        let mut type_path = location.clone();
        type_path.push("type");

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
    async fn download(
        &self,
        url: &String,
        accept_headers: Vec<&String>,
        context: &Context,
    ) -> Result<Response> {
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

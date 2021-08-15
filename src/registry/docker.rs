// use crate::metadata::{HOMEPAGE, NAME, VERSION};
// use crate::oci::image::{
//     Digest as OciDigest, Manifest as OciManifest, ManifestItem, ManifestList as OciManifestList,
// };
// use crate::verify::{Algorithm, Verify};
// use anyhow::{anyhow, Context, Result};
// use async_trait::async_trait;
// use regex::Regex;
// use reqwest::header::CONTENT_TYPE;
// use reqwest::header::{HeaderMap, ACCEPT};
// use reqwest::{Client, Method};
// use serde::de;
// use serde::de::{Deserialize, Deserializer};
// use serde::ser::{Serialize, Serializer};
// use serde_derive::{Deserialize, Serialize};
// use std::convert::TryFrom;
// use std::env::consts;
// use std::error;
// use std::fmt;

// const DIGEST_PATTERN: &str = "^(?P<algorithm>[A-Za-z0-9_+.-]+):(?P<hex>[A-Fa-f0-9]+)$";
// use super::{ContainerRegistry, Image};

// #[derive(Debug, Clone)]
// struct LegacyDigest {
//     algorithm: Algorithm,
//     hex: String,
// }

// impl From<LegacyDigest> for OciDigest {
//     fn from(digest: LegacyDigest) -> Self {
//         OciDigest {
//             algorithm: digest.algorithm,
//             encoded: digest.hex,
//         }
//     }
// }

// impl TryFrom<&str> for LegacyDigest {
//     type Error = ParseDigestError;

//     fn try_from(value: &str) -> Result<Self, Self::Error> {
//         let regex = Regex::new(DIGEST_PATTERN).unwrap();
//         let optional_captures = regex
//             .captures(&value);

//             let captures = optional_captures.ok_or(ParseDigestError)
//             .with_context(|| format!("Digest `{}` could not be parsed.", &value))?;

//         let algorithm = captures.name("algorithm").unwrap().as_str();
//         let hex = captures.name("hex").unwrap().as_str();

//         let algorithm = match algorithm {
//             "sha256" => Ok(Algorithm::SHA256),
//             "sha512" => Ok(Algorithm::SHA512),
//             _ => Err(ParseDigestError),
//         }
//         .with_context(|| {
//             format!(
//                 "Unsupported algorithm `{}` in digest `{}`.",
//                 &algorithm, &value
//             )
//         })?;

//         Ok(LegacyDigest {
//             algorithm,
//             hex: hex.to_string(),
//         })
//     }
// }

// #[derive(Clone, Debug, PartialEq)]
// pub struct ParseDigestError;

// impl From<anyhow::Error> for ParseDigestError {
//     fn from(_: anyhow::Error) -> Self {
//         ParseDigestError
//     }
// }

// impl fmt::Display for ParseDigestError {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "Invalid docker digest format")
//     }
// }

// impl error::Error for ParseDigestError {}

// impl fmt::Display for LegacyDigest {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "{}:{}", &self.algorithm, &self.hex)
//     }
// }

// impl<'de> Deserialize<'de> for LegacyDigest {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         let string = String::deserialize(deserializer)?;
//         LegacyDigest::try_from(string.as_str()).map_err(de::Error::custom)
//     }
// }

// impl Serialize for LegacyDigest {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         let val = format!("{}:{}", &self.hex, &self.algorithm);
//         serializer.serialize_str(val.as_str())
//     }
// }

// #[derive(Debug)]
// pub struct Registry {
//     client: Client,
// }

// enum Manifest {
//     OciImage(OciManifest),
//     OciList(OciManifestList),
//     Legacy(LegacyManifest),
// }

// #[derive(Serialize, Deserialize, Debug, Clone)]
// struct LegacyManifest {
//     #[serde(rename = "schemaVersion")]
//     pub schema_version: u32,

//     #[serde(rename = "fsLayers")]
//     pub fs_layers: Vec<LegacyLayer>,
// }

// #[derive(Serialize, Deserialize, Debug, Clone)]
// struct LegacyLayer {
//     #[serde(rename = "blobSum")]
//     pub blob_sum: LegacyDigest,
// }

// #[async_trait]
// impl ContainerRegistry for Registry {
//     async fn download(&self, host: &str, name: &str, reference: &str) -> Result<super::Image> {
//         let manifest = download_manifest(&self.client, &host, &name, &reference, true).await?;
//         let layers = download_blobs(&self.client, host, name, &manifest).await?;

//         for layer in layers.iter() {
//             layer.verify()?;
//         }

//         let image = Image {
//             registry: host.to_owned(),
//             name: name.to_owned(),
//             // size: manifest.size,
//             layers,
//             // digest: manifest.digest,
//         };
//         // image.verify()?;
//         Ok(image)
//     }
// }

// impl Registry {
//     pub fn new() -> Self {
//         Registry::default()
//     }
// }

// impl Default for Registry {
//     fn default() -> Self {
//         Registry {
//             client: Client::builder()
//                 .user_agent(format!("{}/{} ({})", NAME, VERSION, HOMEPAGE))
//                 .build()
//                 .unwrap(),
//         }
//     }
// }

// async fn download_manifest(
//     client: &Client,
//     host: &str,
//     name: &str,
//     reference: &str,
//     support_list: bool,
// ) -> Result<Manifest> {
//     let manifest_uri = format!("https://{}/v2/{}/manifests/{}", host, name, reference);

//     let mut headers = HeaderMap::new();
//     headers.append(
//         ACCEPT,
//         "application/vnd.oci.image.manifest.v1+json".parse()?,
//     );
//     if support_list {
//         headers.append(ACCEPT, "application/vnd.oci.image.index.v1+json".parse()?);
//     }

//     let request = client
//         .request(Method::GET, &manifest_uri)
//         .headers(headers)
//         .build()?;

//     let response = client.execute(request).await?;
//     let content_type = response
//         .headers()
//         .get(CONTENT_TYPE)
//         .with_context(|| format!("Missing expected header `{}`", CONTENT_TYPE))?
//         .to_str()?;

//     match content_type {
//         "application/vnd.docker.distribution.manifest.v1+json" => {
//             let manifest = response.json::<LegacyManifest>().await?;
//             Ok(Manifest::Legacy(manifest))
//         }
//         "application/vnd.docker.distribution.manifest.v2+json" => {
//             let manifest = response.json::<OciManifest>().await?;
//             Ok(Manifest::OciImage(manifest))
//         }
//         "application/vnd.docker.distribution.manifest.list.v2+json" => {
//             let manifest = response.json::<OciManifestList>().await?;
//             Ok(Manifest::OciList(manifest))
//         }
//         _ => Err(anyhow!(
//             "Received unsupported content type `{}` for request `{} {}`",
//             &content_type,
//             Method::GET,
//             &manifest_uri
//         )),
//     }
// }

// fn platform_manifest(manifests: &Vec<ManifestItem>) -> Option<&ManifestItem> {
//     manifests
//         .iter()
//         .find(|manifest| manifest.platform.supported())
// }

// struct LayerReference {
//     digest: OciDigest,
//     size: Option<usize>,
// }

// async fn resolve_digests(
//     client: &Client,
//     host: &str,
//     name: &str,
//     manifest: &Manifest,
// ) -> Result<Vec<LayerReference>> {
//     match manifest {
//         Manifest::OciList(list) => match platform_manifest(&list.manifests) {
//             Some(manifest_item) => {
//                 let digest = &manifest_item.digest;
//                 let reference = format!("{}:{}", &digest.algorithm, &digest.encoded);
//                 let downloaded_manifest =
//                     download_manifest(&client, &host, &name, &reference, false).await?;

//                 // Do not perform recursion here because of difficult return types.
//                 // Only recurses a single time, so inlining the body again is much easier
//                 match downloaded_manifest {
//                     Manifest::OciImage(manifest) => {
//                         let layers = manifest
//                             .clone()
//                             .layers
//                             .into_iter()
//                             .map(|layer| LayerReference {
//                                 digest: layer.digest,
//                                 size: Some(layer.size),
//                             })
//                             .collect();
//                         Ok(layers)
//                     }
//                     Manifest::Legacy(manifest) => {
//                         let layers = manifest
//                             .clone()
//                             .fs_layers
//                             .into_iter()
//                             .map(|layer| LayerReference {
//                                 digest: layer.blob_sum.into(),
//                                 size: None,
//                             })
//                             .collect();
//                         Ok(layers)
//                     }
//                     Manifest::OciList(_) => Err(anyhow!(
//                         "Unexpectedly received a list manifest within a list manifest."
//                     )),
//                 }
//             }
//             None => Err(anyhow!(
//                 "Image `{}` does not support `{}/{}`.",
//                 &name,
//                 consts::OS,
//                 consts::ARCH
//             )),
//         },
//         Manifest::OciImage(manifest) => {
//             let layers = manifest
//                 .clone()
//                 .layers
//                 .into_iter()
//                 .map(|layer| LayerReference {
//                     digest: layer.digest,
//                     size: Some(layer.size),
//                 })
//                 .collect();
//             Ok(layers)
//         }
//         Manifest::Legacy(manifest) => {
//             let layers = manifest
//                 .clone()
//                 .fs_layers
//                 .into_iter()
//                 .map(|layer| LayerReference {
//                     digest: layer.blob_sum.into(),
//                     size: None,
//                 })
//                 .collect();
//             Ok(layers)
//         }
//     }
// }

// async fn download_blobs(
//     client: &Client,
//     host: &str,
//     name: &str,
//     manifest: &Manifest,
// ) -> Result<Vec<super::Layer>> {
//     let mut layers = Vec::new();

//     let digests = resolve_digests(client, host, name, manifest).await?;
//     for layer_reference in digests.iter() {
//         let layer = download_blob(
//             &client,
//             &host,
//             &name,
//             &layer_reference.digest.clone().into(),
//             &layer_reference.size,
//         )
//         .await?;
//         layers.push(layer);
//     }

//     Ok(layers)
// }

// async fn download_blob(
//     client: &Client,
//     host: &str,
//     name: &str,
//     digest: &OciDigest,
//     expected_size: &Option<usize>,
// ) -> Result<super::Layer> {
//     let blob_uri = format!("https://{}/v2/{}/blobs/{}", &host, &name, &digest);

//     let response = reqwest::get(&blob_uri).await?;
//     let blob = response.bytes().await?;
//     let size = blob.len();

//     if let Some(expected) = expected_size {
//         if size != *expected {
//             return Err(anyhow!(
//                 "Received `{}` bytes, expected `{}`.",
//                 &size,
//                 &expected,
//             ));
//         }
//     }

//     Ok(super::Layer {
//         /*
//            "To ensure security, the content should be verified against the digest used to fetch the content.
//            At times, the returned digest may differ from that used to initiate a request.
//            Such digests are considered to be from different domains, meaning they have different values for algorithm.
//            In such a case, the client may choose to verify the digests in both domains or ignore the server’s digest.
//            To maintain security, the client must always verify the content against the digest used to fetch the content."
//            - https://docs.docker.com/registry/spec/api/#content-digests
//         */
//         digest: digest.clone(),
//         size,
//         bytes: blob.to_vec(),
//     })
// }

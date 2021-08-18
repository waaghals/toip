use anyhow::Result;
use regex::Regex;
use serde::de;
use serde::de::{Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::env::consts::{ARCH as CURRENT_ARCHITECTURE, OS as CURRENT_OS};
use std::fmt;
use thiserror::Error;

#[derive(Serialize, Deserialize, Debug)]
pub struct Image {
    #[serde(rename = "architecture")]
    pub architecture: Architecture,

    #[serde(rename = "author")]
    pub author: Option<String>,

    #[serde(rename = "config")]
    pub config: Option<Config>,

    #[serde(rename = "created")]
    pub created: Option<String>,

    // #[serde(rename = "history")]
    // pub history: Option<Vec<History>>,
    #[serde(rename = "os")]
    pub os: OperatingSystem,

    #[serde(rename = "rootfs")]
    pub rootfs: Rootfs,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(rename = "Cmd")]
    pub cmd: Option<Vec<String>>,

    #[serde(rename = "Entrypoint")]
    pub entrypoint: Option<Vec<String>>,

    #[serde(rename = "Env")]
    pub env: Option<Vec<String>>,

    #[serde(rename = "ExposedPorts")]
    pub exposed_ports: Option<HashMap<String, Option<u16>>>,

    #[serde(rename = "Labels")]
    pub labels: Option<HashMap<String, Option<String>>>,

    #[serde(rename = "StopSignal")]
    pub stop_signal: Option<String>,

    #[serde(rename = "User")]
    pub user: Option<String>,

    #[serde(rename = "Volumes")]
    pub volumes: Option<HashMap<String, Option<String>>>,

    #[serde(rename = "WorkingDir")]
    pub working_dir: Option<String>,
}

// #[derive(Serialize, Deserialize, Debug)]
// pub struct History {
//     #[serde(rename = "author")]
//     pub author: Option<String>,

//     #[serde(rename = "comment")]
//     pub comment: Option<String>,

//     #[serde(rename = "created")]
//     pub created: Option<String>,

//     #[serde(rename = "created_by")]
//     pub created_by: Option<String>,

//     #[serde(rename = "empty_layer")]
//     pub empty_layer: Option<bool>,
// }

#[derive(Serialize, Deserialize, Debug)]
pub struct Rootfs {
    #[serde(rename = "diff_ids")]
    pub diff_ids: Vec<Digest>,

    #[serde(rename = "type")]
    pub fs_type: Type,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Type {
    #[serde(rename = "layers")]
    Layers,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,

    #[serde(rename = "config")]
    pub config: Descriptor,

    #[serde(rename = "layers")]
    pub layers: Vec<Descriptor>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ManifestList {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,

    #[serde(rename = "manifests")]
    pub manifests: Vec<ManifestItem>,
}

impl ManifestList {
    pub fn supported(&self) -> Option<&ManifestItem> {
        let os = host_os()?;
        let arch = host_arch()?;

        self.manifests
            .iter()
            .find(|item| item.platform.os == os && item.platform.architecture == arch)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ManifestItem {
    #[serde(rename = "mediaType")]
    pub media_type: String,

    #[serde(rename = "digest")]
    pub digest: Digest,

    #[serde(rename = "size")]
    pub size: u64,

    #[serde(rename = "platform")]
    pub platform: Platform,
}

impl fmt::Display for ManifestItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.platform)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Platform {
    #[serde(rename = "architecture")]
    pub architecture: Architecture,

    #[serde(rename = "os")]
    pub os: OperatingSystem,

    #[serde(rename = "os.version")]
    pub os_version: Option<String>,

    #[serde(rename = "variant")]
    pub variant: Option<Variant>,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(variant) = &self.variant {
            write!(f, "{:?}/{:?} ({:?})", self.os, self.architecture, variant)
        } else {
            write!(f, "{:?}/{:?}", self.os, self.architecture)
        }
    }
}

fn host_os() -> Option<OperatingSystem> {
    match CURRENT_OS {
        "linux" => Some(OperatingSystem::Linux),
        "macos" => Some(OperatingSystem::Darwin),
        "ios" => Some(OperatingSystem::IOS),
        "freebsd" => Some(OperatingSystem::FreeBSD),
        "dragonfly" => Some(OperatingSystem::Dragonfly),
        "netbsd" => Some(OperatingSystem::NetBSD),
        "openbsd" => Some(OperatingSystem::OpenBSD),
        "solaris" => Some(OperatingSystem::Solaris),
        "android" => Some(OperatingSystem::Android),
        "windows" => Some(OperatingSystem::Windows),
        _ => None,
    }
}

fn host_arch() -> Option<Architecture> {
    match CURRENT_ARCHITECTURE {
        "x86" => Some(Architecture::X86),
        "x86_64" => Some(Architecture::AMD64),
        "arm" => Some(Architecture::ARM),
        "aarch64" => Some(Architecture::ARM64),
        "mips" => Some(Architecture::MIPS),
        "mips64" => Some(Architecture::MIPS64),
        "powerpc64" => Some(Architecture::PPC64),
        "riscv64" => Some(Architecture::RISCV64),
        "s390x" => Some(Architecture::S390x),
        "sparc64" | "powerpc" | _ => None,
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Architecture {
    #[serde(rename = "ppc64")]
    PPC64,

    #[serde(rename = "386")]
    X86,

    #[serde(rename = "amd64")]
    AMD64,

    #[serde(rename = "arm")]
    ARM,

    #[serde(rename = "arm64")]
    ARM64,

    #[serde(rename = "wasm")]
    WASM,

    #[serde(rename = "ppc64le")]
    PPC64le,

    #[serde(rename = "mips")]
    MIPS,

    #[serde(rename = "mipsle")]
    MIPSle,

    #[serde(rename = "mips64")]
    MIPS64,

    #[serde(rename = "mips64le")]
    MIPS64le,

    #[serde(rename = "riscv64")]
    RISCV64,

    #[serde(rename = "s390x")]
    S390x,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum OperatingSystem {
    #[serde(rename = "aix")]
    AIX,

    #[serde(rename = "android")]
    Android,

    #[serde(rename = "darwin")]
    Darwin,

    #[serde(rename = "dragonfly")]
    Dragonfly,

    #[serde(rename = "freebsd")]
    FreeBSD,

    #[serde(rename = "illumos")]
    Illumos,

    #[serde(rename = "ios")]
    IOS,

    #[serde(rename = "js")]
    JS,

    #[serde(rename = "linux")]
    Linux,

    #[serde(rename = "netbsd")]
    NetBSD,

    #[serde(rename = "openbsd")]
    OpenBSD,

    #[serde(rename = "plan9")]
    Plan9,

    #[serde(rename = "solaris")]
    Solaris,

    #[serde(rename = "windows")]
    Windows,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Variant {
    #[serde(rename = "v6")]
    V6,

    #[serde(rename = "v7")]
    V7,

    #[serde(rename = "v8")]
    V8,

    #[serde(rename = "v9")]
    V9,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Descriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,

    #[serde(rename = "digest")]
    pub digest: Digest,

    #[serde(rename = "size")]
    pub size: u64,
    // #[serde(rename = "annotations")]
    // pub annotations: Option<HashMap<String, String>>,
}

impl fmt::Display for Descriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.media_type, self.digest)
    }
}

const DIGEST_PATTERN: &str =
    "^(?P<algorithm>[a-z0-9]+(?:[+._-][a-z0-9]+)?):(?P<encoded>[a-zA-Z0-9=_-]+)$";

impl From<Digest> for String {
    fn from(digest: Digest) -> Self {
        format!("{}:{}", digest.encoded, digest.algorithm)
    }
}
// impl From<anyhow::Error> for ParseDigestError {
//     fn from(_: anyhow::Error) -> Self {
//         ParseDigestError
//     }
// }

impl TryFrom<&str> for Digest {
    type Error = ParseDigestError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let regex = Regex::new(DIGEST_PATTERN).unwrap();
        let captures = regex.captures(value).ok_or(ParseDigestError::Unparsable {
            input: value.to_string(),
        })?;

        let captured_algorithm = captures.name("algorithm").unwrap().as_str();
        let encoded = captures.name("encoded").unwrap().as_str();

        let algorithm = match captured_algorithm {
            "sha256" => Ok(Algorithm::SHA256),
            "sha512" => Ok(Algorithm::SHA512),
            _ => Err(ParseDigestError::UnsupportedAlgorithm {
                algorithm: captured_algorithm.to_string(),
                input: value.to_string(),
            }),
        }?;

        Ok(Digest {
            algorithm,
            encoded: encoded.to_string(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Digest {
    pub algorithm: Algorithm,
    pub encoded: String,
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum ParseDigestError {
    #[error("unparsable digest `{input}`")]
    Unparsable { input: String },
    #[error("unsupported algorithm `{algorithm}` in digest `{input}`")]
    UnsupportedAlgorithm { algorithm: String, input: String },
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", &self.algorithm, &self.encoded)
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
        let val = format!("{}:{}", &self.algorithm, &self.encoded);
        serializer.serialize_str(val.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reference {
    Digest(Digest),
    Tag(String),
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reference::Digest(digest) => write!(f, "{}", digest),
            Reference::Tag(tag) => write!(f, "{}", tag),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Algorithm {
    SHA256,
    SHA512,
}

impl fmt::Display for Algorithm {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Algorithm::SHA256 => write!(f, "sha256"),
            Algorithm::SHA512 => write!(f, "sha512"),
        }
    }
}

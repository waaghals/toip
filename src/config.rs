use std::collections::hash_map::Values;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::{error, fmt};

use anyhow::{Context, Result};
use regex::Regex;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_derive::{Deserialize as DeriveDeserialize, Serialize as DeriveSerialize};
use thiserror::Error;

use crate::oci::image::Reference;

const REGISTRY_PATTERN: &str = r"^(?:(?P<registry>(?:[a-zA-Z0-9]+\.[a-zA-Z0-9.]+?)|[a-zA-Z0-9]+\.)/)?(?P<repository>[a-z0-9][a-z0-9._-]*(?:/[a-z0-9][a-z0-9._-]*)?)(?:(?::(?P<tag>[a-zA-Z0-9_][a-zA-Z0-9._-]+))|@(?P<digest>[a-zA-Z0-9]+:[a-zA-Z0-9]+))?$";

#[derive(Debug, Clone, PartialEq)]
pub struct RegistrySource {
    pub registry: String,
    pub repository: String,
    pub reference: Reference,
}

impl fmt::Display for RegistrySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.reference {
            Reference::Digest(digest) => {
                write!(f, "{}/{}@{}", self.registry, self.repository, digest)
            }
            Reference::Tag(tag) => write!(f, "{}/{}:{}", self.registry, self.repository, tag),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathSource {
    context: PathBuf,
}

impl fmt::Display for PathSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", &self.context)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildSource {
    container_file: Option<PathBuf>,
    context: PathBuf,
}

impl fmt::Display for BuildSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.container_file {
            Some(container_file) => {
                write!(f, "{:?}?containerfile={:?}", self.context, container_file)
            }
            None => write!(f, "{:?}", self.context),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageSource {
    Registry(RegistrySource),
    Path(PathSource),
    Build(BuildSource),
}

impl TryFrom<&str> for RegistrySource {
    type Error = ParseImageError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let regex = Regex::new(REGISTRY_PATTERN).unwrap();
        let captures = regex
            .captures(value)
            .with_context(|| format!("image reference `{}` could not be parsed.", value))
            .map_err(|_| ParseImageError::InvalidRegistry)?;

        let registry = match captures.name("registry") {
            Some(registry_match) => registry_match.as_str(),
            None => "registry-1.docker.io",
        };
        let reference = match captures.name("digest") {
            Some(digest_match) => {
                let digest = digest_match
                    .as_str()
                    .try_into()
                    .map_err(|_| ParseImageError::InvalidRegistry)?;
                Reference::Digest(digest)
            }
            None => match captures.name("tag") {
                Some(tag) => Reference::Tag(tag.as_str().to_string()),
                None => Reference::Tag("latest".to_string()),
            },
        };
        let repository = captures.name("repository").unwrap().as_str();

        Ok(RegistrySource {
            registry: registry.into(),
            repository: repository.into(),
            reference,
        })
    }
}

impl TryFrom<&str> for PathSource {
    type Error = ParseImageError;

    fn try_from(_value: &str) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl TryFrom<&str> for BuildSource {
    type Error = ParseImageError;

    fn try_from(_value: &str) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl TryFrom<&str> for ImageSource {
    type Error = ParseImageError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if let Some(part) = value.strip_prefix("registry://") {
            let registry = part.try_into()?;
            Ok(ImageSource::Registry(registry))
        } else if let Some(part) = value.strip_prefix("build://") {
            let config = part.try_into()?;
            Ok(ImageSource::Build(config))
        } else if let Some(part) = value.strip_prefix("path://") {
            let config = part.try_into()?;
            Ok(ImageSource::Path(config))
        } else {
            Err(ParseImageError::UnknownScheme)
        }
    }
}

impl fmt::Display for ImageSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageSource::Registry(registry) => write!(f, "registry://{}", registry),
            ImageSource::Path(path) => write!(f, "path://{}", path),
            ImageSource::Build(build) => write!(f, "build://{}", build),
        }
    }
}

#[derive(Debug)]
pub enum ParseImageError {
    UnknownScheme,
    InvalidRegistry,
    InvalidPath,
    InvalidBuild,
}

impl fmt::Display for ParseImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseImageError::InvalidBuild => write!(f, "invalid image build"),
            ParseImageError::InvalidPath => write!(f, "invalid image path"),
            ParseImageError::InvalidRegistry => write!(f, "invalid image registry"),
            ParseImageError::UnknownScheme => write!(f, "unknown image scheme"),
        }
    }
}

impl error::Error for ParseImageError {}

impl<'de> Deserialize<'de> for ImageSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string = String::deserialize(deserializer)?;
        ImageSource::try_from(string.as_str()).map_err(de::Error::custom)
    }
}

impl Serialize for ImageSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let val = format!("{}", self);
        serializer.serialize_str(val.as_str())
    }
}

#[derive(Debug, DeriveDeserialize, DeriveSerialize, Clone)]
pub struct ContainerConfig {
    pub image: ImageSource,
    pub links: Option<HashMap<String, String>>,
    pub entrypoint: Option<Vec<String>>,
    pub cmd: Option<Vec<String>>,
    pub volumes: Option<HashMap<String, String>>,
    pub env: Option<HashMap<String, String>>,
    pub inherit_envvars: Option<Vec<String>>,
}

#[derive(Debug, DeriveDeserialize, DeriveSerialize, Clone)]
pub struct Config {
    containers: HashMap<String, ContainerConfig>,
    volumes: HashMap<String, String>,
    aliases: HashMap<String, String>,
}

#[derive(Debug, DeriveDeserialize, DeriveSerialize)]
pub struct RuntimeConfig {
    pub container_name: String,
    pub config: Config,
}

#[derive(Error, Debug)]
pub enum ContainerError {
    #[error("unknown alias `{alias}`")]
    UnknownAlias { alias: String },

    #[error("unknown container `{container}` for alias `{alias}`")]
    UnknownContainer { alias: String, container: String },
}

impl Config {
    pub fn get_container_by_alias(&self, alias: &str) -> Result<&ContainerConfig, ContainerError> {
        match self.aliases.get(alias) {
            Some(name) => match self.containers.get(name) {
                Some(container) => Ok(container),
                None => Err(ContainerError::UnknownContainer {
                    alias: alias.to_string(),
                    container: name.to_string(),
                }),
            },
            None => Err(ContainerError::UnknownAlias {
                alias: alias.to_string(),
            }),
        }
    }

    pub fn containers(&self) -> Values<String, ContainerConfig> {
        self.containers.values()
    }

    pub fn get_container_by_name(&self, name: &str) -> Option<ContainerConfig> {
        let container = self.containers.get(name);
        container.cloned()
    }
}

pub fn from_file(file_name: &Path) -> Result<Config> {
    let file = File::open(file_name)
        .with_context(|| format!("config file `{:?}` not found.", file_name))?;
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader
        .read_to_string(&mut contents)
        .with_context(|| format!("unable to read config file `{:?}`.", file_name))?;

    toml::from_str(&contents)
        .with_context(|| format!("unable to parse config file `{:?}`.", file_name))
}

pub fn from_dir(dir: &Path) -> Result<Config> {
    from_file(&dir.join(".doe.toml"))
}

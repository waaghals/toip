use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::{error, fmt};

use anyhow::{bail, Context, Result};
use regex::Regex;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_derive::{Deserialize as DeriveDeserialize, Serialize as DeriveSerialize};

use crate::oci::image::Reference;

const REGISTRY_PATTERN: &str = r"^(?:(?P<registry>(?:[a-zA-Z0-9]+\.[a-zA-Z0-9.]+?)|[a-zA-Z0-9]+\.)/)?(?P<repository>[a-z0-9][a-z0-9._-]*(?:/[a-z0-9][a-z0-9._-]*)?)(?:(?::(?P<tag>[a-zA-Z0-9_][a-zA-Z0-9._-]+))|@(?P<digest>[a-zA-Z0-9]+:[a-zA-Z0-9]+))?$";
const CONFIG_FILE_NAME: &str = "toip.yaml";

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
    pub container_file: Option<PathBuf>,
    pub context: PathBuf,
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
        } else {
            Err(ParseImageError::UnknownScheme)
        }
    }
}

impl fmt::Display for ImageSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageSource::Registry(registry) => write!(f, "registry://{}", registry),
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
    pub containers: HashMap<String, ContainerConfig>,
    pub volumes: HashMap<String, String>,
    pub aliases: HashMap<String, String>,
}

#[derive(Debug, DeriveDeserialize, DeriveSerialize)]
pub struct RuntimeConfig {
    pub container_name: String,
    pub config: Config,
}

impl Config {
    pub fn get_container_by_name(&self, name: &str) -> Option<ContainerConfig> {
        let container = self.containers.get(name);
        container.cloned()
    }

    pub fn new<R>(read: R) -> Result<Config>
    where
        R: Read,
    {
        let mut buf_reader = BufReader::new(read);
        let mut contents = String::new();
        buf_reader
            .read_to_string(&mut contents)
            .context("unable to read config")?;

        serde_yaml::from_str(&contents).context("unable to parse config")
    }

    pub fn new_from_dir<D>(dir: D) -> Result<Config>
    where
        D: Into<PathBuf>,
    {
        let mut path = dir.into();
        path.push(CONFIG_FILE_NAME);

        if !path.is_file() {
            bail!("path `{}` is not an file", path.display());
        }

        let file = File::open(&path)
            .with_context(|| format!("could not read configuration file `{}`", path.display()))?;

        Config::new(&file)
            .with_context(|| format!("could not parse configuration file `{}`", path.display()))
    }
}

pub fn find_config_file<P>(starting_dir: P) -> Option<PathBuf>
where
    P: Into<PathBuf>,
{
    let mut path: PathBuf = starting_dir.into();
    let file_name = Path::new(CONFIG_FILE_NAME);

    loop {
        path.push(file_name);

        if path.is_file() {
            break Some(path);
        }

        if !(path.pop() && path.pop()) {
            // remove file && remove parent
            break None;
        }
    }
}

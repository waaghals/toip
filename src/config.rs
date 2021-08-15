use std::collections::hash_map::{Keys, Values};
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::{error, fmt};

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_derive::{Deserialize as DeriveDeserialize, Serialize as DeriveSerialize};

use crate::oci::image::Reference;

const REGISTRY_PATTERN: &str = r"^(?:(?P<registry>[a-zA-Z0-9][a-zA-Z0-9.]+?)/)?(?P<repository>[a-z0-9][a-z0-9._-]*(?:/[a-z0-9][a-z0-9._-]*)?)(?:(?::(?P<tag>[a-zA-Z0-9_][a-zA-Z0-9._-]+))|@(?P<digest>[a-zA-Z0-9]+:[a-zA-Z0-9]+))?$";

#[derive(Debug, Clone, PartialEq)]
pub struct RegistryImage {
    pub registry: String,
    pub repository: String,
    pub reference: Reference,
}

impl fmt::Display for RegistryImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.reference {
            Reference::Digest(digest) => write!(f, "{}/{}@{}", self.registry, self.repository, digest),
            Reference::Tag(tag) => write!(f, "{}/{}:{}", self.registry, self.repository, tag),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Path {
    context: PathBuf,
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", &self.context)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Build {
    container_file: Option<PathBuf>,
    context: PathBuf,
}

impl fmt::Display for Build {
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
pub enum ImageReference {
    Registry(RegistryImage),
    Path(Path),
    Build(Build),
}

impl TryFrom<&str> for RegistryImage {
    type Error = ParseImageError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let regex = Regex::new(REGISTRY_PATTERN).unwrap();
        let captures = regex
            .captures(value)
            .with_context(|| format!("Image reference `{}` could not be parsed.", value))
            .map_err(|_| ParseImageError::InvalidRegistry)?;

        let registry = match captures.name("registry") {
            Some(registry_match) => registry_match.as_str(),
            None => "docker.io",
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

        Ok(RegistryImage {
            registry: registry.into(),
            repository: repository.into(),
            reference,
        })
    }
}

impl TryFrom<&str> for Path {
    type Error = ParseImageError;

    fn try_from(_value: &str) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl TryFrom<&str> for Build {
    type Error = ParseImageError;

    fn try_from(_value: &str) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl TryFrom<&str> for ImageReference {
    type Error = ParseImageError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.starts_with("registry://") {
            let part = &value[11..];
            let registry = part.try_into()?;
            Ok(ImageReference::Registry(registry))
        } else if value.starts_with("build://") {
            let part = &value[8..];
            let config = part.try_into()?;
            Ok(ImageReference::Build(config))
        } else if value.starts_with("path://") {
            let part = &value[7..];
            let config = part.try_into()?;
            Ok(ImageReference::Path(config))
        } else {
            Err(ParseImageError::UnknownScheme)
        }
    }
}

impl fmt::Display for ImageReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageReference::Registry(registry) => write!(f, "registry://{}", registry),
            ImageReference::Path(path) => write!(f, "path://{}", path),
            ImageReference::Build(build) => write!(f, "build://{}", build),
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
            ParseImageError::InvalidBuild => write!(f, "Invalid image build"),
            ParseImageError::InvalidPath => write!(f, "Invalid image path"),
            ParseImageError::InvalidRegistry => write!(f, "Invalid image registry"),
            ParseImageError::UnknownScheme => write!(f, "Unknown image scheme"),
        }
    }
}

impl error::Error for ParseImageError {}

impl<'de> Deserialize<'de> for ImageReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string = String::deserialize(deserializer)?;
        ImageReference::try_from(string.as_str()).map_err(de::Error::custom)
    }
}

impl Serialize for ImageReference {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let val = format!("{}", self);
        serializer.serialize_str(val.as_str())
    }
}

#[derive(Debug, DeriveDeserialize, DeriveSerialize, Clone)]
pub struct Container {
    pub image: ImageReference,
    pub links: Option<HashMap<String, String>>,
    pub cmd: String,
    pub args: Option<Vec<String>>,
    pub volumes: Option<HashMap<String, String>>,
    pub envvars: Option<HashMap<String, String>>,
}

struct MissingContainerForLink {
    container: String,
    link: String,
}

struct MissingContainerForAlias {
    container: String,
    alias: String,
}

pub struct Errors {
    missing_containers_for_alias: Vec<MissingContainerForAlias>,
    missing_containers_for_link: Vec<MissingContainerForLink>,
}

impl fmt::Debug for Errors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for missing_container in &self.missing_containers_for_alias {
            writeln!(
                f,
                "Alias \"{}\": no container named \"{}\".",
                missing_container.alias, missing_container.container
            )?;
        }
        for missing_container in &self.missing_containers_for_link {
            writeln!(
                f,
                "Link \"{}\": no container named \"{}\".",
                missing_container.link, missing_container.container
            )?;
        }
        Ok(())
    }
}

impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for missing_container in &self.missing_containers_for_alias {
            writeln!(
                f,
                "Alias \"{}\": no container named \"{}\".",
                missing_container.alias, missing_container.container
            )?;
        }
        for missing_container in &self.missing_containers_for_link {
            writeln!(
                f,
                "Link \"{}\": no container named \"{}\".",
                missing_container.link, missing_container.container
            )?;
        }
        Ok(())
    }
}

impl Error for Errors {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

#[derive(Debug, DeriveDeserialize, DeriveSerialize, Clone)]
pub struct Config {
    containers: HashMap<String, Container>,
    aliases: HashMap<String, String>,
}

#[derive(Debug, DeriveDeserialize, DeriveSerialize)]
pub struct RuntimeConfig {
    pub container_name: String,
    pub config: Config,
}

impl Config {
    pub fn get_container_by_alias(&self, name: &str) -> Option<Result<(&str, &Container)>> {
        match self.aliases.get(name) {
            Some(container_name) => {
                let result = match self.containers.get(container_name) {
                    Some(container) => Ok((container_name.as_str(), container)),
                    None => Err(anyhow!(
                        "Alias `{}` resolves to unknown container `{}`.",
                        name,
                        container_name
                    )),
                };
                Some(result)
            }
            None => None,
        }
    }

    pub fn containers(&self) -> Values<String, Container> {
        self.containers.values()
    }

    pub fn get_container_by_name(&self, name: &String) -> Option<&Container> {
        self.containers.get(name)
    }

    pub fn available_aliases(&self) -> Keys<'_, String, String> {
        self.aliases.keys()
    }

    pub fn validate<'a>(&self) -> Result<(), Errors> {
        Ok(())
        // return Err(Errors {
        //     missing_containers_for_alias: vec![MissingContainerForAlias {
        //         container: String::from("container-value"),
        //         alias: String::from("alias-value")
        //     }],
        //     missing_containers_for_link: vec![MissingContainerForLink {
        //         container: String::from("container-value"),
        //         link: String::from("link-value")
        //     }]
        // });
    }
}

pub fn from_file(file_name: &PathBuf) -> Result<Config> {
    let file = File::open(file_name)
        .with_context(|| format!("Config file `{:?}` not found.", file_name))?;
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader
        .read_to_string(&mut contents)
        .with_context(|| format!("Unable to read config file `{:?}`.", file_name))?;

    serde_json::from_str(&contents)
        .with_context(|| format!("Unable to parse config file `{:?}`.", file_name))
}

pub fn from_dir(dir: &PathBuf) -> Result<Config> {
    from_file(&dir.join(".doe.json"))
}

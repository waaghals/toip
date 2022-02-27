use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::fs::File;
use std::io::{BufReader, Read};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{error, fmt, str};

use anyhow::{bail, Context, Result};
use regex::Regex;
use serde::de::{Error, MapAccess, Unexpected, Visitor};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_derive::Deserialize as DeriveDeserialize;

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

#[derive(Debug, Clone, PartialEq, DeriveDeserialize)]
pub struct BuildSource {
    pub file: Option<PathBuf>,
    pub target: Option<String>,
    pub context: PathBuf,
    #[serde(default)]
    pub build_args: HashMap<String, EnvString>,
    #[serde(default)]
    pub secrets: HashMap<String, EnvPathBuf>,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_ssh")]
    pub ssh: HashMap<String, EnvPathBuf>,
}

impl fmt::Display for BuildSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.file {
            Some(container_file) => {
                write!(f, "{:?}?containerfile={:?}", self.context, container_file)
            }
            None => write!(f, "{:?}", self.context),
        }
    }
}

#[derive(Debug, Clone, PartialEq, DeriveDeserialize)]
#[serde(tag = "type")]
pub enum ImageSource {
    #[serde(rename = "volume")]
    Registry(RegistrySource),
    #[serde(rename = "build")]
    Build(BuildSource),
}

#[derive(Debug, Clone, PartialEq, DeriveDeserialize)]
pub struct BindVolume {
<<<<<<< HEAD
    pub source: EnvPathBuf,
=======
    #[serde(deserialize_with = "substitute_pathbuf")]
    pub source: PathBuf,
>>>>>>> main
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Debug, Clone, PartialEq, DeriveDeserialize)]
pub struct AnonymousVolume {
<<<<<<< HEAD
    pub name: EnvString,
=======
    #[serde(deserialize_with = "substitute_string")]
    pub name: String,
>>>>>>> main
    #[serde(default)]
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, DeriveDeserialize)]
#[serde(tag = "type")]
pub enum Volume {
    #[serde(rename = "volume")]
    Anonymous(AnonymousVolume),
    #[serde(rename = "bind")]
    Bind(BindVolume),
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

// impl<'de> Deserialize<'de> for ImageSource {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         let string = String::deserialize(deserializer)?;
//         ImageSource::try_from(string.as_str()).map_err(de::Error::custom)
//     }
// }

impl Serialize for ImageSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let val = format!("{}", self);
        serializer.serialize_str(val.as_str())
    }
}

impl<'de> Deserialize<'de> for RegistrySource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string = String::deserialize(deserializer)?;
        RegistrySource::try_from(string.as_str()).map_err(de::Error::custom)
    }
}

#[derive(Debug, DeriveDeserialize, Clone)]
pub struct ContainerConfig {
    pub image: ImageSource,
    #[serde(default)]
    pub links: HashMap<String, String>,
    pub entrypoint: Option<String>,
    pub workdir: Option<PathBuf>,
    pub cmd: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub volumes: HashMap<PathBuf, String>,
    #[serde(default)]
    pub env: HashMap<String, EnvString>,
    #[serde(default)]
    pub inherit_envvars: Vec<String>,
}

#[derive(Debug, DeriveDeserialize, Clone)]
pub struct Config {
    pub containers: HashMap<String, ContainerConfig>,
    #[serde(default)]
    pub volumes: HashMap<String, Volume>,
    pub aliases: HashMap<String, String>,
}

#[derive(Debug, DeriveDeserialize)]
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

<<<<<<< HEAD
#[derive(Debug, Clone, PartialEq)]
pub struct EnvSub<T> {
    substituted: T,
}

impl<T> EnvSub<T> {
    pub fn into_inner(self) -> T {
        self.substituted
    }
}

impl<T> AsRef<Path> for EnvSub<T>
where
    T: AsRef<Path>,
{
    fn as_ref(&self) -> &Path {
        self.substituted.as_ref()
    }
}

type EnvPathBuf = EnvSub<PathBuf>;
type EnvString = EnvSub<String>;

impl<'de, T> Deserialize<'de> for EnvSub<T>
where
    T: Deserialize<'de> + FromStr,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SubstitutingVisitor<T>(PhantomData<fn() -> T>);

        impl<'de, T> Visitor<'de> for SubstitutingVisitor<T>
        where
            T: Deserialize<'de> + FromStr,
        {
            type Value = T;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("string or anything")
            }

            fn visit_str<E>(self, value: &str) -> Result<T, E>
            where
                E: de::Error,
            {
                let substituted = subst::substitute(value, &subst::Env)
                    .map_err(|err| de::Error::custom(format!("{}", err)))?;

                T::from_str(substituted.as_str())
                    .map_err(|_| de::Error::custom(format!("Failed to parse `{}`", substituted)))
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Deserialize::deserialize(de::value::BytesDeserializer::new(v))
            }

            // fn visit_seq<A>(self, v: A) -> Result<Self::Value, A::Error>
            // where
            //     A: SeqAccess<'de>,
            // {
            //     Deserialize::deserialize(de::value::SeqDeserializer::new(v))
            // }

            fn visit_map<A>(self, v: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                Deserialize::deserialize(de::value::MapAccessDeserializer::new(v))
            }
        }

        let value = deserializer.deserialize_any(SubstitutingVisitor(PhantomData))?;
        Ok(EnvSub { substituted: value })
    }
}

fn deserialize_ssh<'de, D>(deserializer: D) -> Result<HashMap<String, EnvPathBuf>, D::Error>
where
    D: Deserializer<'de>,
{
    struct SshVisitor;

    impl<'de> Visitor<'de> for SshVisitor {
        type Value = HashMap<String, EnvPathBuf>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<HashMap<String, EnvPathBuf>, E>
        where
            E: de::Error,
        {
            if value != "default" {
                Err(de::Error::invalid_value(Unexpected::Str(value), &"default"))
            } else {
                let mut map = HashMap::new();
                let socket = std::env::var("SSH_AUTH_SOCK")
                    .map_err(|_| de::Error::custom("Missing environment variable `SSH_AUTH_SOCK`. Consider configuring it in `.env.local`"))?;
                map.insert(
                    "default".to_owned(),
                    EnvSub {
                        substituted: PathBuf::from(socket),
                    },
                );
                Ok(map)
            }
        }

        fn visit_map<M>(self, map: M) -> Result<HashMap<String, EnvPathBuf>, M::Error>
        where
            M: MapAccess<'de>,
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(SshVisitor)
=======
struct BytesVisitor;

impl<'de> Visitor<'de> for BytesVisitor {
    type Value = Vec<u8>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a PathBuf")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(s.into())
    }

    fn visit_string<E>(self, s: String) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(s.into())
    }
}

fn substitute_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = deserializer.deserialize_str(BytesVisitor)?;

    let substituted = subst::substitute_bytes(value.as_ref(), &subst::Env)
        .map_err(|err| D::Error::custom(format!("{}", err)))?;

    String::from_utf8(substituted)
        .map_err(|_err| D::Error::custom(format!("Failed to substitute environment")))
}

fn substitute_pathbuf<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    let value = deserializer.deserialize_str(BytesVisitor)?;

    let substituted = subst::substitute_bytes(value.as_ref(), &subst::Env)
        .map_err(|err| D::Error::custom(format!("{}", err)))?;

    let str = str::from_utf8(substituted.as_ref())
        .map_err(|_err| D::Error::custom(format!("Failed to substitute environment")))?;

    PathBuf::from_str(str)
        .map_err(|_err| D::Error::custom(format!("Failed to substitute environment")))
>>>>>>> main
}

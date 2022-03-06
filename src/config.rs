use std::collections::HashMap;
use std::convert::{Infallible, TryFrom, TryInto};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Read};
use std::marker::PhantomData;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{fmt, str};

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde::de::{Error, MapAccess, Unexpected, Visitor};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_derive::{Deserialize as DeriveDeserialize, Serialize as DeriveSerialize};
use sha2::{Digest as Sha2Digest, Sha256};

const CONFIG_FILE_NAME: &str = "toip.yaml";

#[derive(Debug, Clone, PartialEq, DeriveDeserialize, DeriveSerialize)]
pub struct RegistrySource {
    #[serde(default)]
    pub registry: String,
    pub repository: String,
    #[serde(default)]
    pub reference: Reference,
}

impl Default for RegistrySource {
    fn default() -> Self {
        RegistrySource {
            registry: "localhost".to_string(),
            // TODO hash based on container config
            repository: "123456789".to_string(),
            reference: Default::default(),
        }
    }
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

const REGISTRY_PATTERN: &str = r"^(?:(?P<registry>(?:[a-zA-Z0-9]+\.[a-zA-Z0-9.]+?)|[a-zA-Z0-9]+\.)/)?(?P<repository>[a-z0-9][a-z0-9._-]*(?:/[a-z0-9][a-z0-9._-]*)?)(?:(?::(?P<tag>[a-zA-Z0-9_][a-zA-Z0-9._-]*))|@(?P<digest>[a-zA-Z0-9]+:[a-zA-Z0-9]+))?$";
impl TryFrom<&str> for RegistrySource {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let regex = Regex::new(REGISTRY_PATTERN).unwrap();
        let captures = regex
            .captures(value)
            .with_context(|| format!("image reference `{}` could not be parsed.", value))?;

        let registry = match captures.name("registry") {
            Some(registry_match) => registry_match.as_str(),
            None => "registry-1.docker.io",
        };
        let reference = match captures.name("digest") {
            Some(digest_match) => {
                let string = digest_match.as_str();
                let digest = string
                    .try_into()
                    .with_context(|| format!("could not parse digest `{}`", string))?;
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

#[derive(Debug, Clone, PartialEq, DeriveSerialize, DeriveDeserialize)]
pub enum Reference {
    Digest(Digest),
    Tag(String),
}

impl Default for Reference {
    fn default() -> Self {
        Reference::Tag("latest".to_owned())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Digest {
    pub algorithm: Algorithm,
    pub encoded: String,
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", &self.algorithm, &self.encoded)
    }
}

const DIGEST_PATTERN: &str =
    "^(?P<algorithm>[a-z0-9]+(?:[+._-][a-z0-9]+)?):(?P<encoded>[a-zA-Z0-9=_-]+)$";
impl TryFrom<&str> for Digest {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let regex = Regex::new(DIGEST_PATTERN).unwrap();
        let captures = regex
            .captures(value)
            .ok_or_else(|| anyhow!("failed to parse digest from `{}`", value))?;

        let captured_algorithm = captures.name("algorithm").unwrap().as_str();
        let encoded = captures.name("encoded").unwrap().as_str();

        let algorithm = match captured_algorithm {
            "sha256" => Ok(Algorithm::SHA256),
            "sha512" => Ok(Algorithm::SHA512),
            _ => Err(anyhow!(
                "unsupported algorithm `{}` in digest `{}`",
                captured_algorithm,
                value
            )),
        }?;

        Ok(Digest {
            algorithm,
            encoded: encoded.to_string(),
        })
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

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reference::Digest(digest) => write!(f, "{}", digest),
            Reference::Tag(tag) => write!(f, "{}", tag),
        }
    }
}

#[derive(Debug, Clone, PartialEq, DeriveSerialize)]
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

#[derive(Debug, Clone, PartialEq, Default, DeriveDeserialize, DeriveSerialize)]
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

impl FromStr for BuildSource {
    type Err = Infallible;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let context = PathBuf::from_str(value)?;
        Ok(BuildSource {
            context,
            ..Default::default()
        })
    }
}

#[derive(Debug, Clone, PartialEq, DeriveDeserialize)]
pub struct BindVolume {
    pub source: EnvPathBuf,
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Debug, Clone, PartialEq, DeriveDeserialize)]
pub struct AnonymousVolume {
    pub name: EnvString,
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

#[derive(Debug, DeriveDeserialize, DeriveSerialize, Clone)]
pub struct ContainerConfig {
    #[serde(default)]
    #[serde(deserialize_with = "registry")]
    pub image: Option<RegistrySource>,
    #[serde(default)]
    #[serde(deserialize_with = "build")]
    pub build: Option<BuildSource>,
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

#[derive(Debug, Clone, PartialEq, DeriveSerialize)]
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
}

fn build<'de, D>(deserializer: D) -> Result<Option<BuildSource>, D::Error>
where
    D: Deserializer<'de>,
{
    struct BuildSourceVisitor;

    impl<'de> Visitor<'de> for BuildSourceVisitor {
        type Value = Option<BuildSource>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let result = BuildSource::from_str(value).unwrap();
            Ok(Some(result))
        }

        fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(None)
        }

        fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            BuildSource::deserialize(deserializer).map(Some)
        }

        fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let result = Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))?;
            Ok(Some(result))
        }
    }

    deserializer.deserialize_any(BuildSourceVisitor)
}

fn registry<'de, D>(deserializer: D) -> Result<Option<RegistrySource>, D::Error>
where
    D: Deserializer<'de>,
{
    struct RegistrySourceVisitor;

    impl<'de> Visitor<'de> for RegistrySourceVisitor {
        type Value = Option<RegistrySource>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let result = RegistrySource::try_from(value)
                .map_err(|err| de::Error::custom(err.to_string()))?;
            Ok(Some(result))
        }

        fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(None)
        }

        fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            RegistrySource::deserialize(deserializer).map(Some)
        }

        fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let result = Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))?;
            Ok(Some(result))
        }
    }

    deserializer.deserialize_any(RegistrySourceVisitor)
}

pub fn hash<D>(dir: D) -> Result<String>
where
    D: AsRef<OsStr>,
{
    let data = dir.as_ref().as_bytes();
    Ok(format!("{:x}", Sha256::digest(data)))
}

use anyhow::{anyhow, Context, Result};
use serde_derive::{Deserialize, Serialize};
use std::collections::hash_map::Keys;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Container {
    pub image: String,
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
        return Ok(());
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
        return Ok(());
    }
}

impl Error for Errors {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    containers: HashMap<String, Container>,
    aliases: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
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

    pub fn get_container_by_name(&self, name: &String) -> Option<&Container> {
        return self.containers.get(name);
    }

    pub fn available_aliases(&self) -> Keys<'_, String, String> {
        return self.aliases.keys();
    }

    pub fn validate<'a>(&self) -> Result<(), Errors> {
        return Ok(());
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

    toml::from_str(&contents)
        .with_context(|| format!("Unable to parse config file `{:?}`.", file_name))
}

pub fn from_dir(dir: &PathBuf) -> Result<Config> {
    from_file(&dir.join(".doe.toml"))
}

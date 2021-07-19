use serde_derive::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::fmt;
use std::collections::hash_map::Keys;

#[derive(Debug, Deserialize)]
pub struct Container {
    image: String,
    links: Option<HashMap<String, String>>,
    volumes: Option<HashMap<String, String>>,
    envvars: Option<HashMap<String, String>>,
}

impl Container {
    pub fn image(&self) -> &String {
        &self.image
    }
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
            writeln!(f, "Alias \"{}\": no container named \"{}\".", missing_container.alias, missing_container.container)?;
        }
        for missing_container in &self.missing_containers_for_link {
            writeln!(f, "Link \"{}\": no container named \"{}\".", missing_container.link, missing_container.container)?;
        }
        return Ok(());
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    containers: HashMap<String, Container>,
    aliases: HashMap<String, String>,
}

impl Config {
    pub fn get_container_by_alias(&self, name: &String) -> Option<&Container> {
        let alias = self.aliases.get(name);
        match alias {
            Some(container) => self.containers.get(container),
            None => None,
        }
    }

    pub fn available_aliases(&self) -> Keys<'_, String, String>{
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

fn parse_config(file: &PathBuf) -> Config {
    let file = File::open(file).unwrap();
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents).unwrap();

    toml::from_str(&contents).unwrap()
}

pub fn from_dir(dir: &PathBuf) -> Option<Config> {
    Some(parse_config(&dir.join(".doe.toml")))
}
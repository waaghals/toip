use serde_derive::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{PathBuf};

pub fn files(top_path: &PathBuf) -> impl Iterator<Item=ConfigLocation> + '_ {
    top_path.ancestors()
        .map(|path| path.join(".doe"))
        .filter(|path| path.exists())
        .map(|path| ConfigLocation { path: path.parent().unwrap().to_path_buf(), config: parse_config(&path) })
}

#[derive(Debug)]
pub struct ConfigLocation {
    path: PathBuf,
    config: Config,
}

impl ConfigLocation {
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
}

#[derive(Debug, Deserialize)]
pub struct Alias {
    run: String,
}

impl Alias {
    pub fn run(&self) -> &String {
        &self.run
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    aliases: Option<HashMap<String, Alias>>
}

impl Config {
    pub fn aliases(&self) -> &Option<HashMap<String, Alias>> {
        &self.aliases
    }
}

fn parse_config(file: &PathBuf) -> Config {
    let file = File::open(file).unwrap();
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents).unwrap();

    toml::from_str(&contents).unwrap()
}

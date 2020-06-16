use serde_derive::Deserialize;
use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;
use std::env;
use std::process::Command;

#[derive(Debug, Deserialize)]
struct Config {
    aliases: Option<Vec<Alias>>,
}

#[derive(Debug, Deserialize)]
struct Alias {
    alias: String,
    command: String,
}

fn main() {
    // TODO find file recursivly
    // TODO proper error handling
    let file = File::open(".doe.toml").unwrap();
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents).unwrap();
    
    println!("{:#?}", env::args());
    let config: Config = toml::from_str(&contents).unwrap();
    let args = env::args().skip(1);
    let input = args.collect::<Vec<String>>().join(" ");

    let matched_alias = find_matching_alias(&input, &config);
    match matched_alias{
        None => {
            println!("Not found");
        }
        Some(alias) => {
            let command = &alias.command;
            println!("Running {:#?}", command);
            Command::new(command)
            .spawn()
            .expect("failed to execute process");
        }
    }
    // println!("Matching alias result {:#?}", );
}

fn find_matching_alias<'a>(command: &String, config: &'a Config) -> Option<&'a Alias> {
    match &config.aliases {
        None => {
            return None;
        }
        Some(aliases) => {
            for item in aliases {
                if command.starts_with(&item.alias) {
                    return Some(item);
                }
            }

            return None;
        },
    }
}
use serde_derive::Deserialize;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::io::{self, Write};
use std::process::Command;
use std::str::SplitWhitespace;

#[derive(Debug, Deserialize)]
struct Config {
    aliases: Option<Vec<Alias>>,
}

#[derive(Debug, Deserialize)]
struct Alias {
    alias: String,
    command: String,
    arguments: Vec<String>
}

fn main() {
    // TODO find file recursivly
    // TODO proper error handling
    let file = File::open(".doe.toml").unwrap();
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents).unwrap();

    let config: Config = toml::from_str(&contents).unwrap();
    println!("{:#?}", config);
    let args = env::args().skip(1);
    let input = args.collect::<Vec<String>>().join(" ");

    let matched_alias = find_matching_alias(&input, &config);
    match matched_alias {
        None => {
            println!("Not found");
        }
        Some(alias) => {
            let command = &alias.command;
            let alias_arguments = &alias.arguments;
            let prefix_length = alias.alias.chars().count();
            let user_arguments = arguments(&input, prefix_length);
            println!("user args {:#?}", user_arguments);
            println!("{:#?}", command);

            let output = Command::new(command)
                .args(alias_arguments)
                .args(user_arguments)
                .output()
                .expect("failed to execute process");

            io::stdout().write_all(&output.stdout).unwrap();
            io::stderr().write_all(&output.stderr).unwrap();
        }
    }
}

fn find_matching_alias<'a>(command: &str, config: &'a Config) -> Option<&'a Alias> {
    match &config.aliases {
        None => {
            None
        }
        Some(aliases) => {
            for item in aliases {                
                if command.starts_with(&item.alias) {
                    return Some(item);
                }
            }

            None
        }
    }
}

fn arguments(input: &str, alias_lenght: usize) -> SplitWhitespace {
    slice_arguments(input, alias_lenght).split_whitespace()
}

fn slice_arguments(input: &str, alias_lenght: usize) -> &str {
    match input.char_indices().nth(alias_lenght) {
        Some((pos, _)) => &input[pos..],
        None => "",
    }
}

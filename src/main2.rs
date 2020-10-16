use serde_derive::Deserialize;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::io::{self, Write};
use std::process::Command;
use std::str::SplitWhitespace;

//
use portable_pty::{CommandBuilder, PtySize, native_pty_system, PtySystem};
// use anyhow::Error;

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

    let pty_system = native_pty_system();

    // Create a new pty
    let mut pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        // Not all systems support pixel_width, pixel_height,
        // but it is good practice to set it to something
        // that matches the size of the selected font.  That
        // is more complex than can be shown here in this
        // brief example though!
        pixel_width: 0,
        pixel_height: 0,
    }).unwrap();


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

            // let output = Command::new(command)
            //     .args(alias_arguments)
            //     .args(user_arguments)
            //     .output()
            //     .expect("failed to execute process");

            // Spawn a shell into the pty
            let mut cmd = CommandBuilder::new(command);
            cmd.args(alias_arguments);
            cmd.args(user_arguments);
            let child = pair.slave.spawn_command(cmd).unwrap();

// Read and parse output from the pty with reader
let mut reader = pair.master.try_clone_reader().unwrap();
let mut writer = pair.master.try_clone_writer().unwrap();

            // io::stdout().write_all(writer).unwrap();
            // io::stderr().write_all(&output.stderr).unwrap();
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

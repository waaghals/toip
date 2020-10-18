use std::env;
use std::env::current_dir;

use crate::config::{Alias, ConfigLocation};

mod config;
mod parser;

fn main() {
    // TODO proper error handling
    let dir = current_dir().unwrap();
    let configs: Vec<ConfigLocation> = config::files(&dir).collect();

    let input = env::args().skip(1).collect::<Vec<String>>().join(" ");

    let matched_alias = find_matching_alias(&input, &configs);

    match matched_alias {
        None => {
            println!("Not found");
        }
        Some((alias, config)) => {
            let run = &config.run();
            println!("Run: {:#?}", run);
            println!("Input: {:#?}", input);
            println!("Alias: {:#?}", alias);
            let args: String = input.chars().skip(alias.len()).collect();
            println!("Arguments: {:#?}", args.trim());
//
//            let mut child = Command::new(command)
//                .spawn()
//                .unwrap();
//
//            // don't accept another command until this one completes
//            child.wait();
        }
    }
}

fn find_matching_alias<'a>(command: &str, configs: &'a Vec<ConfigLocation>) -> Option<(&'a String, &'a Alias)> {
    for config_location in configs {
        let config = config_location.config();
        match config.aliases() {
            None => continue,
            Some(aliases) => {
                for (alias, alias_config) in aliases {
                    if command.starts_with(alias) {
                        return Some((alias, alias_config));
                    }
                }
            }
        }
    }
    None
}

// https://www.joshmcguigan.com/blog/build-your-own-shell-rust/
// https://github.com/myfreeweb/interactor/tree/master/src
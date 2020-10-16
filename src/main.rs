use std::env::current_dir;
use std::env;
use std::str::SplitWhitespace;
use crate::config::{ConfigLocation, Alias};

mod config;

fn main() {
    // TODO proper error handling
    let dir = current_dir().unwrap();
    let configs: Vec<ConfigLocation> = config::files(&dir).collect();

    println!("{:#?}", configs);

    let args = env::args().skip(1);
    let input = args.collect::<Vec<String>>().join(" ");

    println!("{:#?}", input);
    let matched_alias = find_matching_alias(&input, &configs);

    match matched_alias {
        None => {
            println!("Not found");
        }
        Some(alias) => {
            let run = &alias.run();
//            let alias_arguments = &alias.arguments;
//            let prefix_length = alias.alias.chars().count();
//            let user_arguments = arguments(&input, prefix_length);
//            println!("user args {:#?}", user_arguments);
            println!("{:#?}", run);
        }
    }
}

fn find_matching_alias<'a>(command: &str, configs: &'a Vec<ConfigLocation>) -> Option<&'a Alias> {
    for configLocation in configs {
        let config = configLocation.config();
        match config.aliases() {
            None => continue,
            Some(aliases) => {
                for (alias, alias_config) in aliases {
                    if command.starts_with(alias) {
                        return Some(alias_config);
                    }
                }
            }
        }
    }
    None
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

// https://www.joshmcguigan.com/blog/build-your-own-shell-rust/
// https://github.com/myfreeweb/interactor/tree/master/src
use std::env;
use std::env::current_dir;

use crate::config::{Config, Errors};

mod config;

fn main() -> Result<(), Errors>{
    let dir = current_dir().unwrap();
    let config: Config = config::from_dir(&dir).unwrap();
    config.validate()?;

    let command = env::args().skip(1).next().unwrap();

    match config.get_container_by_alias(&command) {
        Some(container) => {
            println!("{:#?}", container)
        },
        None => {
            println!("No such container. \nOnly available containers:\n{:#?}", config.available_aliases());
        }
    }

    return Ok(());
}

// https://www.joshmcguigan.com/blog/build-your-own-shell-rust/
// https://github.com/myfreeweb/interactor/tree/master/src
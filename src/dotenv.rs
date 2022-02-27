use std::fs::File;
use std::io::ErrorKind;
use std::{env, fs};

use anyhow::{Context, Result};
use dotenv::{dotenv, Error};
use futures_util::future::err;
use itertools::Itertools;

pub fn load() -> Result<()> {
    for file in vec![".env.local", ".env"] {
        // Ignore not found errors
        let result = match dotenv::from_filename(file) {
            Ok(_) => Ok(()),
            Err(error) => match &error {
                Error::Io(io_error) => match io_error.kind() {
                    ErrorKind::NotFound => Ok(()),
                    _ => Err(error),
                },
                _ => Err(error),
            },
        };

        if result.is_err() {
            result?
        }
    }

    Ok(())
}

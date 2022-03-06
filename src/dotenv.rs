use std::io::ErrorKind;

use anyhow::Result;
use dotenv::Error;

pub fn load() -> Result<()> {
    for file in &[".env.local", ".env"] {
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

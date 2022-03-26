use std::fs::File;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::{anyhow, bail, Context, Result};

use crate::backend::script;
use crate::config::Config;
use crate::{config, dirs};

fn create_scripts<D>(directory: D, config: &Config) -> Result<()>
where
    D: Into<PathBuf>,
{
    let directory = directory.into();
    let current_exe = env::current_exe()?;
    fs::create_dir_all(&directory)
        .with_context(|| format!("could not create directory `{}`", directory.display()))?;
    for container_name in config.containers.keys() {
        let mut script_path = directory.clone();
        script_path.push(&container_name);
        script::create_run(&script_path, &current_exe, container_name).with_context(|| {
            format!(
                "could not create run script for directory `{}`",
                directory.display()
            )
        })?;
    }

    Ok(())
}

fn modify_lookup<D>(target_dir: D) -> Result<()>
where
    D: AsRef<Path>,
{
    let target_dir_display = target_dir.as_ref().display();
    log::info!(
        "Pointing scripts lookup directory to `{}`",
        target_dir_display
    );
    let bin_dir = dirs::path().context("could not determine bin backend")?;

    if let Some(parent) = bin_dir.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "could not create parent directory for symlink `{}`",
                    bin_dir.display()
                )
            })?;
        }
    }

    if bin_dir.exists() {
        // Not actually a directory as it is a symlink
        fs::remove_file(&bin_dir)
            .with_context(|| format!("could not remove `{}`", bin_dir.display()))?;
    }

    unix_fs::symlink(&target_dir, &bin_dir).with_context(|| {
        format!(
            "could not configure symlink `{}` to `{}`",
            target_dir_display,
            bin_dir.display()
        )
    })
}

pub fn install(ignore_missing_config: bool) -> Result<()> {
    let current_dir = env::current_dir()?;

    let config_path = config::find_config_file(current_dir);

    match config_path {
        None => {
            let empty = Path::new("/dev/null");
            modify_lookup(&empty).context("could not modify container lookup directory")?;
            if ignore_missing_config {
                Ok(())
            } else {
                bail!("Missing config file");
            }
        }
        Some(file) => {
            let config_file = File::open(&file).with_context(|| {
                format!(
                    "could not open config file `{}` for reading",
                    file.display()
                )
            })?;

            let config = Config::new(config_file).with_context(|| {
                format!("could not create config from file `{}`", file.display())
            })?;

            // Parent directory always exists because a file always
            // exists within a directory
            let config_dir = file.parent().unwrap();

            let script_dir = dirs::script(&config_dir)?;

            if script_dir.exists() {
                // Reset whole directory
                fs::remove_dir_all(&script_dir).with_context(|| {
                    format!(
                        "could not reset scripts directory `{}`",
                        script_dir.display()
                    )
                })?;
            }

            create_scripts(&script_dir, &config).with_context(|| {
                format!(
                    "could not create scripts in directory `{}`",
                    script_dir.display()
                )
            })?;

            let mut new_config_path = script_dir.clone();
            // Do not hard code the config file name here, but derive it from the current config file
            let config_file_name = file
                .file_name()
                .ok_or_else(|| anyhow!("Failed to determine config file name"))?;

            new_config_path.push(&config_file_name);
            fs::copy(&file, &new_config_path).with_context(|| {
                format!(
                    "could not copy configuration file `{}` to `{}`",
                    file.display(),
                    new_config_path.display()
                )
            })?;

            modify_lookup(&script_dir).context("could not modify container lookup directory")?;

            Ok(())
        }
    }
}

use std::collections::VecDeque;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::{Context, Result};
use itertools::Itertools;

use crate::cli::Shell;
use crate::config::Config;
use crate::{config, dirs, script};

fn create_scripts<D>(directory: D, config: &Config) -> Result<()>
where
    D: Into<PathBuf>,
{
    let directory = directory.into();
    let current_exe = env::current_exe()?;
    fs::create_dir_all(&directory)
        .with_context(|| format!("could not create directory `{}`", directory.display()))?;
    for (alias, container_name) in config.aliases.iter() {
        let mut script_path = directory.clone();
        script_path.push(&alias);
        script::create_run(&script_path, &current_exe, &container_name).with_context(|| {
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
    let bin_dir = dirs::path().context("could not determine bin dir")?;

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
            target_dir.as_ref().display(),
            bin_dir.display()
        )
    })
}

pub fn install() -> Result<()> {
    let current_dir = env::current_dir()?;
    let dir = dirs::script(&current_dir)?;
    let config = config::from_dir(&current_dir)?;
    create_scripts(&dir, &config)
        .with_context(|| format!("could not create script in directory `{}`", dir.display()))?;
    modify_lookup(dir).context("could not modify alias lookup directory")?;

    Ok(())
}

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::{anyhow, Context, Result};
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

fn modify_path<D>(script_dir: D) -> Result<()>
where
    D: AsRef<Path>,
{
    let path = script_dir.as_ref();
    let path = path.to_str().with_context(|| {
        format!(
            "could not convert path `{}` into string representation",
            path.display()
        )
    })?;

    let current_path = env::var("PATH").context("environment variable `PATH` is not set")?;
    let mut current: VecDeque<&str> = current_path.split(":").collect();

    let scripts_dir = dirs::scripts_dir()?;
    let scripts_str = scripts_dir
        .to_str()
        .ok_or(anyhow!("cannot convert scripts directory to string"))?;
    current.retain(|path| !path.starts_with(scripts_str));
    current.push_front(path);

    env::set_var("PATH", current.into_iter().join(":"));

    Ok(())
}

pub fn inject(_shell: Shell) -> Result<()> {
    let current_dir = env::current_dir()?;
    let dir = dirs::script(&current_dir)?;
    let config = config::from_dir(&current_dir)?;
    create_scripts(&dir, &config)
        .with_context(|| format!("could not create script in directory `{}`", dir.display()))?;
    modify_path(dir).context("could not modify PATH variable")?;

    Ok(())
}

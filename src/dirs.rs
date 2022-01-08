use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;

use crate::metadata::{APPLICATION_NAME, ORGANIZATION, QUALIFIER};

fn project_directories() -> Result<ProjectDirs> {
    let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION_NAME)
        .context("could not determin application directories")?;

    Ok(dirs)
}

fn cache_dir<P>(sub_directory: P) -> Result<PathBuf>
where
    P: AsRef<Path>,
{
    let project_directories = project_directories()?;
    let cache_directory = project_directories.cache_dir();
    let mut directory: PathBuf = cache_directory.into();
    directory.push(sub_directory);
    Ok(directory)
}

fn data_dir<P>(sub_directory: P) -> Result<PathBuf>
where
    P: AsRef<Path>,
{
    let project_directories = project_directories()?;
    let data_directory = project_directories.data_dir();
    let mut directory: PathBuf = data_directory.into();
    directory.push(sub_directory);
    Ok(directory)
}

pub fn layers_dir() -> Result<PathBuf> {
    cache_dir("layers")
}

pub fn blobs_dir() -> Result<PathBuf> {
    cache_dir("blobs")
}

pub fn containers_dir() -> Result<PathBuf> {
    cache_dir("containers")
}

pub fn volumes_dir() -> Result<PathBuf> {
    data_dir("volumes")
}

pub fn run_dir() -> Result<PathBuf> {
    let project_directories = project_directories()?;
    let run_directory = project_directories.runtime_dir();

    match run_directory {
        Some(directory) => Ok(directory.into()),
        None => data_dir("run"),
    }
}

pub fn create_directories() -> anyhow::Result<()> {
    create_directory(&layers_dir()?)?;
    create_directory(&blobs_dir()?)?;
    create_directory(&containers_dir()?)?;
    create_directory(&volumes_dir()?)?;
    create_directory(&run_dir()?)?;

    Ok(())
}

fn create_directory(dir: &PathBuf) -> anyhow::Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("could not create directory `{:#?}`", dir))
}

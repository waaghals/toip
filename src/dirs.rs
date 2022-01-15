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

fn state_dir<P>(sub_directory: P) -> Result<PathBuf>
where
    P: AsRef<Path>,
{
    let project_directories = project_directories()?;
    let state_directory = project_directories.state_dir();
    match state_directory {
        Some(directory) => {
            let mut directory: PathBuf = directory.into();
            directory.push(sub_directory);
            Ok(directory)
        }
        None => cache_dir(sub_directory),
    }
}

fn run_dir<P>(path: P) -> Result<PathBuf>
where
    P: AsRef<Path>,
{
    let project_directories = project_directories()?;
    let run_directory = project_directories.runtime_dir();

    match run_directory {
        Some(directory) => {
            let mut directory: PathBuf = directory.into();
            directory.push(path);
            Ok(directory)
        }
        None => data_dir(path),
    }
}

fn layers_dir() -> Result<PathBuf> {
    cache_dir("layers")
}

pub fn layer_dir<A, H>(algorithm: A, hash: H) -> Result<PathBuf>
where
    A: AsRef<Path>,
    H: AsRef<Path>,
{
    let mut dir = layers_dir()?;
    dir.push(algorithm);
    dir.push(hash);

    Ok(dir)
}

pub fn blobs_dir() -> Result<PathBuf> {
    cache_dir("blobs")
}

fn containers_dir() -> Result<PathBuf> {
    state_dir("containers")
}

fn volumes_dir() -> Result<PathBuf> {
    data_dir("volumes")
}

pub fn volume<V>(volume: V) -> Result<PathBuf>
where
    V: AsRef<Path>,
{
    let mut dir = volumes_dir()?;
    dir.push(volume);
    Ok(dir)
}

pub fn container<C>(container_id: C) -> Result<PathBuf>
where
    C: AsRef<Path>,
{
    let mut dir: PathBuf = containers_dir()?;
    dir.push(container_id);
    Ok(dir)
}

pub fn socket_path() -> Result<PathBuf> {
    run_dir("socket")
}

pub fn create(dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("could not create directory `{:#?}`", dir))
}
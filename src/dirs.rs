use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::{BaseDirs, ProjectDirs};

use crate::config;
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

fn images() -> Result<PathBuf> {
    state_dir("images")
}

pub fn scripts() -> Result<PathBuf> {
    state_dir("scripts")
}

fn volumes_dir() -> Result<PathBuf> {
    data_dir("volumes")
}

pub fn volume<V, S>(volume: V, seed: Option<S>) -> Result<PathBuf>
where
    V: AsRef<Path>,
    S: AsRef<OsStr>,
{
    let mut dir = volumes_dir()?;
    if let Some(seed) = seed {
        let digest = config::hash(seed)?;
        dir.push(digest);
    }
    dir.push(volume);
    Ok(dir)
}

pub fn script<D>(dir: D) -> Result<PathBuf>
where
    D: AsRef<OsStr>,
{
    let digest = config::hash(dir)?;
    let mut dir: PathBuf = scripts()?;
    dir.push(digest);
    Ok(dir)
}

pub fn image<D, I>(driver: D, config_dir: I) -> Result<PathBuf>
where
    D: AsRef<Path>,
    I: AsRef<OsStr>,
{
    let digest = config::hash(config_dir)?;
    let mut dir: PathBuf = images()?;
    dir.push(driver);
    dir.push(digest);
    Ok(dir)
}

pub fn socket_path() -> Result<PathBuf> {
    run_dir("socket")
}

pub fn create(dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("could not create directory `{:#?}`", dir))
}

pub fn path() -> Result<PathBuf> {
    let dirs = BaseDirs::new().context("could not determine home directory")?;
    let bin_dir = dirs
        .executable_dir()
        .context("could not determine binary directory")?;

    let mut path_buf = bin_dir.to_path_buf();
    path_buf.push(APPLICATION_NAME);

    Ok(path_buf)
}

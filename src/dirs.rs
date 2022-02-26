use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use directories::{BaseDirs, ProjectDirs};
use sha2::{Digest, Sha256};

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
fn containers() -> Result<PathBuf> {
    state_dir("containers")
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
    S: AsRef<Path>,
{
    let mut dir = volumes_dir()?;
    if let Some(seed) = seed {
        let data = seed
            .as_ref()
            .to_str()
            .ok_or(anyhow!(
                "cannot convert directory to string to generate volume seed"
            ))?
            .as_ref();
        dir.push(format!("{:x}", Sha256::digest(data)));
    }
    dir.push(volume);
    Ok(dir)
}

pub fn script<D>(dir: D) -> Result<PathBuf>
where
    D: AsRef<Path>,
{
    let data = dir
        .as_ref()
        .to_str()
        .ok_or(anyhow!(
            "cannot convert directory to string to generate script directory hash"
        ))?
        .as_ref();
    let digest = format!("{:x}", Sha256::digest(data));

    let mut dir: PathBuf = scripts()?;
    dir.push(digest);
    Ok(dir)
}

pub fn image<D, I>(driver: D, image_id: I) -> Result<PathBuf>
where
    D: AsRef<Path>,
    I: AsRef<Path>,
{
    let mut dir: PathBuf = images()?;
    dir.push(driver);
    dir.push(image_id);
    Ok(dir)
}

pub fn container<C>(container_id: C) -> Result<PathBuf>
where
    C: AsRef<Path>,
{
    let mut dir: PathBuf = containers()?;
    dir.push(container_id);
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

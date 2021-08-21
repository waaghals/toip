use std::path::{Path, PathBuf};

use crate::metadata::{NAME, ORGANIZATION, QUALIFIER};
use directories::ProjectDirs;

fn project_directories() -> ProjectDirs {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, NAME).unwrap()
}

fn cache_dir<P>(sub_directory: P) -> PathBuf
where
    P: AsRef<Path>,
{
    let project_directories = project_directories();
    let cache_directory = project_directories.cache_dir();
    let mut directory: PathBuf = cache_directory.into();
    directory.push(sub_directory);
    directory
}

fn data_dir<P>(sub_directory: P) -> PathBuf
where
    P: AsRef<Path>,
{
    let project_directories = project_directories();
    let data_directory = project_directories.data_dir();
    let mut directory: PathBuf = data_directory.into();
    directory.push(sub_directory);
    directory
}

pub fn layer_dir() -> PathBuf {
    cache_dir("layers")
}

pub fn blob_dir() -> PathBuf {
    cache_dir("blobs")
}

pub fn container_dir() -> PathBuf {
    cache_dir("containers")
}

pub fn volume_dir() -> PathBuf {
    data_dir("volumes")
}

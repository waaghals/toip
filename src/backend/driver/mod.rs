mod docker;

use std::ffi::OsStr;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::Result;
use async_trait::async_trait;
pub use docker::Docker;

use crate::backend::{BuildArg, EnvVar, Image, Mount, Secret, Ssh};
use crate::oci::image::Reference;

#[async_trait]
pub trait Driver {
    type Image: Image;

    async fn pull(
        &self,
        registry: &str,
        repository: &str,
        reference: &Reference,
    ) -> Result<Self::Image>;

    async fn build<C, F>(
        &self,
        context: C,
        file: F,
        build_args: Vec<BuildArg>,
        secrets: Vec<Secret>,
        ssh_sockets: Vec<Ssh>,
        target: Option<String>,
    ) -> Result<Self::Image>
    where
        C: AsRef<Path> + Send,
        F: AsRef<Path> + Send;

    async fn run(
        &self,
        image: Self::Image,
        mounts: Vec<Mount>,
        entrypoint: Option<String>,
        cmd: Option<String>,
        args: Option<Vec<String>>,
        env_vars: Vec<EnvVar>,
        env_files: Vec<PathBuf>,
        workdir: Option<PathBuf>,
        init: Option<bool>,

        stdin: Stdio,
        stdout: Stdio,
        stderr: Stdio,
    ) -> Result<()>;
}

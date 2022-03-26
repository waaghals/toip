mod docker;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::Result;
use async_trait::async_trait;
pub use docker::DockerCliCompatible;

use crate::backend::{BuildArg, EnvVar, Mount, Secret, Ssh};
use crate::config::{Port, Reference, RegistrySource};

#[async_trait]
pub trait Driver {
    async fn path(&self, _repository: &str, _reference: &Reference) -> Result<Option<String>> {
        Ok(None)
    }

    async fn pull(&self, image: &RegistrySource) -> Result<()>;

    #[allow(clippy::too_many_arguments)]
    async fn build<C, F>(
        &self,
        context: C,
        file: F,
        build_args: Vec<BuildArg>,
        secrets: Vec<Secret>,
        ssh_sockets: Vec<Ssh>,
        target: Option<String>,
        repository: &str,
        reference: &Reference,
    ) -> Result<()>
    where
        C: AsRef<Path> + Send,
        F: AsRef<Path> + Send;

    #[allow(clippy::too_many_arguments)]
    async fn run(
        &self,
        repository: &str,
        reference: &Reference,
        mounts: Vec<Mount>,
        entrypoint: Option<String>,
        cmd: Option<String>,
        args: Option<Vec<String>>,
        env_vars: Vec<EnvVar>,
        env_files: Vec<PathBuf>,
        workdir: Option<PathBuf>,
        init: Option<bool>,
        ports: HashMap<u16, u16>,

        stdin: Stdio,
        stdout: Stdio,
        stderr: Stdio,
    ) -> Result<()>;
}

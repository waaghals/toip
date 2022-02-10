mod docker;

use std::fmt;
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::Result;
use async_trait::async_trait;
pub use docker::Docker;

use crate::config::ContainerConfig;

enum BindPropagation {
    Shared,
    Slave,
    Private,
    Rshared,
    Rslave,
    Rprivate,
}

impl fmt::Display for BindPropagation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BindPropagation::Shared => write!(f, "shared"),
            BindPropagation::Slave => write!(f, "slave"),
            BindPropagation::Private => write!(f, "private"),
            BindPropagation::Rshared => write!(f, "rshared"),
            BindPropagation::Rslave => write!(f, "rslave"),
            BindPropagation::Rprivate => write!(f, "rprivate"),
        }
    }
}

enum BindConsistency {
    Consistent,
    Cached,
    Delegated,
}

impl fmt::Display for BindConsistency {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BindConsistency::Consistent => write!(f, "consistent"),
            BindConsistency::Cached => write!(f, "cached"),
            BindConsistency::Delegated => write!(f, "delegated"),
        }
    }
}

enum MountType {
    Volume {
        source: Option<String>,
    },
    Bind {
        source: PathBuf,
        consistency: Option<BindConsistency>,
        bind_propagation: Option<BindPropagation>,
        bind_nonrecursive: Option<bool>,
    },
    Tmpfs,
}

pub struct Mount {
    mount_type: MountType,
    target: PathBuf,
    readonly: bool,
}

struct Prepper {}

impl Prepper {
    fn prepare() {}
}

struct Runner {}

impl Runner {}

#[async_trait]
pub trait Backend {
    type Image;

    async fn prepare(&self, config: ContainerConfig) -> Result<Self::Image>;

    async fn spawn(
        &self,
        container: Self::Image,
        stdin: Stdio,
        stdout: Stdio,
        stderr: Stdio,
    ) -> Result<()>;
}

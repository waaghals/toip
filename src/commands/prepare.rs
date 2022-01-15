use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, IoSlice};
use std::os::unix::io::FromRawFd;
use std::os::unix::net::{SocketAncillary, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{self, Stdio};
use std::{env, fs};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use itertools::join;
use rand::distributions::Alphanumeric;
use rand::{self, Rng};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::commands::call::call;
use crate::commands::run::run;
use crate::config;
use crate::image::manager::ImageManager;
use crate::oci::runtime::{OciCliRuntime, Runtime};
use crate::runtime::generator::{RunGenerator, RuntimeBundleGenerator};
use crate::serve::Serve;

pub async fn prepare(_container: Option<String>) -> Result<()> {
    let config = config::from_current_dir()?;
    let image_manager = ImageManager::new().context("could not construct `ImageManager`")?;
    for container in config.containers() {
        image_manager.prepare(&container.image).await?;
    }

    Ok(())
}

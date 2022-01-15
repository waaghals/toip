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

use crate::image::manager::ImageManager;
use crate::oci::runtime::{OciCliRuntime, Runtime};
use crate::runtime::generator::{RunGenerator, RuntimeBundleGenerator};
use crate::serve::Serve;
use crate::CallInfo;

pub fn call<S, C, A>(socket_path: S, alias: C, args: A) -> Result<()>
where
    S: AsRef<Path>,
    C: Into<String>,
    A: IntoIterator<Item = String>,
{
    let call_info = CallInfo {
        name: alias.into(),
        arguments: args.into_iter().collect(),
        envargs: HashMap::new(),
    };

    let socket_path = socket_path.as_ref();

    let json =
        serde_json::to_string(&call_info).context("could not serialize call info to json")?;
    let data = json.as_bytes();
    let size = data.len() as u32;

    let socket = UnixStream::connect(&socket_path)
        .with_context(|| format!("could not connect to socket `{}`", socket_path.display()))?;

    let buf1 = size.to_be_bytes();
    let bufs = &[IoSlice::new(&buf1), IoSlice::new(data)][..];
    let fds = [0, 1, 2];
    let mut ancillary_buffer = [0; 128];
    let mut ancillary = SocketAncillary::new(&mut ancillary_buffer[..]);
    ancillary.add_fds(&fds[..]);
    log::debug!(
        "sending ancillary information over socket `{:#?}` with file descriptors `{}`",
        &socket_path,
        join(fds, ", ")
    );
    socket
        .send_vectored_with_ancillary(bufs, &mut ancillary)
        .with_context(|| {
            format!(
                "could not send ancillary data to socket `{}`",
                socket_path.display()
            )
        })?;

    Ok(())
}

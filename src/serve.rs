use std::collections::HashMap;
use std::future::Future;
use std::io::IoSliceMut;
use std::os::unix::net::{AncillaryData, SocketAncillary, UnixStream};
use std::os::unix::prelude::RawFd;
use std::path::PathBuf;
use std::str;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::{FutureExt, StreamExt};
use itertools::join;
use serde_derive::{Deserialize, Serialize};
use tokio::net::UnixListener;
use tokio::select;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::UnixListenerStream;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Serialize, Deserialize)]
pub struct Call {
    pub info: CallInfo,
    pub file_descriptors: Vec<RawFd>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CallInfo {
    pub name: String,
    pub arguments: Vec<String>,
    pub envargs: HashMap<String, String>,
}

struct Inner {
    sender: Sender<Call>,
}

impl Inner {
    // Handle a connection, read the sent file descriptors and read the send call instructions
    async fn handle(&self, stream: UnixStream) -> Result<()> {
        log::info!("handling incoming connection");
        // TODO implement bidirectional communication.
        // Host should communicate the inherited envvars so the client only send
        // the env vars needed, limiting the exposure of envvars
        let mut buf1 = [0; 4];
        let mut buf2 = [0; 1024];
        let bufs = &mut [IoSliceMut::new(&mut buf1), IoSliceMut::new(&mut buf2)][..];
        let mut ancillary_buffer = [0; 128];
        let mut ancillary = SocketAncillary::new(&mut ancillary_buffer[..]);
        let _size = stream.recv_vectored_with_ancillary(bufs, &mut ancillary)?;
        let message_size = u32::from_be_bytes([buf1[0], buf1[1], buf1[2], buf1[3]]) as usize;
        if message_size >= 1024 {
            panic!("Message size to large for single buffer"); // TODO allow arbitrary buffer size
        }

        let info: CallInfo = serde_json::from_slice(&buf2[0..message_size])?;

        let mut file_descriptors = vec![];
        for ancillary_result in ancillary.messages() {
            if let AncillaryData::ScmRights(scm_rights) = ancillary_result.unwrap() {
                for fd in scm_rights {
                    file_descriptors.push(fd);
                }
            }
        }
        log::info!(
            "received call for `{}`, with file descriptors `{}`",
            info.name,
            join(&file_descriptors, ", ")
        );

        self.sender
            .send(Call {
                info,
                file_descriptors,
            })
            .await?;

        Ok(())
    }
}

pub struct Serve {
    cancellation_token: CancellationToken,
    socket_path: PathBuf,
    inner: Arc<Inner>,
}

impl Serve {
    pub fn new<S>(
        cancellation_token: CancellationToken,
        socket_path: S,
        sender: Sender<Call>,
    ) -> Self
    where
        S: Into<PathBuf>,
    {
        Self {
            cancellation_token,
            socket_path: socket_path.into(),
            inner: Arc::new(Inner { sender }),
        }
    }

    pub async fn listen(&self) -> Result<()> {
        let path = self.socket_path.to_string_lossy();
        log::info!("listening on `{}`", path);
        let listener = UnixListener::bind(&self.socket_path)
            .with_context(|| format!("could not listen on socket `{}`", path))?;

        let mut unix_stream = UnixListenerStream::new(listener);
        let cancellation_token = &self.cancellation_token;
        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => break,
                Some(incoming) = unix_stream.next() => {
                    let stream = incoming?;

                    let inner = self.inner.clone();
                    log::trace!("accepted incoming connection");

                    let std_stream = stream
                        .into_std()
                        .context("could not convert Tokio's UnixStream to std's UnixStream")?;
                    inner.handle(std_stream).await?;
                },
                else => break,
            }
        }

        log::info!(
            "stopped listening on `{}`",
            self.socket_path.to_string_lossy()
        );
        Ok(())
    }
}

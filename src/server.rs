use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::os::unix::prelude::RawFd;
use std::path::Path;
use std::str;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use itertools::join;
use serde_derive::{Deserialize, Serialize};
use tokio::net::UnixListener;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::UnixListenerStream;
use tokio_util::sync::CancellationToken;
use uds::UnixStreamExt;

#[derive(Debug, Serialize, Deserialize)]
pub struct Call {
    pub info: CallInfo,
    pub file_descriptors: [RawFd; 3],
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

        let mut data = [0; 1024];
        let mut file_descriptors = [0; 3];
        stream.recv_fds(&mut data, &mut file_descriptors)?;

        let message_size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if message_size >= 1024 {
            panic!("Message size to large for single buffer"); // TODO allow arbitrary buffer size
        }

        log::trace!("{}", String::from_utf8_lossy(&data[4..message_size + 4]));
        let info: CallInfo = serde_json::from_slice(&data[4..message_size + 4])?;
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

pub struct Server {
    cancellation_token: CancellationToken,
    listener_stream: UnixListenerStream,
    inner: Arc<Inner>,
}

impl Server {
    pub async fn listen(mut self) -> Result<()> {
        let cancellation_token = &self.cancellation_token;

        loop {
            tokio::select! {
                Some(incoming) = self.listener_stream.next() => {
                    let stream = incoming?;

                    let inner = self.inner.clone();
                    log::trace!("accepted incoming connection");

                    let std_stream = stream
                        .into_std()
                        .context("could not convert Tokio's UnixStream to std's UnixStream")?;
                    inner.handle(std_stream).await.context("could not handle stream")?;
                },
                _ = cancellation_token.cancelled() => break,
                else => break,
            }
        }

        log::info!("stopped listening on call socket");
        Ok(())
    }
}

pub fn create<S>(
    socket_path: S,
    sender: Sender<Call>,
    cancellation_token: CancellationToken,
) -> Result<Server>
where
    S: AsRef<Path>,
{
    let socket_path = socket_path.as_ref();
    let path = socket_path.to_string_lossy();
    log::info!("listening on `{}`", path);
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("could not listen on socket `{}`", path))?;

    let unix_stream = UnixListenerStream::new(listener);

    Ok(Server {
        cancellation_token,
        listener_stream: unix_stream,
        inner: Arc::new(Inner { sender }),
    })
}

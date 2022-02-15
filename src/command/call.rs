use std::collections::HashMap;
use std::io::IoSlice;
use std::os::unix::net::{SocketAncillary, UnixStream};
use std::path::Path;

use anyhow::{Context, Result};
use itertools::join;

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

    // TODO should wait for result here

    Ok(())
}

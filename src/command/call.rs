use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::path::Path;

use anyhow::{Context, Result};
use itertools::join;
use uds::UnixStreamExt;

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
    let socket = UnixStream::connect(&socket_path)
        .with_context(|| format!("could not connect to socket `{}`", socket_path.display()))?;

    let json =
        serde_json::to_string(&call_info).context("could not serialize call info to json")?;
    let payload = json.as_bytes();

    let size = payload.len() as u32;
    let payload_length = size.to_be_bytes();
    let fds = [0, 1, 2];
    log::debug!(
        "sending ancillary information over socket `{:#?}` with file descriptors `{}`",
        &socket_path,
        join(fds, ", ")
    );

    let mut data = Vec::new();
    data.extend(payload_length);
    data.extend(payload);

    socket.send_fds(&data, &fds).with_context(|| {
        format!(
            "could not send ancillary data to socket `{}`",
            socket_path.display()
        )
    })?;
    // TODO should wait for result here

    Ok(())
}

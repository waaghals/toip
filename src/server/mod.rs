use std::sync::Arc;

use anyhow::{Result};
use futures_util::{StreamExt};
use tokio::io;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader};
use tokio::net::UnixStream;
use tokio_stream::Stream;

use crate::config::{Config, ContainerConfig};
use crate::oci::runtime::Runtime;
use crate::runtime::generator::RuntimeBundleGenerator;

mod decode;
mod encode;
pub mod stdio;
mod stream;

struct Inner<R, G> {
    runtime: R,
    generator: G,
    config: Config,
}

pub struct Server<R, G> {
    inner: Arc<Inner<R, G>>,
}

impl<R, G> Server<R, G>
where
    R: Runtime + Sync + Send + 'static,
    G: RuntimeBundleGenerator + Sync + Send + 'static,
{
    pub fn new(runtime: R, generator: G, config: Config) -> Server<R, G> {
        let inner = Inner {
            runtime,
            generator,
            config,
        };
        Server {
            inner: Arc::new(inner),
        }
    }

    pub async fn listen<S>(&self, stream: S)
    where
        S: Stream<Item = std::io::Result<UnixStream>>,
    {
        tokio::pin!(stream);
        while let Some(next) = stream.next().await {
            let inner = self.inner.clone();
            tokio::spawn(async move {
                let socket = next.unwrap();
                let (rd, wr) = io::split(socket);
                if let Err(error) = inner.handle(rd, wr).await {
                    log::error!("received error: {}", error);
                }
            });
        }
    }
}

impl<R, G> Inner<R, G>
where
    R: Runtime + Sync + Send,
    G: RuntimeBundleGenerator + Sync + Send,
{
    async fn handle<Rd, Wr>(&self, read: Rd, write: Wr) -> Result<()>
    where
        Rd: AsyncRead + Unpin,
        Wr: AsyncWrite + Unpin,
    {
        let mut reader = BufReader::new(read);
        let mut name_buf = String::new();
        let mut args_buf = String::new();
        reader.read_line(&mut name_buf).await?;
        log::trace!("received call for container `{}`", name_buf);
        reader.read_line(&mut args_buf).await?;
        log::trace!(
            "received arguments `{}` for container `{}`",
            name_buf,
            args_buf
        );

        let args = args_buf.split_ascii_whitespace().map(|f| f.to_string());

        let container = self.config.get_container_by_name(&name_buf);
        if let Some(config) = container {
            self.spawn(&config, args, reader, write).await?;
        }
        Ok(())
    }

    async fn spawn<I, Rd, Wr>(
        &self,
        config: &ContainerConfig,
        args: I,
        read: Rd,
        write: Wr,
    ) -> Result<()>
    where
        I: IntoIterator<Item = String> + Send,
        Rd: AsyncRead + Unpin,
        Wr: AsyncWrite + Unpin,
    {
        let container_id = "tmp";
        todo!();
        // let bundle = self.generator.build(container_id, config, args).await?;

        // TODO use runtime to start container, with connected stdio

        // let mut child = Command::new("runc")
        //     .arg("run")
        //     .arg("--bundle")
        //     .arg(bundle)
        //     .env_clear()
        //     .spawn()
        //     .with_context(|| format!("could not spawn process for container `{}`", container_id))
        //     .unwrap();

        // let a = FramedWrite::new(write, StdioEncoder::new());
        // let b = FramedWrite::new(write, StdioEncoder::new());

        // let stdout = child.stdout.take().unwrap();
        // let stderr = child.stderr.take().unwrap();
        // let stdout_stream = StdioStream::new(Stdio::Out, stdout);
        // let stderr_stream = StdioStream::new(Stdio::Err, stderr);
        // let mut stdout = LinesStream::new(BufReader::new(stdout).lines());
        // let stderr = LinesStream::new(BufReader::new(stderr).lines());
        // let merged = stdout_stream.merge(stderr_stream);
        // let mut merged =
        //     tokio_stream::StreamExt::merge(stdout_stream, stderr_stream).map(Result::Ok);

        // let mut encoder = FramedWrite::new(write, StdioEncoder::new());
        // let mut decoder = FramedRead::new(read, StdioDecoder::new());
        // self.pipe(encoder).await;
        // encoder.send_all(&mut merged).await?;
        // let mut stdin = child.stdin.take().unwrap();
        // while let Some(data) = decoder.next().await {
        //     if let Ok(data) = data {
        //         stdin.write_all(&data.data).await.unwrap();
        //     }
        // }
        // Ok(())

        // merged.forward(encoder);

        // let mut map = StreamMap::new();
        // map.insert("out", a);

        // while let Some(tuple) = map.next().await.unwrap() {
        //     let (key, value) = tuple;
        //     let value = match key {
        //         "out" => Stdio::Out(value),
        //         "err" => Stdio::Err(value),
        //         _ => unreachable!(),
        //     };
        // }
        // let framed = FramedWrite::new(write, StdioEncoder::new());

        // self.runtime.run(container_id, bundle)
    }

    // async fn pipe(&self, mut sink: impl Sink<StdioData> + Unpin) {
    //     let all_the_things = vec![StdioData::new(Stdio::Out, Bytes::new())];
    //     while let Some(v) = all_the_things.pop() {
    //         sink.send(v)
    //             .await
    //             .unwrap();
    //     }
    // }
}

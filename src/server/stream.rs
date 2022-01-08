use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::Error;
use bytes::Bytes;
use futures_util::{Sink, Stream};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::stdio::Stdio;
use crate::server::stdio::StdioData;

// enum Stdio {
//     In,
//     Out,
//     Err,
// }

// pub struct StdioWrite<W> {
//     inner: W,
//     marker: u8,
//     written: usize,
// }

// impl<W: AsyncWrite> StdioWrite<W> {
//     pub fn new<M: Into<u8>>(inner: W, marker: M) -> Self {
//         let marker = marker.into();

//         StdioWrite {
//             inner,
//             marker,
//             written: 0,
//         }
//     }
// }

// impl<W: AsyncWrite + Unpin> AsyncWrite for StdioWrite<W> {
//     fn poll_write(
//         mut self: Pin<&mut Self>,
//         cx: &mut Context<'_>,
//         buf: &[u8],
//     ) -> Poll<Result<usize>> {
//         let this = &mut *self;

//         loop {
//             let buf_length = buf.len();
//             let buffer = Vec::with_capacity(8usize + buf_length);
//             let buf_length = buf_length.to_be_bytes();
//             let header = [
//                 this.marker,
//                 0u8,
//                 0u8,
//                 0u8,
//                 buf_length[0],
//                 buf_length[1],
//                 buf_length[2],
//                 buf_length[3],
//             ];

//             buffer.extend_from_slice(&header);
//             buffer.extend_from_slice(buf);

//             Pin::new(&mut this.inner).poll_write(cx, &buffer)
//         }
//     }

//     fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
//         Pin::new(&mut self.inner).poll_flush(cx)
//     }

//     fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
//         Pin::new(&mut self.inner).poll_shutdown(cx)
//     }
// }

pub struct StdioStream<R> {
    kind: Stdio,
    inner: R,
}

impl<R> StdioStream<R> {
    pub fn new(kind: Stdio, inner: R) -> Self {
        Self { kind, inner }
    }
}

impl<R> Stream for StdioStream<R>
where
    R: AsyncRead + Unpin,
{
    type Item = StdioData;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buf = vec![];
        let mut read_buf = ReadBuf::new(&mut buf);
        match Pin::new(&mut self.inner).poll_read(cx, &mut read_buf) {
            Poll::Ready(Ok(())) => {
                if read_buf.filled().is_empty() {
                    Poll::Ready(None)
                } else {
                    let bytes = Bytes::copy_from_slice(read_buf.filled());
                    Poll::Ready(Some(StdioData::new(self.kind.clone(), bytes)))
                }
            }
            Poll::Ready(Err(_)) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct StdioSink<W> {
    kind: Stdio,
    inner: W,
}

impl<W> StdioSink<W> {
    pub fn new(kind: Stdio, inner: W) -> Self {
        Self { kind, inner }
    }
}

impl<W> Sink<StdioData> for StdioSink<W>
where
    W: AsyncWrite,
{
    type Error = Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        todo!()
    }

    fn start_send(self: Pin<&mut Self>, item: StdioData) -> Result<(), Self::Error> {
        todo!()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        todo!()
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        todo!()
    }
}

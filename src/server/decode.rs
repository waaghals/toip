use anyhow::Error;
use bytes::BytesMut;
use tokio_util::codec::Decoder;

use super::stdio::{Stdio, StdioData};

#[derive(Debug, Clone)]
enum DecoderState {
    AwaitingHeader,
    AwaitingContent(u8, usize),
}

#[derive(Debug, Clone)]
pub struct StdioDecoder {
    state: DecoderState,
}

impl Decoder for StdioDecoder {
    type Error = Error;
    type Item = StdioData;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match self.state {
                DecoderState::AwaitingHeader => {
                    if src.len() < 8 {
                        log::trace!("waiting for more data to read header");
                        src.reserve(8 - src.len());
                        return Ok(None);
                    }

                    let header = src.split_to(8);
                    let content_length =
                        u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
                    log::debug!(
                        "read header with type `{}` and length `{}`",
                        header[0],
                        content_length
                    );
                    self.state = DecoderState::AwaitingContent(header[0], content_length);
                }
                DecoderState::AwaitingContent(typ, length) => {
                    if src.len() < length {
                        log::trace!("waiting for more data to read content");
                        src.reserve(length - src.len());
                        return Ok(None);
                    } else {
                        log::debug!("reading content");
                        let bytes = src.split_to(length).freeze();
                        let item = match typ {
                            0 => StdioData::new(Stdio::In, bytes),
                            1 => StdioData::new(Stdio::Out, bytes),
                            2 => StdioData::new(Stdio::Err, bytes),
                            _ => unreachable!(),
                        };

                        self.state = DecoderState::AwaitingHeader;
                        return Ok(Some(item));
                    }
                }
            }
        }
    }
}

use anyhow::Error;
use bytes::{BufMut, BytesMut};
use tokio_util::codec::Encoder;

use super::stdio::{Stdio, StdioData};

pub struct StdioEncoder {}

impl StdioEncoder {
    fn header(item: &StdioData) -> [u8; 8] {
        let marker = match item.kind {
            Stdio::Err => 2u8,
            Stdio::Out => 1u8,
            Stdio::In => 0u8,
        };

        let size = item.data.len().to_be_bytes();

        [marker, 0u8, 0u8, 0u8, size[0], size[1], size[2], size[3]]
    }
}

impl Encoder<StdioData> for StdioEncoder {
    type Error = Error;

    fn encode(&mut self, item: StdioData, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let header = StdioEncoder::header(&item);
        let frame_length = item.data.len() + header.len();
        dst.reserve(frame_length);
        dst.put(&header[..]);
        dst.put(item.data);
        Ok(())
    }
}

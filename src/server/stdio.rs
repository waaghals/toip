use bytes::Bytes;

#[derive(Debug, Clone)]
pub enum Stdio {
    In,
    Out,
    Err,
}

pub struct StdioData {
    pub kind: Stdio,
    pub data: Bytes,
}

impl StdioData {
    pub fn new(kind: Stdio, data: Bytes) -> Self {
        Self { kind, data }
    }
}

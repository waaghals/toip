use anyhow::anyhow;
use bytes::Bytes;
use sha2::{digest::Digest as sha2Digest, Sha256, Sha512};
use std::fmt;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Algorithm {
    SHA256,
    SHA512,
}

impl fmt::Display for Algorithm {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub trait Digest {
    fn algorithm(&self) -> &Algorithm;
    fn digest(&self) -> &str;
    fn data(&self) -> &Bytes;
}

pub trait Verify {
    fn verify(&self) -> anyhow::Result<()>;
}

impl<T> Verify for T
where
    T: Digest,
{
    fn verify(&self) -> anyhow::Result<()> {
        let calculated = match &self.algorithm() {
            Algorithm::SHA256 => Sha256::digest(&self.bytes()),
            Algorithm::SHA512 => Sha512::digest(&self.bytes()),
        };
        let hex = format!("{:x}", calculated);

        if self.digest() == hex {
            return Ok(());
        }

        Err(anyhow!(
            "Digest `{}` is not equal to the calculated value `{}`.",
            self.digest(),
            hex
        ))
    }
}

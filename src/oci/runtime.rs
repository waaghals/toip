use super::image::{Config as ImageConfig, Image};
use anyhow::{Context, Result};
use serde_derive::{Deserialize, Serialize};
use std::fmt;
use std::{
    ffi::OsStr,
    fmt::Display,
    io::{Read, Write},
    path::PathBuf,
    process::{ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
};


pub trait Runtime {
    fn run<C>(&self, config: &Image) -> Result<()>
    where
        C: AsRef<OsStr> + fmt::Display + fmt::Debug;
}

pub struct OciCliRuntime<S>
where
    S: AsRef<OsStr> + Display,
{
    program: S, // runc, crun, youki, conmon
}

impl Default for OciCliRuntime<&str> {
    fn default() -> Self {
        OciCliRuntime { program: "runc" }
    }
}

impl<S> OciCliRuntime<S>
where
    S: AsRef<OsStr> + Display,
{
    pub fn new(program: S) -> Self {
        OciCliRuntime { program }
    }

    fn run<C, A>(&self, command: &'static str, container_id: &C, argument: Option<&A>) -> Result<()>
    where
        C: AsRef<OsStr> + fmt::Display + fmt::Debug,
        A: AsRef<OsStr>,
    {
        let mut child = Command::new(&self.program);
        child
            .arg(command)
            .arg(container_id)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        if let Some(argument) = argument {
            child.arg(argument);
        }

        child.spawn().with_context(|| {
            format!(
                "could not spawn process `{} {} {}`",
                self.program, command, container_id
            )
        })?;

        Ok(())
    }

    fn state<C>(&self, container_id: &C) -> Result<()>
    where
        C: AsRef<OsStr> + fmt::Display + fmt::Debug,
    {
        self.run("state", container_id, None::<&String>)
    }

    fn create<C>(&self, container_id: &C, bundle: &PathBuf) -> Result<()>
    where
        C: AsRef<OsStr> + fmt::Display + fmt::Debug,
    {
        self.run("create", container_id, Some(bundle))
    }

    fn start<C>(&self, container_id: &C) -> Result<()>
    where
        C: AsRef<OsStr> + fmt::Display + fmt::Debug,
    {
        self.run("start", container_id, None::<&String>)
    }

    fn kill<C>(&self, container_id: &C, signal: u8) -> Result<()>
    where
        C: AsRef<OsStr> + fmt::Display + fmt::Debug,
    {
        let string_signal = signal.to_string();
        self.run("kill", container_id, Some(&string_signal))
    }

    fn delete<C>(&self, container_id: &C) -> Result<()>
    where
        C: AsRef<OsStr> + fmt::Display + fmt::Debug,
    {
        self.run("delete", container_id, None::<&String>)
    }
}

impl<S> Runtime for OciCliRuntime<S>
where
    S: AsRef<OsStr> + Display,
{
    fn run<C>(&self, config: &Image) -> Result<()>
    where
        C: AsRef<OsStr> + fmt::Display + fmt::Debug,
    {
        todo!()
    }
}

#[derive(Debug)]
struct YoukiRuntime {}

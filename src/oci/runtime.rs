use anyhow::{Context, Result};
use std::fmt;
use std::{
    ffi::OsStr,
    fmt::Display,
    process::{Command, Stdio},
};

pub trait Runtime {
    fn run<C, B>(&self, container_id: C, bundle: B) -> Result<()>
    where
        B: AsRef<OsStr> + fmt::Debug,
        C: AsRef<OsStr> + fmt::Debug;
}

pub struct OciCliRuntime<S>
where
    S: AsRef<OsStr> + Display,
{
    program: S, // runc, crun, youki, conmoncd ../
}

impl Default for OciCliRuntime<&str> {
    fn default() -> Self {
        OciCliRuntime::new("runc")
    }
}

impl<S> OciCliRuntime<S>
where
    S: AsRef<OsStr> + Display,
{
    pub fn new(program: S) -> Self {
        OciCliRuntime { program }
    }

    fn exec<C, A>(&self, command: &'static str, container_id: &C, bundle: Option<&A>) -> Result<()>
    where
        C: AsRef<OsStr> + fmt::Debug,
        A: AsRef<OsStr>,
    {
        let mut child = Command::new(&self.program);
        child
            .arg(command)
            .arg(container_id)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        if let Some(bundle) = bundle {
            child.arg("--bundle");
            child.arg(bundle);
        }

        child
            .output()
            .with_context(|| format!("could not spawn process `{:?}`", child))?;

        Ok(())
    }
}

impl<S> Runtime for OciCliRuntime<S>
where
    S: AsRef<OsStr> + Display,
{
    fn run<C, B>(&self, container_id: C, bundle: B) -> Result<()>
    where
        C: AsRef<OsStr> + fmt::Debug,
        B: AsRef<OsStr> + fmt::Debug,
    {
        self.exec("run", &container_id, Some(&bundle))
    }
}

#[derive(Debug)]
struct YoukiRuntime {}

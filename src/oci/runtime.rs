use std::ffi::OsStr;
use std::fmt;
use std::fmt::Display;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
pub trait Runtime {
    fn run<C, B, I, O, E>(
        &self,
        container_id: C,
        bundle: B,
        stdin: I,
        stdout: O,
        stderr: E,
    ) -> Result<()>
    where
        B: AsRef<OsStr> + fmt::Debug,
        C: AsRef<OsStr> + fmt::Debug,
        I: Into<Stdio>,
        O: Into<Stdio>,
        E: Into<Stdio>;
}

#[derive(Debug, Clone)]
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
}

impl<S> Runtime for OciCliRuntime<S>
where
    S: AsRef<OsStr> + Display,
{
    fn run<C, B, I, O, E>(
        &self,
        container_id: C,
        bundle: B,
        stdin: I,
        stdout: O,
        stderr: E,
    ) -> Result<()>
    where
        B: AsRef<OsStr> + fmt::Debug,
        C: AsRef<OsStr> + fmt::Debug,
        I: Into<Stdio>,
        O: Into<Stdio>,
        E: Into<Stdio>,
    {
        let mut child = Command::new(&self.program);
        child
            .arg("run")
            .arg(&container_id)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr);

        // if let Some(bundle) = bundle {
        child.arg("--bundle");
        child.arg(&bundle);
        // }

        let mut spawned = child
            .spawn()
            .with_context(|| format!("could not spawn process `{:?}`", child))?;
        let pid = spawned.id();
        log::info!(
            "spawned process with `{} run {:?} --bundle {:?}` with pid `{}`",
            self.program,
            container_id,
            bundle,
            pid
        );

        let status = spawned.wait()?;
        log::info!("processes `{}` exited with status {:?}", pid, status);

        Ok(())
    }
}

#[derive(Debug)]
struct YoukiRuntime {}

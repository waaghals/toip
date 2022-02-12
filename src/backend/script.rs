use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result};

fn create<D, B>(destination: D, binary: B, command: &str, argument: &str) -> Result<()>
where
    D: AsRef<Path>,
    B: AsRef<Path>,
{
    let binary = binary.as_ref();
    let destination = destination.as_ref();
    let script = format!("#!{} {}\n{}\n", binary.display(), command, argument);

    let mut file = File::create(&destination)
        .with_context(|| format!("could not create file `{}`", destination.display()))?;

    file.write_all(script.as_bytes())
        .with_context(|| format!("could not write to file `{}`", destination.display()))?;

    let mut permissions = fs::metadata(&destination)
        .with_context(|| {
            format!(
                "could not read metadata for file `{}`",
                destination.display()
            )
        })?
        .permissions();
    permissions.set_mode(0o744);

    fs::set_permissions(&destination, permissions).with_context(|| {
        format!(
            "could not apply permissions `{}` to file `{}`",
            744,
            destination.display()
        )
    })?;

    Ok(())
}

pub fn create_call<D, B>(destination: D, binary: B, target: &str) -> Result<()>
where
    D: AsRef<Path>,
    B: AsRef<Path>,
{
    create(destination, binary, "call", target)
}

pub fn create_run<D, B>(destination: D, binary: B, target: &str) -> Result<()>
where
    D: AsRef<Path>,
    B: AsRef<Path>,
{
    create(destination, binary, "run", target)
}

pub fn read_container<P>(path: P) -> Result<String>
where
    P: AsRef<Path>,
{
    let file = File::open(&path)
        .with_context(|| format!("could not open file `{}`", path.as_ref().display()))?;

    let container_name = BufReader::new(file).lines().last().with_context(|| {
        format!(
            "could not read container name from file `{}`",
            path.as_ref().display()
        )
    })??;

    Ok(container_name)
}

use std::env;

use anyhow::Result;
use itertools::Itertools;

use crate::cli::Shell;
use crate::dirs;

fn print_bash_compatible(export_path: bool, auto_install: bool, auto_prepare: bool) -> Result<()> {
    if export_path {
        let path = dirs::path()?;
        println!("export PATH={}:$PATH", path.display());
    }

    let mut calls = Vec::new();
    let current_exe = env::current_exe()?;
    let current_exe = current_exe.display();
    if auto_install {
        calls.push(format!("{} install --ignore-missing", &current_exe));
    }
    if auto_prepare {
        calls.push(format!("{} prepare --ignore-missing", &current_exe));
    }

    if !calls.is_empty() {
        print!(
            r##"
function _toip_hook {{
  if [[ "$PREVPWD" != "$PWD" ]]; then
{}
  fi
  # refresh last working dir record
  export PREVPWD="$PWD"
}}

# add `;` after _toip_hook if PROMPT_COMMAND is not empty
export PROMPT_COMMAND="_toip_hook${{PROMPT_COMMAND:+;$PROMPT_COMMAND}}"
"##,
            calls.iter().map(|l| format!("    {}", l)).join("\n")
        );
    }

    Ok(())
}

fn print_fish(export_path: bool, auto_install: bool, auto_prepare: bool) -> Result<()> {
    if export_path {
        let path = dirs::path()?;
        println!("set PATH {} $PATH", path.display());
    }

    let mut calls = Vec::new();
    let current_exe = env::current_exe()?;
    let current_exe = current_exe.display();
    if auto_install {
        calls.push(format!("{} install --ignore-missing", &current_exe));
    }
    if auto_prepare {
        calls.push(format!("{} prepare --ignore-missing", &current_exe));
    }

    if !calls.is_empty() {
        print!(
            r##"
function _toip_hook --on-variable PWD {{
{}
}}
"##,
            calls.iter().map(|l| format!("    {}", l)).join("\n")
        );
    }

    Ok(())
}

pub fn inject(shell: Shell) -> Result<()> {
    match shell {
        Shell::Bash { delegate } | Shell::Zsh { delegate } => print_bash_compatible(
            delegate.export_path,
            delegate.auto_install,
            delegate.auto_prepare,
        ),
        Shell::Fish { delegate } => print_fish(
            delegate.export_path,
            delegate.auto_install,
            delegate.auto_prepare,
        ),
    }
}

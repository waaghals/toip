use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;

#[derive(Parser, Debug)]
#[clap(version, author, about)]
#[clap(propagate_version = true)]
pub struct Cli {
    #[clap(flatten)]
    pub verbose: Verbosity,

    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Arguments {
    #[clap(external_subcommand)]
    Arguments(Vec<String>),
}

#[derive(Debug, PartialEq, Subcommand)]
#[clap(infer_subcommands = true)]
pub enum Command {
    /// Install the configured containers
    Install {
        /// Ignore missing configuration file
        #[clap(short, long)]
        ignore_missing: bool,
    },

    /// Add the current configured container into the shell
    Inject {
        // shell injection script to generate
        #[clap(subcommand)]
        shell: Shell,
    },

    Debug {},
    /// Build and or pull containers
    Prepare {
        /// Container name
        #[clap(short, long)]
        container: Option<String>,

        /// Ignore missing configuration file
        #[clap(short, long)]
        ignore_missing: bool,
    },

    /// Run a container
    Run {
        /// Configuration script
        #[clap(parse(from_os_str))]
        script: PathBuf,
        /// Argument to call the container with
        #[clap(subcommand)]
        args: Option<Arguments>,
    },

    /// Run a linked container from another container
    Call {
        /// Configuration script
        #[clap(parse(from_os_str))]
        script: PathBuf,
        /// Argument to call the container with
        #[clap(subcommand)]
        args: Option<Arguments>,
    },

    /// Remove cache and/or containers
    Clean {
        /// Remove containers
        #[clap(short, long)]
        containers: bool,
    },
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Shell {
    /// Configuration for bash
    ///
    /// Add the following to ~/.bashrc
    ///
    ///    source <(toip inject bash [options])
    ///
    /// For example, to configure the $PATH variable
    /// and to automatically install to containers;
    /// add the following
    ///
    ///    source <(toip inject bash --export-path --auto-install)
    #[clap(verbatim_doc_comment)]
    Bash {
        #[clap(flatten)]
        delegate: InjectShell,
    },

    /// Configuration for fish
    ///
    /// Add the following to ~/.config/fish/config.fish
    ///
    ///    source (toip inject fish [options] | psub)
    ///
    /// For example, to configure the $PATH variable
    /// and to automatically install to containers;
    /// add the following
    ///
    ///    source (toip inject fish --export-path --auto-install | psub)
    Fish {
        #[clap(flatten)]
        delegate: InjectShell,
    },

    /// Configuration for zsh
    ///
    /// Add the following to ~/.zshrc
    ///
    ///    source <(toip inject zsh [options])
    ///
    /// For example, to configure the $PATH variable
    /// and to automatically install to containers;
    /// add the following
    ///
    ///    source <(toip inject zsh --export-path --auto-install)
    Zsh {
        #[clap(flatten)]
        delegate: InjectShell,
    },
}
#[derive(Debug, PartialEq, Args)]
pub struct InjectShell {
    /// Modify path variable
    #[clap(short, long)]
    pub export_path: bool,

    /// Automatically install when changing directory
    #[clap(short = 'i', long)]
    pub auto_install: bool,

    /// Automatically pull and/or build images when changing directory (not recommended)
    #[clap(short = 'p', long)]
    pub auto_prepare: bool,
}

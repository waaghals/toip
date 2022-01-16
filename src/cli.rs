use std::path::PathBuf;

use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;

#[derive(Parser, Debug)]
#[clap(version, author, about)]
pub struct Cli {
    #[clap(flatten)]
    pub verbose: Verbosity,

    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Add the current configured aliases into the shell
    Inject {
        // shell injection script to generate
        #[clap(subcommand)]
        shell: Shell,
    },

    /// Build and or pull containers
    Prepare {
        /// Container name
        #[clap(short, long)]
        container: Option<String>,
    },

    /// Run a container for a given alias
    Run {
        /// Alias to run
        alias: String,
        /// Argument to call the container with
        args: Vec<String>,
    },

    /// Run a linked container
    Call {
        #[clap(parse(from_os_str))]
        file_path: PathBuf,
        /// Argument to call the container with
        args: Vec<String>,
    },

    /// Remove cache and/or containers
    Clean {
        /// Remove containers
        #[clap(short, long)]
        containers: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum Shell {
    /// Configuration for bash
    Bash {
        #[clap(short, long)]
        export_path: bool,
    },

    /// Configuration for fish
    Fish {
        #[clap(short, long)]
        export_path: bool,
    },

    /// Configuration for zsh
    Zsh {
        #[clap(short, long)]
        export_path: bool,
    },
}

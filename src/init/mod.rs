use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use nix::errno::Errno;
use nix::libc::{pid_t, STDIN_FILENO};
use nix::sys::signal::{kill, SigSet, Signal};
use nix::sys::signalfd::SignalFd;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{getpgrp, setpgid, tcsetpgrp, Pid};

fn convert_to_signal(v: u32) -> Result<Signal> {
    match v as i32 {
        x if x == Signal::SIGHUP as i32 => Ok(Signal::SIGHUP),
        x if x == Signal::SIGINT as i32 => Ok(Signal::SIGINT),
        x if x == Signal::SIGQUIT as i32 => Ok(Signal::SIGQUIT),
        x if x == Signal::SIGILL as i32 => Ok(Signal::SIGILL),
        x if x == Signal::SIGTRAP as i32 => Ok(Signal::SIGTRAP),
        x if x == Signal::SIGABRT as i32 => Ok(Signal::SIGABRT),
        x if x == Signal::SIGBUS as i32 => Ok(Signal::SIGBUS),
        x if x == Signal::SIGFPE as i32 => Ok(Signal::SIGFPE),
        x if x == Signal::SIGKILL as i32 => Ok(Signal::SIGKILL),
        x if x == Signal::SIGUSR1 as i32 => Ok(Signal::SIGUSR1),
        x if x == Signal::SIGSEGV as i32 => Ok(Signal::SIGSEGV),
        x if x == Signal::SIGUSR2 as i32 => Ok(Signal::SIGUSR2),
        x if x == Signal::SIGPIPE as i32 => Ok(Signal::SIGPIPE),
        x if x == Signal::SIGALRM as i32 => Ok(Signal::SIGALRM),
        x if x == Signal::SIGTERM as i32 => Ok(Signal::SIGTERM),
        x if x == Signal::SIGSTKFLT as i32 => Ok(Signal::SIGSTKFLT),
        x if x == Signal::SIGCHLD as i32 => Ok(Signal::SIGCHLD),
        x if x == Signal::SIGCONT as i32 => Ok(Signal::SIGCONT),
        x if x == Signal::SIGSTOP as i32 => Ok(Signal::SIGSTOP),
        x if x == Signal::SIGTSTP as i32 => Ok(Signal::SIGTSTP),
        x if x == Signal::SIGTTIN as i32 => Ok(Signal::SIGTTIN),
        x if x == Signal::SIGTTOU as i32 => Ok(Signal::SIGTTOU),
        x if x == Signal::SIGURG as i32 => Ok(Signal::SIGURG),
        x if x == Signal::SIGXCPU as i32 => Ok(Signal::SIGXCPU),
        x if x == Signal::SIGXFSZ as i32 => Ok(Signal::SIGXFSZ),
        x if x == Signal::SIGVTALRM as i32 => Ok(Signal::SIGVTALRM),
        x if x == Signal::SIGPROF as i32 => Ok(Signal::SIGPROF),
        x if x == Signal::SIGWINCH as i32 => Ok(Signal::SIGWINCH),
        x if x == Signal::SIGIO as i32 => Ok(Signal::SIGIO),
        x if x == Signal::SIGPWR as i32 => Ok(Signal::SIGPWR),
        x if x == Signal::SIGSYS as i32 => Ok(Signal::SIGSYS),
        _ => Err(anyhow!(
            "Could not convert signal {} to nix::sys::signal::Signal",
            v
        )),
    }
}

fn reap_zombies() -> Result<Vec<Pid>> {
    let mut zombies = Vec::new();

    loop {
        match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(pid, _)) | Ok(WaitStatus::Signaled(pid, _, _)) => {
                zombies.push(pid)
            }
            Ok(WaitStatus::StillAlive) => break,

            status @ Ok(_) => log::error!("Received unknown status: `{:?}`", status),
            Err(err) => {
                return Err(anyhow!(
                    "Received unexpected error while waiting for status: {}",
                    err
                ))
            }
        }
    }

    Ok(zombies)
}

fn handle_signals(pid: Pid, sfd: &mut SignalFd) -> Result<Vec<Pid>> {
    let signal_option = sfd.read_signal().context("Could not read signal")?;
    let signal_info = signal_option.ok_or_else(|| anyhow!("Received no signal"))?;
    let signal = convert_to_signal(signal_info.ssi_signo)?;
    match signal {
        Signal::SIGCHLD => reap_zombies(),
        _ => {
            kill(pid, signal)?;
            Ok(Vec::new())
        }
    }
}

fn make_foreground() -> Result<()> {
    // Create a new process group.
    let zero_pid = Pid::from_raw(0);
    setpgid(zero_pid, zero_pid)?;
    let parent_group_pid = getpgrp();

    // Open /dev/tty, to avoid issues of std{in,out,err} being duped.
    let tty = match File::open("/dev/tty") {
        Ok(tty) => tty.as_raw_fd(),
        Err(err) => {
            log::debug!("Could not open /dev/tty: {}", err);
            STDIN_FILENO
        }
    };

    // We have to block SIGTTOU here otherwise we will get stopped if we are in
    // a background process group.
    let mut sigmask = SigSet::empty();
    sigmask.add(Signal::SIGTTOU);
    sigmask.thread_block()?;

    // Set ourselves to be the foreground process group in our session.
    match tcsetpgrp(tty.as_raw_fd(), parent_group_pid) {
        Ok(_) => Ok(()),

        Err(Errno::ENOTTY) | Err(Errno::EBADF) => {
            log::debug!("Setting foreground process failed because there is no tty present.");
            Ok(())
        }
        Err(Errno::ENXIO) => {
            log::debug!("Setting foreground process failed because there is no such device.");
            Ok(())
        }

        Err(err) => Err(anyhow!(
            "Error while bringing process to foreground: {}",
            err
        )),
    }
}

// TODO add mounted volumes to PATH
pub fn spawn(cmd: String, args: Vec<String>) -> Result<()> {
    let init_sigmask = SigSet::thread_get_mask().expect("could not get main thread sigmask");

    let sigmask = SigSet::all();
    sigmask.thread_block().expect("could not block all signals");
    let mut sfd = SignalFd::new(&sigmask).expect("could not create signalfd for all signals");

    log::debug!("Spawning cmd `{}` with arguments `{:#?}`", cmd, args);
    let child = unsafe {
        Command::new(&cmd).args(&args).pre_exec(move || {
            make_foreground().unwrap();
            init_sigmask.thread_set_mask()?;
            Ok(())
        })
    }
    .spawn()
    .with_context(|| format!("Could not spawn child `{}`", cmd))?;

    let pid = Pid::from_raw(child.id() as pid_t);
    log::info!("Spawned child process `{}`", pid);

    loop {
        match handle_signals(pid, &mut sfd) {
            // Do not stop handling signals in case of an event.
            // Always keep the process running as we are the parent for the whole container
            Err(err) => log::error!("unexpected error during signal handling: {}", err),
            Ok(pids) => {
                if pids.contains(&pid) {
                    break;
                }
            }
        };
    }
    Ok(())
}

use anyhow::{Context, Result};
use nix::fcntl::{OFlag, open};
use nix::sys::stat::Mode;
use nix::unistd::{ForkResult, chdir, fork, setsid};
use std::fs;
use std::os::fd::AsRawFd;

pub fn daemonize(nochdir: bool, noclose: bool) -> Result<()> {
    match unsafe { fork() } {
        Ok(ForkResult::Parent { .. }) => {
            std::process::exit(0);
        }
        Ok(ForkResult::Child) => {}
        Err(e) => return Err(anyhow::anyhow!("form failed: {e}")),
    }

    setsid().context("setsid failed")?;

    if !nochdir {
        chdir("/").context("chadir / failed")?;
    }

    if !noclose {
        let devnull =
            open("/dev/null", OFlag::O_RDWR, Mode::empty()).context("open /dev/null failed")?;

        let ret0 = unsafe { libc::dup2(devnull.as_raw_fd(), 0) };
        if ret0 == -1 {
            return Err(anyhow::anyhow!("dup2 stdin failed"));
        }

        let ret1 = unsafe { libc::dup2(devnull.as_raw_fd(), 1) };
        if ret1 == -1 {
            return Err(anyhow::anyhow!("dup2 stdin failed"));
        }

        let ret2 = unsafe { libc::dup2(devnull.as_raw_fd(), 2) };
        if ret2 == -1 {
            return Err(anyhow::anyhow!("dup2 stdin failed"));
        }

        drop(devnull);
    }

    Ok(())
}

pub fn save_pid(path: &str) -> Result<()> {
    let pid = std::process::id();
    let tmp = format!("{path}.tmp");

    fs::write(&tmp, format!("{pid}\n"))
        .with_context(|| format!("failed to write pid file {tmp}"))?;
    fs::rename(&tmp, path).with_context(|| format!("failed to rename {tmp} to {path}"))?;

    Ok(())
}

pub fn remove_pid(path: &str) -> Result<()> {
    fs::remove_file(path).with_context(|| format!("failed to remove pid file {path}"))?;
    Ok(())
}

use serde::Deserialize;
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::{fs::OpenOptionsExt, fs::PermissionsExt, io::AsRawFd, net::UnixStream},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
pub struct Start {
    pub args: Vec<String>,
    pub state_dir: Option<PathBuf>,
}

extern "C" {
    fn dup2(old: i32, new: i32) -> i32;
}

pub fn connect(dir: &Path) -> super::Result<(Start, tokio::net::UnixStream)> {
    let metadata = fs::symlink_metadata(dir)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("CLI session must be a private directory".into());
    }
    let start = serde_json::from_slice(&fs::read(dir.join("start.json"))?)?;
    let stream = UnixStream::connect(dir.join("control.sock"))?;
    for output in [1, 2] {
        if unsafe { dup2(stream.as_raw_fd(), output) } == -1 {
            return Err(io::Error::last_os_error().into());
        }
    }
    stream.set_nonblocking(true)?;
    Ok((start, tokio::net::UnixStream::from_std(stream)?))
}

pub fn finish(dir: &Path, success: bool) -> super::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("exit"))?;
    file.write_all(if success { b"0" } else { b"1" })?;
    Ok(())
}

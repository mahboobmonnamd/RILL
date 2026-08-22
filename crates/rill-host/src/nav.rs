//! Cold `.nav` socket: snapshot and NEW_TAB (ADR 0056, SPEC-NAV).

use crate::Error;
use rill_attach::cold_nav_socket_path;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavSnapshot {
    pub workspace_id: u64,
    pub tabs: Vec<u64>,
    pub leaves: Vec<u64>,
}

pub fn parse_nav_line(line: &str) -> Option<NavSnapshot> {
    let line = line.trim();
    let mut workspace_id = None;
    let mut tabs = Vec::new();
    let mut leaves = Vec::new();
    for part in line.split_whitespace() {
        if let Some(v) = part.strip_prefix("ws=") {
            workspace_id = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("tabs=") {
            tabs = parse_id_list(v);
        }
        if let Some(v) = part.strip_prefix("leaves=") {
            leaves = parse_id_list(v);
        }
    }
    Some(NavSnapshot {
        workspace_id: workspace_id?,
        tabs,
        leaves,
    })
}

fn parse_id_list(s: &str) -> Vec<u64> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split(',').filter_map(|p| p.parse().ok()).collect()
}

fn read_line(stream: &mut UnixStream) -> Result<String, Error> {
    let mut buf = [0u8; 512];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Err(Error::Dead);
    }
    Ok(String::from_utf8_lossy(&buf[..n]).into_owned())
}

pub fn nav_snapshot(attach: &Path) -> Result<NavSnapshot, Error> {
    let mut stream = UnixStream::connect(cold_nav_socket_path(attach))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let line = read_line(&mut stream)?;
    parse_nav_line(&line).ok_or(Error::InvalidHostIdentity)
}

pub fn nav_new_tab(attach: &Path) -> Result<NavSnapshot, Error> {
    if std::env::var("RILL_MUTATE").as_deref() == Ok("chrome_invents_tab") {
        return Err(Error::Dead);
    }
    let mut stream = UnixStream::connect(cold_nav_socket_path(attach))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let first = read_line(&mut stream)?;
    let _ = parse_nav_line(&first).ok_or(Error::InvalidHostIdentity)?;
    stream.write_all(b"NEW_TAB\n")?;
    let second = read_line(&mut stream)?;
    let line = second
        .lines()
        .find(|l| l.contains("tabs="))
        .unwrap_or(second.trim());
    parse_nav_line(line).ok_or(Error::InvalidHostIdentity)
}

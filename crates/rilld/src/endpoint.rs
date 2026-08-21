//! Protected runtime directory and local peer credentials.
//!
//! SPEC-ATTACH §1, SPEC-RUNTIME-SUPERVISION §6.

use crate::error::Error;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;

pub fn default_runtime_dir() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("RILL_RUNTIME_DIR") {
        return std::path::PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/var/empty".into());
    #[cfg(target_os = "macos")]
    {
        std::path::PathBuf::from(home).join("Library/Application Support/RILL/run")
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
            return std::path::PathBuf::from(xdg).join("rill");
        }
        std::path::PathBuf::from(home).join(".local/share/rill/run")
    }
}

pub fn ensure_protected_parent(socket: &Path) -> Result<(), Error> {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("skip_runtime_parent_check") {
        if let Some(dir) = socket.parent() {
            std::fs::create_dir_all(dir)?;
        }
        return Ok(());
    }
    let dir = socket.parent().ok_or(Error::UnprotectedEndpoint)?;
    std::fs::create_dir_all(dir)?;
    let meta = std::fs::metadata(dir)?;
    let mode = meta.permissions().mode();
    let uid = unsafe { libc::getuid() };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.uid() != uid {
            return Err(Error::UnprotectedEndpoint);
        }
    }
    if mode & 0o002 != 0 {
        return Err(Error::UnprotectedEndpoint);
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub fn peer_uid(stream: &UnixStream) -> Result<u32, Error> {
    if let Ok(fake) = std::env::var("RILL_TEST_FAKE_PEER_UID") {
        if let Ok(n) = fake.parse::<u32>() {
            return Ok(n);
        }
    }
    let fd = stream.as_raw_fd();
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    #[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "openbsd"))]
    {
        if unsafe { libc::getpeereid(fd, &mut uid, &mut gid) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let _ = gid;
        Ok(uid)
    }
    #[cfg(target_os = "linux")]
    {
        let mut cred = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut len = std::mem::size_of_val(&cred) as libc::socklen_t;
        let rc = unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                &mut cred as *mut _ as *mut libc::c_void,
                &mut len,
            )
        };
        if rc != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let _ = gid;
        Ok(cred.uid)
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "linux"
    )))]
    {
        let _ = (fd, uid, gid);
        Err(Error::UnprotectedEndpoint)
    }
}

pub fn authorize_peer(stream: &UnixStream) -> Result<(), Error> {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("peer_cred_always_ok") {
        return Ok(());
    }
    let got = peer_uid(stream)?;
    let want = unsafe { libc::getuid() };
    if got != want {
        return Err(Error::PeerRefused);
    }
    Ok(())
}

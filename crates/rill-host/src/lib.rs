//! GUI attach client. Does not spawn the user shell. Does not own a PTY.

use rill_attach::{Decoder, Frame};
use rill_chip0::{load_host_surface, Chip0, HostSurface, PodGrid, TerminalEmulation};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct Client {
    stream: UnixStream,
    decoder: Decoder,
    chip: Chip0,
    surface: HostSurface,
    alive: bool,
    saw_json: bool,
    last_paint: Instant,
}

impl Client {
    pub fn connect(socket: impl AsRef<Path>, surface: HostSurface) -> Result<Self, Box<dyn std::error::Error>> {
        let stream = UnixStream::connect(socket.as_ref())?;
        stream.set_nonblocking(true)?;
        let chip = Chip0::new(surface.cols, surface.rows)?;
        let mut client = Self {
            stream,
            decoder: Decoder::new(),
            chip,
            surface,
            alive: true,
            saw_json: false,
            last_paint: Instant::now(),
        };
        client.send(Frame::Attach { generation: 1 })?;
        client.send(Frame::Credit(u32::MAX))?;
        Ok(client)
    }

    pub fn font_family(&self) -> &str {
        &self.surface.font_family
    }

    pub fn font_size(&self) -> f32 {
        self.surface.font_size
    }

    pub fn send_input(&mut self, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        if !self.alive {
            return Err("pane is dead".into());
        }
        self.last_paint = Instant::now();
        self.send(Frame::Data(bytes.to_vec()))
    }

    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        px_w: u16,
        px_h: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.chip.resize(cols, rows, 8, 16)?;
        self.send(Frame::Resize {
            cols,
            rows,
            px_w,
            px_h,
        })
    }

    pub fn pump(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        let mut buf = [0u8; 65536];
        match self.stream.read(&mut buf) {
            Ok(0) => Ok(false),
            Ok(n) => {
                if buf[..n].contains(&b'{') && looks_like_json(&buf[..n]) {
                    self.saw_json = true;
                }
                let frames = self.decoder.push(&buf[..n])?;
                for frame in frames {
                    match frame {
                        Frame::Data(bytes) => {
                            self.chip.feed(&bytes)?;
                            self.last_paint = Instant::now();
                        }
                        Frame::Exit { .. } => {
                            self.alive = false;
                        }
                        Frame::Refused { .. } => {
                            return Err("attach refused".into());
                        }
                        _ => {}
                    }
                }
                let _ = self.send(Frame::Credit(65536));
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(true),
            Err(e) => Err(e.into()),
        }
    }

    pub fn snapshot(&mut self) -> Result<PodGrid, rill_chip0::Error> {
        self.chip.snapshot()
    }

    pub fn alive(&self) -> bool {
        self.alive
    }

    pub fn saw_control_rpc(&self) -> bool {
        self.saw_json
    }

    fn send(&mut self, frame: Frame) -> Result<(), Box<dyn std::error::Error>> {
        let bytes = frame.encode()?;
        // Blocking write of a complete frame; keys are small.
        self.stream.set_nonblocking(false)?;
        self.stream.write_all(&bytes)?;
        self.stream.set_nonblocking(true)?;
        Ok(())
    }
}

fn looks_like_json(buf: &[u8]) -> bool {
    let s = String::from_utf8_lossy(buf);
    s.contains("pane_replay") || s.contains("\"cells\"")
}

pub fn default_socket() -> PathBuf {
    if let Ok(p) = std::env::var("RILL_SOCKET") {
        return PathBuf::from(p);
    }
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/rill-{uid}.sock"))
}

pub fn load_surface() -> Result<HostSurface, rill_chip0::Error> {
    let mut paths = vec![PathBuf::from("host-surface.toml")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join("../Resources/host-surface.toml"));
            paths.push(dir.join("host-surface.toml"));
        }
    }
    paths.push(PathBuf::from("../../host-surface.toml"));
    for p in &paths {
        if p.exists() {
            return load_host_surface(p);
        }
    }
    load_host_surface("host-surface.toml")
}

/// NFR-KEY: key-down → first POD snapshot containing the glyph. No control RPC.
pub fn nfr_key(client: &mut Client, count: u32) -> Result<NfrReport, Box<dyn std::error::Error>> {
    let mut samples = Vec::with_capacity(count as usize);
    client.pump()?;
    for i in 0..count {
        let ch = b'a' + (i % 26) as u8;
        let t0 = Instant::now();
        client.send_input(&[ch])?;
        let deadline = Instant::now() + Duration::from_millis(100);
        let mut seen = false;
        while Instant::now() < deadline {
            let _ = client.pump()?;
            if let Ok(grid) = client.snapshot() {
                if grid.cells.iter().any(|c| c.codepoint == u32::from(ch)) {
                    seen = true;
                    break;
                }
            }
        }
        if !seen {
            return Err("glyph did not paint".into());
        }
        samples.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((samples.len() as f64) * 0.95).ceil() as usize;
    let p95 = samples[idx.min(samples.len() - 1)];
    Ok(NfrReport {
        p95_ms: p95,
        count: samples.len() as u32,
        control_rpc: client.saw_control_rpc(),
        on_battery: on_battery(),
    })
}

#[derive(Debug)]
pub struct NfrReport {
    pub p95_ms: f64,
    pub count: u32,
    pub control_rpc: bool,
    pub on_battery: bool,
}

fn on_battery() -> bool {
    let out = std::process::Command::new("pmset")
        .args(["-g", "batt"])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains("Battery Power"),
        Err(_) => false,
    }
}

mod ffi;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_nfr_key_paints_without_control_rpc() {
        if std::env::var("RILL_REQUIRE_NFR").is_err() {
            eprintln!("skipping T-NFR in-process: set RILL_REQUIRE_NFR=1 with a dedicated socket");
            return;
        }
        let socket = default_socket();
        if UnixStream::connect(&socket).is_err() {
            panic!("T-NFR: rilld not running at {}", socket.display());
        }
        let surface = load_surface().expect("surface");
        let mut client = Client::connect(&socket, surface).expect("connect");
        for _ in 0..20 {
            let _ = client.pump();
        }
        let report = nfr_key(&mut client, 1000).expect("nfr");
        assert!(!report.control_rpc, "control RPC on the warm path");
        assert!(
            report.p95_ms < 16.7,
            "p95 {}ms missed one-frame budget",
            report.p95_ms
        );
        if std::env::var("RILL_REQUIRE_BATTERY").ok().as_deref() == Some("1") {
            assert!(report.on_battery, "NFR-KEY must run on battery");
        }
    }
}

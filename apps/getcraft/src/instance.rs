//! Keeps GetCraft to a single running copy. The first instance listens on a localhost port
//! (recorded in a file); a second launch connects, asks it to show its window, and exits.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

const REQUEST: &[u8] = b"getcraft:show\n";
const REPLY: &str = "getcraft:ok";

pub enum Instance {
    /// We're the first copy; the listener (if one could be opened) receives "show" requests
    /// from later launches.
    Primary(Option<TcpListener>),
    /// Another copy is running and has been asked to show itself.
    AlreadyRunning,
}

fn port_file(dir: &Path) -> PathBuf {
    dir.join("instance.port")
}

pub fn acquire(dir: &Path) -> Instance {
    if let Some(port) = std::fs::read_to_string(port_file(dir)).ok().and_then(|p| p.trim().parse::<u16>().ok())
        && ask_to_show(port)
    {
        return Instance::AlreadyRunning;
    }
    match TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) {
        Ok(listener) => {
            if let Ok(addr) = listener.local_addr() {
                let _ = std::fs::create_dir_all(dir);
                let _ = std::fs::write(port_file(dir), addr.port().to_string());
            }
            Instance::Primary(Some(listener))
        }
        // Without a listener later launches can't reach us, which is tolerable.
        Err(e) => {
            log::warn!("single-instance listener unavailable: {e}");
            Instance::Primary(None)
        }
    }
}

/// True only if a GetCraft instance answered; a stale port now used by something else won't.
fn ask_to_show(port: u16) -> bool {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(300)) else { return false };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    if stream.write_all(REQUEST).is_err() {
        return false;
    }
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).is_ok() && reply.trim() == REPLY
}

/// Calls `on_show` for every valid request, on a background thread.
pub fn serve(listener: TcpListener, on_show: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let mut line = String::new();
            let mut reader = BufReader::new(&stream);
            if reader.read_line(&mut line).is_ok() && line.as_bytes() == REQUEST {
                let _ = (&stream).write_all(format!("{REPLY}\n").as_bytes());
                on_show();
            }
        }
    });
}

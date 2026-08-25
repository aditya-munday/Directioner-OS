//! Unix-socket JSON-lines server for ai-capabilityd.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::handler::Handler;
use crate::{CapabilityRequest, CapabilityResponse};

pub type SharedHandler = Arc<Mutex<Handler>>;

pub struct Server {
    listener: UnixListener,
    handler: SharedHandler,
}

impl Server {
    pub fn bind(path: impl AsRef<Path>, handler: SharedHandler) -> std::io::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        Ok(Self { listener, handler })
    }

    pub fn serve(&self) -> std::io::Result<()> {
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    if let Err(e) = handle_conn(stream, &self.handler) {
                        eprintln!("connection error: {e}");
                    }
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
        Ok(())
    }
}

fn handle_conn(mut stream: UnixStream, handler: &SharedHandler) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let response = process_line(line, handler);
        let mut out = serde_json::to_vec(&response).map_err(std::io::Error::other)?;
        out.push(b'\n');
        stream.write_all(&out)?;
        stream.flush()?;
    }
    Ok(())
}

fn process_line(line: &str, handler: &SharedHandler) -> CapabilityResponse {
    let req: CapabilityRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return CapabilityResponse::Error {
                request_id: "?".into(),
                message: format!("malformed request: {e}"),
                audit_seq: 0,
            };
        }
    };
    if req.api_version != crate::API_VERSION {
        return CapabilityResponse::Denied {
            request_id: req.id.clone(),
            reason: format!(
                "unsupported api_version {}, expected {}",
                req.api_version,
                crate::API_VERSION
            ),
            audit_seq: 0,
        };
    }
    match handler.lock().execute(&req) {
        Ok((_seq, resp)) => resp,
        Err(e) => CapabilityResponse::Error {
            request_id: req.id.clone(),
            message: e.to_string(),
            audit_seq: 0,
        },
    }
}

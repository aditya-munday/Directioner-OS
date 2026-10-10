//! Unix-socket JSON-lines server for ai-capabilityd.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::handler::Handler;
use crate::{CapabilityRequest, CapabilityResponse};

/// Thread-safe reference-counted pointer to the Capability [`Handler`].
pub type SharedHandler = Arc<Mutex<Handler>>;

/// Default maximum request payload size: 64 KiB (TODO 001).
pub const DEFAULT_MAX_REQUEST_BYTES: usize = 64 * 1024;
/// Default per-connection read timeout: 5 seconds (TODO 001).
pub const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// Default maximum concurrent active connections: 64 (TODO 001).
pub const DEFAULT_MAX_CONCURRENT_CONNECTIONS: usize = 64;

/// Configuration settings for the Capability [`Server`].
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Authorized caller UIDs allowed to connect to the Unix domain socket.
    pub allowed_uids: Vec<u32>,
    /// Maximum allowed request frame size in bytes before allocation rejection (default 64 KiB).
    pub max_request_bytes: usize,
    /// Per-connection read timeout (default 5 seconds).
    pub read_timeout: std::time::Duration,
    /// Maximum concurrent active connections handled by the server (default 64).
    pub max_concurrent_connections: usize,
}

impl ServerConfig {
    /// Parses a comma-separated string of UIDs and appends unique parsed values to `uids`.
    pub fn parse_uids_into(raw: &str, uids: &mut Vec<u32>) {
        for part in raw.split(',') {
            if let Ok(uid) = part.trim().parse::<u32>()
                && !uids.contains(&uid)
            {
                uids.push(uid);
            }
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        let current_uid = rustix::process::getuid().as_raw();
        let mut uids = vec![current_uid];
        if current_uid != 0 {
            uids.push(0);
        }
        if let Ok(env_uids) = std::env::var("DIRECTIONER_ALLOWED_UIDS") {
            Self::parse_uids_into(&env_uids, &mut uids);
        }
        Self {
            allowed_uids: uids,
            max_request_bytes: DEFAULT_MAX_REQUEST_BYTES,
            read_timeout: DEFAULT_READ_TIMEOUT,
            max_concurrent_connections: DEFAULT_MAX_CONCURRENT_CONNECTIONS,
        }
    }
}

/// Unix domain socket server dispatching capability requests to the [`Handler`].
pub struct Server {
    listener: UnixListener,
    handler: SharedHandler,
    config: ServerConfig,
    active_connections: Arc<std::sync::atomic::AtomicUsize>,
}

impl Server {
    /// Returns the count of currently active in-flight connections.
    pub fn active_connections(&self) -> usize {
        self.active_connections
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Binds a Unix domain socket at the specified path and initializes the server with default configuration.
    pub fn bind(path: impl AsRef<Path>, handler: SharedHandler) -> std::io::Result<Self> {
        Self::bind_with_config(path, handler, ServerConfig::default())
    }

    /// Binds a Unix domain socket at the specified path and initializes the server with explicit configuration.
    pub fn bind_with_config(
        path: impl AsRef<Path>,
        handler: SharedHandler,
        config: ServerConfig,
    ) -> std::io::Result<Self> {
        use std::os::unix::fs::PermissionsExt;

        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            listener,
            handler,
            config,
            active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }

    /// Listens for incoming Unix socket connections and handles requests concurrently up to configured limits.
    pub fn serve(&self) -> std::io::Result<()> {
        for stream in self.listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let active = self
                        .active_connections
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if active >= self.config.max_concurrent_connections {
                        self.active_connections
                            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                        let err_resp = CapabilityResponse::Error {
                            request_id: "conn-limit".into(),
                            message: format!(
                                "server connection limit of {} exceeded",
                                self.config.max_concurrent_connections
                            ),
                            audit_seq: 0,
                        };
                        if let Ok(mut out) = serde_json::to_vec(&err_resp) {
                            out.push(b'\n');
                            let _ = stream.write_all(&out);
                            let _ = stream.flush();
                        }
                        let _ = stream.shutdown(std::net::Shutdown::Both);
                        continue;
                    }

                    let handler = self.handler.clone();
                    let config = self.config.clone();
                    let counter = self.active_connections.clone();

                    std::thread::spawn(move || {
                        struct ConnGuard(Arc<std::sync::atomic::AtomicUsize>);
                        impl Drop for ConnGuard {
                            fn drop(&mut self) {
                                self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                            }
                        }
                        let _guard = ConnGuard(counter);
                        if let Err(e) = handle_conn(stream, &handler, &config) {
                            eprintln!("connection error: {e}");
                        }
                    });
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
        Ok(())
    }
}

/// Errors encountered while reading framed requests over the socket wire.
#[derive(Debug)]
pub enum ReadFrameError {
    /// Underlying I/O error.
    Io(std::io::Error),
    /// Request exceeded the configured wire maximum size before newline delimiter.
    PayloadTooLarge {
        /// Maximum allowed payload size.
        max_bytes: usize,
    },
    /// Per-connection read timeout elapsed.
    Timeout,
}

fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    line: &mut String,
    max_bytes: usize,
) -> Result<usize, ReadFrameError> {
    line.clear();
    let mut total_read = 0;
    loop {
        let available = match reader.fill_buf() {
            Ok(buf) => buf,
            Err(e)
                if e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                return Err(ReadFrameError::Timeout);
            }
            Err(e) => return Err(ReadFrameError::Io(e)),
        };

        if available.is_empty() {
            return Ok(total_read);
        }

        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            let chunk_len = pos + 1;
            if total_read + chunk_len > max_bytes {
                reader.consume(chunk_len);
                return Err(ReadFrameError::PayloadTooLarge { max_bytes });
            }
            let chunk_str = std::str::from_utf8(&available[..chunk_len]).map_err(|e| {
                ReadFrameError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })?;
            line.push_str(chunk_str);
            reader.consume(chunk_len);
            total_read += chunk_len;
            return Ok(total_read);
        } else {
            let chunk_len = available.len();
            if total_read + chunk_len > max_bytes {
                reader.consume(chunk_len);
                return Err(ReadFrameError::PayloadTooLarge { max_bytes });
            }
            let chunk_str = std::str::from_utf8(available).map_err(|e| {
                ReadFrameError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })?;
            line.push_str(chunk_str);
            reader.consume(chunk_len);
            total_read += chunk_len;
        }
    }
}

fn handle_conn(
    mut stream: UnixStream,
    handler: &SharedHandler,
    config: &ServerConfig,
) -> std::io::Result<()> {
    // AUDIT-CAP-02: Enforce caller UID verification via SO_PEERCRED
    let peer_uid = match rustix::net::sockopt::socket_peercred(&stream) {
        Ok(ucred) => ucred.uid.as_raw(),
        Err(e) => {
            eprintln!("failed to query SO_PEERCRED on incoming connection: {e}");
            let err_resp = CapabilityResponse::Denied {
                request_id: "auth".into(),
                reason: format!("unable to verify caller identity via SO_PEERCRED: {e}"),
                audit_seq: 0,
            };
            let mut out = serde_json::to_vec(&err_resp).map_err(std::io::Error::other)?;
            out.push(b'\n');
            let _ = stream.write_all(&out);
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Both);
            return Ok(());
        }
    };

    if !config.allowed_uids.contains(&peer_uid) {
        eprintln!(
            "rejecting connection: caller UID {peer_uid} not in allowed_uids {:?}",
            config.allowed_uids
        );
        let denied_resp = CapabilityResponse::Denied {
            request_id: "auth".into(),
            reason: format!(
                "caller UID {peer_uid} is not authorized to connect to ai-capabilityd (allowed: {:?})",
                config.allowed_uids
            ),
            audit_seq: 0,
        };
        let mut out = serde_json::to_vec(&denied_resp).map_err(std::io::Error::other)?;
        out.push(b'\n');
        let _ = stream.write_all(&out);
        let _ = stream.flush();
        // Drain unread bytes so kernel doesn't issue ECONNRESET when socket is closed
        let _ = stream.set_nonblocking(true);
        let mut drain = [0u8; 1024];
        while let Ok(n) = stream.read(&mut drain) {
            if n == 0 {
                break;
            }
        }
        let _ = stream.shutdown(std::net::Shutdown::Both);
        return Ok(());
    }

    // Set per-connection read timeout to mitigate slowloris attacks
    if let Err(e) = stream.set_read_timeout(Some(config.read_timeout)) {
        eprintln!("warning: failed to set stream read timeout: {e}");
    }

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    loop {
        match read_bounded_line(&mut reader, &mut line, config.max_request_bytes) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let response = process_line(trimmed, handler);
                let mut out = serde_json::to_vec(&response).map_err(std::io::Error::other)?;
                out.push(b'\n');
                stream.write_all(&out)?;
                stream.flush()?;
            }
            Err(ReadFrameError::PayloadTooLarge { max_bytes }) => {
                let err_resp = CapabilityResponse::Error {
                    request_id: "?".into(),
                    message: format!(
                        "request payload exceeds maximum allowed size of {max_bytes} bytes"
                    ),
                    audit_seq: 0,
                };
                let mut out = serde_json::to_vec(&err_resp).map_err(std::io::Error::other)?;
                out.push(b'\n');
                let _ = stream.write_all(&out);
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
                return Ok(());
            }
            Err(ReadFrameError::Timeout) => {
                let err_resp = CapabilityResponse::Error {
                    request_id: "?".into(),
                    message: format!("connection read timed out after {:?}", config.read_timeout),
                    audit_seq: 0,
                };
                let mut out = serde_json::to_vec(&err_resp).map_err(std::io::Error::other)?;
                out.push(b'\n');
                let _ = stream.write_all(&out);
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
                return Ok(());
            }
            Err(ReadFrameError::Io(e)) => {
                return Err(e);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_parse_uids_into() {
        let mut uids = vec![0];
        ServerConfig::parse_uids_into("1000, 1001, invalid, 1002, 1000", &mut uids);
        assert_eq!(uids, vec![0, 1000, 1001, 1002]);
    }

    #[test]
    fn test_server_config_default_contains_current_and_root() {
        let config = ServerConfig::default();
        let current_uid = rustix::process::getuid().as_raw();
        assert!(config.allowed_uids.contains(&current_uid));
        assert!(config.allowed_uids.contains(&0));
    }
}

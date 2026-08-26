//! Unix-socket IPC client for the Capability Layer (`ai-capabilityd`).
//!
//! Exposes a synchronous / blocking client that handles token generation,
//! newline-delimited JSON framing, and typed response decoding over a domain socket.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use capability_layer::{API_VERSION, CapabilityKind, CapabilityRequest, CapabilityResponse};

/// Error communicating with `ai-capabilityd`.
#[derive(Debug)]
pub enum ClientError {
    /// Failed to connect to daemon Unix socket.
    Connect(std::io::Error),
    /// Failed sending request or reading response.
    Io(std::io::Error),
    /// Failed to serialize request or deserialize response JSON.
    Json(serde_json::Error),
    /// Request was rejected or timed out.
    Protocol(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Connect(e) => write!(f, "cannot connect to ai-capabilityd socket: {e}"),
            ClientError::Io(e) => write!(f, "I/O error communicating with ai-capabilityd: {e}"),
            ClientError::Json(e) => write!(f, "JSON serialization error: {e}"),
            ClientError::Protocol(msg) => write!(f, "protocol error: {msg}"),
        }
    }
}

impl std::error::Error for ClientError {}

/// Client that communicates with `ai-capabilityd` over a Unix domain socket.
pub struct CapabilityClient {
    socket_path: PathBuf,
    requester: String,
    timeout: Duration,
}

impl CapabilityClient {
    /// Create a new client pointing to the given daemon socket.
    pub fn new(socket_path: impl Into<PathBuf>, requester: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
            requester: requester.into(),
            timeout: Duration::from_secs(10),
        }
    }

    /// Set a custom I/O timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Return the path to the socket.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Dispatch a single typed capability to `ai-capabilityd` and wait for response.
    /// Dispatch a single typed capability to `ai-capabilityd` and wait for response.
    pub fn send(
        &self,
        capability: CapabilityKind,
        correlation_id: impl Into<String>,
        token: Option<String>,
    ) -> Result<CapabilityResponse, ClientError> {
        self.send_ext(capability, correlation_id, token, false)
    }

    /// Dispatch a capability designated specifically as a compensating rollback.
    pub fn send_rollback(
        &self,
        capability: CapabilityKind,
        correlation_id: impl Into<String>,
        token: Option<String>,
    ) -> Result<CapabilityResponse, ClientError> {
        self.send_ext(capability, correlation_id, token, true)
    }

    /// Dispatch a capability with explicit rollback flag.
    pub fn send_ext(
        &self,
        capability: CapabilityKind,
        correlation_id: impl Into<String>,
        token: Option<String>,
        is_rollback: bool,
    ) -> Result<CapabilityResponse, ClientError> {
        let req_id = correlation_id.into();
        let tok = token.unwrap_or_else(|| format!("tok-{}", req_id));

        let request = CapabilityRequest {
            api_version: API_VERSION,
            id: req_id,
            requester: self.requester.clone(),
            token: tok,
            is_rollback,
            capability,
        };

        let mut stream = UnixStream::connect(&self.socket_path).map_err(ClientError::Connect)?;
        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(ClientError::Io)?;
        stream
            .set_write_timeout(Some(self.timeout))
            .map_err(ClientError::Io)?;

        let mut payload = serde_json::to_string(&request).map_err(ClientError::Json)?;
        payload.push('\n');

        stream
            .write_all(payload.as_bytes())
            .map_err(ClientError::Io)?;
        stream.flush().map_err(ClientError::Io)?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).map_err(ClientError::Io)?;

        if bytes_read == 0 {
            return Err(ClientError::Protocol(
                "connection closed by ai-capabilityd without response".into(),
            ));
        }

        let resp: CapabilityResponse =
            serde_json::from_str(line.trim()).map_err(ClientError::Json)?;
        Ok(resp)
    }

    /// Perform a health check probe (`ping`).
    pub fn ping(&self) -> Result<bool, ClientError> {
        match self.send(CapabilityKind::Ping, "ping-probe", None)? {
            CapabilityResponse::Success { .. } => Ok(true),
            CapabilityResponse::Denied { .. } | CapabilityResponse::Error { .. } => Ok(false),
        }
    }
}

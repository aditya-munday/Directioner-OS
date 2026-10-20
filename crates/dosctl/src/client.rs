use directioner_actd::{ActRequest, ActResponse};
use directioner_statd::{StatRequest, StatResponse};
use std::env;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// IPC Client for communicating with Directioner Fabric Daemons (U079)
pub struct FabricClient;

impl FabricClient {
    /// Return the statd socket path
    pub fn statd_socket_path() -> PathBuf {
        env::var("DIRECTIONER_STATD_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/run/directioner/statd.sock"))
    }

    /// Return the actd socket path
    pub fn actd_socket_path() -> PathBuf {
        let runtime = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(runtime).join("directioner/actd.sock")
    }

    /// Query state tree or specific node from directioner-statd
    pub async fn query_statd(req: &StatRequest) -> Result<StatResponse, String> {
        let sock_path = Self::statd_socket_path();
        if !sock_path.exists() {
            return Err(format!(
                "directioner-statd daemon socket not found at '{}'",
                sock_path.display()
            ));
        }

        let stream = UnixStream::connect(&sock_path)
            .await
            .map_err(|e| format!("Failed to connect to statd: {e}"))?;

        let (reader, mut writer) = stream.into_split();
        let mut req_str = serde_json::to_string(req).map_err(|e| e.to_string())?;
        req_str.push('\n');
        writer
            .write_all(req_str.as_bytes())
            .await
            .map_err(|e| e.to_string())?;

        let mut lines = BufReader::new(reader).lines();
        let resp_line = lines
            .next_line()
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "EOF from statd".to_string())?;

        serde_json::from_str::<StatResponse>(&resp_line).map_err(|e| e.to_string())
    }

    /// Send action request to directioner-actd (preview, execute, undo)
    pub async fn request_actd(req: &ActRequest) -> Result<ActResponse, String> {
        let sock_path = Self::actd_socket_path();
        if !sock_path.exists() {
            return Err(format!(
                "directioner-actd daemon socket not found at '{}'",
                sock_path.display()
            ));
        }

        let stream = UnixStream::connect(&sock_path)
            .await
            .map_err(|e| format!("Failed to connect to actd: {e}"))?;

        let (reader, mut writer) = stream.into_split();
        let mut req_str = serde_json::to_string(req).map_err(|e| e.to_string())?;
        req_str.push('\n');
        writer
            .write_all(req_str.as_bytes())
            .await
            .map_err(|e| e.to_string())?;

        let mut lines = BufReader::new(reader).lines();
        let resp_line = lines
            .next_line()
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "EOF from actd".to_string())?;

        serde_json::from_str::<ActResponse>(&resp_line).map_err(|e| e.to_string())
    }
}

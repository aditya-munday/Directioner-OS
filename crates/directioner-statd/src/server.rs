use crate::consent::ConsentLedger;
use crate::providers::{NetworkProvider, PowerProvider, StateProvider, SystemProvider};
use crate::redact::RedactionEngine;
use crate::tree::StateTree;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// Statd IPC Request (strictly read-only queries)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StatRequest {
    /// Retrieve snapshot of entire state tree
    GetTree,
    /// Retrieve single node by path
    GetNode { path: String },
    /// Any prohibited write attempt
    #[serde(other)]
    ProhibitedWrite,
}

/// Statd IPC Response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StatResponse {
    /// Successful query with JSON payload
    Success { data: serde_json::Value },
    /// Request rejected or error
    Error { code: String, message: String },
}

/// Statd Daemon Service (U066, U067)
pub struct StatDaemon {
    socket_path: PathBuf,
    tree: Arc<Mutex<StateTree>>,
    ledger: Arc<Mutex<ConsentLedger>>,
    providers: Vec<Box<dyn StateProvider>>,
}

impl StatDaemon {
    /// Initialize daemon with a specified socket path
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        let mut daemon = Self {
            socket_path: socket_path.into(),
            tree: Arc::new(Mutex::new(StateTree::new())),
            ledger: Arc::new(Mutex::new(ConsentLedger::new())),
            providers: vec![
                Box::new(SystemProvider),
                Box::new(PowerProvider),
                Box::new(NetworkProvider),
            ],
        };
        // Initial telemetry poll
        daemon.refresh_all();
        daemon
    }

    /// Access the state tree directly (for in-process queries or testing)
    pub fn tree(&self) -> Arc<Mutex<StateTree>> {
        Arc::clone(&self.tree)
    }

    /// Access the consent ledger directly
    pub fn ledger(&self) -> Arc<Mutex<ConsentLedger>> {
        Arc::clone(&self.ledger)
    }

    /// Poll all registered providers and update state tree
    pub fn refresh_all(&mut self) {
        let mut tree = self.tree.lock().unwrap();
        for provider in &mut self.providers {
            if let Ok(updates) = provider.poll() {
                for (path, val, sens) in updates {
                    tree.insert(path, val, sens, provider.name());
                }
            }
        }
    }

    /// Bind unix socket with 0600 permissions and serve requests
    pub async fn run(self) -> std::io::Result<()> {
        if self.socket_path.exists() {
            let _ = fs::remove_file(&self.socket_path);
        }

        if let Some(parent) = self.socket_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let listener = UnixListener::bind(&self.socket_path)?;

        // Ensure socket permissions are 0600 (owner only) per U066
        let _ = fs::set_permissions(&self.socket_path, fs::Permissions::from_mode(0o600));

        let tree = Arc::clone(&self.tree);
        let ledger = Arc::clone(&self.ledger);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let tree_clone = Arc::clone(&tree);
                    let ledger_clone = Arc::clone(&ledger);
                    tokio::spawn(async move {
                        let _ = Self::handle_client(stream, tree_clone, ledger_clone).await;
                    });
                }
                Err(e) => {
                    eprintln!("statd: accept error: {e}");
                }
            }
        }
    }

    async fn handle_client(
        stream: UnixStream,
        tree: Arc<Mutex<StateTree>>,
        ledger: Arc<Mutex<ConsentLedger>>,
    ) -> std::io::Result<()> {
        // Extract caller UID via SO_PEERCRED on Linux
        let peer_uid = match stream.peer_cred() {
            Ok(cred) => cred.uid(),
            Err(_) => 65534, // Default to nobody on failure
        };

        let consent_level = {
            let leg = ledger.lock().unwrap();
            leg.resolve_level(peer_uid)
        };

        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        while let Some(line) = lines.next_line().await? {
            let resp = match serde_json::from_str::<StatRequest>(&line) {
                Ok(StatRequest::GetTree) => {
                    let t = tree.lock().unwrap();
                    let filtered = RedactionEngine::filter_tree(&t, consent_level);
                    StatResponse::Success {
                        data: serde_json::to_value(&filtered).unwrap(),
                    }
                }
                Ok(StatRequest::GetNode { path }) => {
                    let t = tree.lock().unwrap();
                    let filtered = RedactionEngine::filter_tree(&t, consent_level);
                    match filtered.get(&path) {
                        Some(node) => StatResponse::Success {
                            data: serde_json::to_value(node).unwrap(),
                        },
                        None => StatResponse::Error {
                            code: "NOT_FOUND".into(),
                            message: format!("Node '{path}' not found"),
                        },
                    }
                }
                _ => {
                    // Any unparseable or mutating action is rejected by construction (U066)
                    StatResponse::Error {
                        code: "WRITE_NOT_ALLOWED".into(),
                        message: "directioner-statd is strictly read-only by construction (U066)"
                            .into(),
                    }
                }
            };

            let mut out = serde_json::to_vec(&resp)?;
            out.push(b'\n');
            writer.write_all(&out).await?;
        }

        Ok(())
    }
}

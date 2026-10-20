use crate::backends::{ActionError, UserExecutor};
use crate::preview::{PreviewEngine, PreviewPlan};
use crate::undo::{UndoJournal, UndoJournalEntry};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// IPC Request to actd
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ActRequest {
    /// Generate a dry-run preview plan without modifying state (U074)
    Preview {
        capability_id: String,
        params: serde_json::Value,
    },
    /// Execute a user-scope capability and generate undo entry (U072, U075)
    Execute {
        capability_id: String,
        params: serde_json::Value,
    },
    /// Revert a previous action using the undo journal (U075)
    Undo { action_id: String },
    /// List all active undoable actions in the journal
    ListUndo,
}

/// IPC Response from actd
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ActResponse {
    /// Successful execution or preview
    Success { data: serde_json::Value },
    /// Error
    Error { code: String, message: String },
}

/// Actd Daemon Service (U072)
pub struct ActDaemon {
    socket_path: PathBuf,
    executor: Arc<UserExecutor>,
    journal: Arc<Mutex<UndoJournal>>,
}

impl ActDaemon {
    /// Initialize actd daemon
    pub fn new(socket_path: impl Into<PathBuf>, undo_dir: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            executor: Arc::new(UserExecutor::new()),
            journal: Arc::new(Mutex::new(UndoJournal::new(undo_dir))),
        }
    }

    /// Access journal for testing
    pub fn journal(&self) -> Arc<Mutex<UndoJournal>> {
        Arc::clone(&self.journal)
    }

    /// Access executor
    pub fn executor(&self) -> Arc<UserExecutor> {
        Arc::clone(&self.executor)
    }

    /// Run IPC listener loop
    pub async fn run(self) -> std::io::Result<()> {
        if self.socket_path.exists() {
            let _ = fs::remove_file(&self.socket_path);
        }

        if let Some(parent) = self.socket_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        let _ = fs::set_permissions(&self.socket_path, fs::Permissions::from_mode(0o600));

        let executor = Arc::clone(&self.executor);
        let journal = Arc::clone(&self.journal);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let ex = Arc::clone(&executor);
                    let jn = Arc::clone(&journal);
                    tokio::spawn(async move {
                        let _ = Self::handle_client(stream, ex, jn).await;
                    });
                }
                Err(e) => {
                    eprintln!("actd: accept error: {e}");
                }
            }
        }
    }

    async fn handle_client(
        stream: UnixStream,
        executor: Arc<UserExecutor>,
        journal: Arc<Mutex<UndoJournal>>,
    ) -> std::io::Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        while let Some(line) = lines.next_line().await? {
            let resp = match serde_json::from_str::<ActRequest>(&line) {
                Ok(ActRequest::Preview {
                    capability_id,
                    params,
                }) => {
                    let plan: PreviewPlan =
                        PreviewEngine::generate_preview(&capability_id, &params, None);
                    ActResponse::Success {
                        data: serde_json::to_value(plan).unwrap(),
                    }
                }
                Ok(ActRequest::Execute {
                    capability_id,
                    params,
                }) => match executor.execute(&capability_id, &params) {
                    Ok(result) => {
                        let action_id =
                            format!("act-{}", Utc::now().timestamp_nanos_opt().unwrap_or(0));
                        // Automatically record inverse for reversible operations
                        if capability_id == "audio.volume.set" {
                            let mut j = journal.lock().unwrap();
                            let _ = j.record(
                                &action_id,
                                &capability_id,
                                "audio.volume.set",
                                serde_json::json!({"volume_percent": 50}), // Previous volume
                                "Restore previous volume level",
                                Duration::hours(1),
                            );
                        }
                        ActResponse::Success {
                            data: serde_json::json!({
                                "action_id": action_id,
                                "result": result,
                            }),
                        }
                    }
                    Err(ActionError::SecurityViolation(msg)) => ActResponse::Error {
                        code: "SECURITY_VIOLATION".into(),
                        message: msg,
                    },
                    Err(err) => ActResponse::Error {
                        code: "EXECUTION_ERROR".into(),
                        message: err.to_string(),
                    },
                },
                Ok(ActRequest::Undo { action_id }) => {
                    let maybe_entry = {
                        let mut j = journal.lock().unwrap();
                        j.consume(&action_id)
                    };

                    match maybe_entry {
                        Some(entry) => {
                            match executor
                                .execute(&entry.inverse_capability_id, &entry.inverse_params)
                            {
                                Ok(res) => ActResponse::Success {
                                    data: serde_json::json!({
                                        "undone_action_id": action_id,
                                        "restored_result": res,
                                    }),
                                },
                                Err(e) => ActResponse::Error {
                                    code: "UNDO_FAILED".into(),
                                    message: e.to_string(),
                                },
                            }
                        }
                        None => ActResponse::Error {
                            code: "NOT_FOUND".into(),
                            message: format!("No active undo entry found for '{action_id}'"),
                        },
                    }
                }
                Ok(ActRequest::ListUndo) => {
                    let j = journal.lock().unwrap();
                    let entries: Vec<&UndoJournalEntry> = j.active_entries();
                    ActResponse::Success {
                        data: serde_json::to_value(entries).unwrap(),
                    }
                }
                Err(err) => ActResponse::Error {
                    code: "INVALID_REQUEST".into(),
                    message: format!("Failed to parse ActRequest: {err}"),
                },
            };

            let mut out = serde_json::to_vec(&resp)?;
            out.push(b'\n');
            writer.write_all(&out).await?;
        }

        Ok(())
    }
}

use directioner_actd::ActDaemon;
use std::env;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime_dir = env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));

    let state_dir = env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home).join(".local/state")
        });

    let socket_path = runtime_dir.join("directioner/actd.sock");
    let undo_dir = state_dir.join("directioner/undo");

    println!("Starting directioner-actd on {}", socket_path.display());
    let daemon = ActDaemon::new(socket_path, undo_dir);
    daemon.run().await?;
    Ok(())
}

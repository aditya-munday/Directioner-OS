use directioner_statd::StatDaemon;
use std::env;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = env::var("DIRECTIONER_STATD_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/run/directioner/statd.sock"));

    println!(
        "Starting directioner-statd (Observe Plane Daemon) on {}",
        socket_path.display()
    );
    let daemon = StatDaemon::new(socket_path);
    daemon.run().await?;
    Ok(())
}

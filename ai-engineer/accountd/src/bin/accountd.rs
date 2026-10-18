use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use accountd::{AccountEngine, AccountRequest, AccountResponse, MockBackend, PlatformConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let socket_path = parse_socket_path(&args);
    let data_dir = parse_data_dir(&args);

    let config = PlatformConfig::default();
    let backend = MockBackend::new();
    let mut engine = AccountEngine::new(&data_dir, config, backend)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if socket_path.exists() {
        let _ = fs::remove_file(&socket_path);
    }

    let listener = UnixListener::bind(&socket_path)?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;

    println!("directioner-accountd listening on {:?}", socket_path);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let _ = handle_client(&mut engine, stream);
            }
            Err(e) => {
                eprintln!("Connection failed: {}", e);
            }
        }
    }

    Ok(())
}

fn handle_client(engine: &mut AccountEngine, mut stream: UnixStream) -> std::io::Result<()> {
    let reader = BufReader::new(stream.try_clone()?);
    let now = chrono::Utc::now().timestamp();
    let caller_uid = 1000;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let resp = match serde_json::from_str::<AccountRequest>(&line) {
            Ok(req) => engine.handle_request(req, caller_uid, now),
            Err(e) => AccountResponse::Error {
                code: "invalid_json".to_string(),
                message: e.to_string(),
            },
        };

        let out = serde_json::to_string(&resp).unwrap_or_default();
        stream.write_all(out.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
    }

    Ok(())
}

fn parse_socket_path(args: &[String]) -> PathBuf {
    for i in 0..args.len() {
        if args[i] == "--socket" && i + 1 < args.len() {
            return PathBuf::from(&args[i + 1]);
        }
    }
    PathBuf::from("/run/directioner/accountd.sock")
}

fn parse_data_dir(args: &[String]) -> PathBuf {
    for i in 0..args.len() {
        if args[i] == "--data-dir" && i + 1 < args.len() {
            return PathBuf::from(&args[i + 1]);
        }
    }
    PathBuf::from("/var/lib/directioner/accountd")
}

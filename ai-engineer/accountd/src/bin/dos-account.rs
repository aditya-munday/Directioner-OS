use accountd::{AccountRequest, AccountResponse, ResponseData};
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let is_json = args.iter().any(|a| a == "--json");
    let cmd = &args[1];

    let socket_path = parse_socket_path(&args);

    let req = match cmd.as_str() {
        "login" => AccountRequest::LoginStartDevice,
        "logout" => AccountRequest::Logout,
        "status" => AccountRequest::Status,
        "whoami" => AccountRequest::Whoami,
        "devices" => AccountRequest::DeviceList,
        "token" => {
            let aud = parse_audience(&args).unwrap_or_else(|| "cli".to_string());
            AccountRequest::TokenGet { audience: aud }
        }
        "export" => AccountRequest::ExportData,
        "delete" => AccountRequest::DeleteAccount,
        "--help" | "-h" | "help" => {
            print_usage();
            return Ok(());
        }
        "--version" | "-v" => {
            println!("dos-account 42.0.0 (Genesis; contract v1)");
            return Ok(());
        }
        unknown => {
            eprintln!("Unknown command: {}", unknown);
            print_usage();
            std::process::exit(1);
        }
    };

    if !socket_path.exists() {
        if is_json {
            println!(
                r#"{{"error":"accountd_unavailable","message":"account daemon socket not found"}}"#
            );
        } else {
            eprintln!(
                "Error: account daemon socket not found at {:?}",
                socket_path
            );
            eprintln!("Ensure directioner-accountd is running.");
        }
        std::process::exit(1);
    }

    let mut stream = UnixStream::connect(&socket_path)?;
    let msg = serde_json::to_string(&req)?;
    stream.write_all(msg.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    if is_json {
        println!("{}", line.trim());
    } else {
        let resp: AccountResponse = serde_json::from_str(&line)?;
        match resp {
            AccountResponse::Ok(data) => format_human_readable(data),
            AccountResponse::Error { code, message } => {
                eprintln!("Error [{}]: {}", code, message);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

fn format_human_readable(data: ResponseData) {
    match data {
        ResponseData::DeviceCodeInitiated {
            user_code,
            verification_uri,
            interval,
            ..
        } => {
            println!("To sign in, open the following URL on any device:");
            println!("  {}", verification_uri);
            println!("Enter this one-time code:");
            println!("  {}", user_code);
            println!("Polling every {} seconds...", interval);
        }
        ResponseData::TokenIssued(t) => {
            println!("Authentication successful.");
            println!("Audience: {}", t.audience);
            println!("Scopes:   {}", t.scopes.join(", "));
            println!("Expires in: {}s", t.expires_in);
        }
        ResponseData::StatusInfo {
            state,
            user,
            device_id,
            plan,
            offline_grace_remaining_secs,
        } => {
            println!("Account State: {:?}", state);
            println!("Device ID:     {}", device_id);
            if let Some(u) = user {
                println!("User ID:       {}", u.user_id);
            }
            if let Some(p) = plan {
                println!("Active Plan:   {}", p);
            }
            if let Some(g) = offline_grace_remaining_secs {
                println!("Offline Grace: {}s remaining", g);
            }
        }
        ResponseData::WhoamiInfo {
            user_id,
            user_hash,
            device_name,
        } => {
            println!("User ID:     {}", user_id);
            println!("User Hash:   {}", user_hash);
            println!("Device Name: {}", device_name);
        }
        ResponseData::DeviceCollection(devices) => {
            println!("Registered Devices ({}):", devices.len());
            for d in devices {
                let status = if d.revoked {
                    "REVOKED"
                } else if d.is_current {
                    "CURRENT"
                } else {
                    "ACTIVE"
                };
                println!(
                    "  - {} ({}, {}) [{}]",
                    d.device_name, d.device_id, d.os_version, status
                );
            }
        }
        ResponseData::EntitlementsInfo(e) => {
            println!("Plan:     {}", e.plan);
            println!("Features: {}", e.features.join(", "));
            println!("Quota:    {} operations remaining", e.quota_remaining);
        }
        ResponseData::LoggedOut => {
            println!("Successfully signed out.");
        }
        ResponseData::DeviceRevoked { device_id } => {
            println!("Device {} successfully revoked.", device_id);
        }
        ResponseData::ExportArchive { json_payload } => {
            println!("{}", json_payload);
        }
        ResponseData::AccountDeleted => {
            println!("Account deletion confirmed.");
        }
    }
}

fn print_usage() {
    println!("Usage: dos-account <command> [options]");
    println!("Commands:");
    println!("  login              Start device authorization login");
    println!("  logout             Sign out and revoke active session");
    println!("  status             Show current account state and grace");
    println!("  whoami             Show logged in user identity");
    println!("  devices            List all registered devices");
    println!("  token              Request scoped token (e.g. --audience cli)");
    println!("  export             Export local account data archive");
    println!("  delete             Request account wipe and revoke");
    println!("Options:");
    println!("  --json             Emit structured JSON output");
    println!("  --socket <path>    Specify accountd socket path");
}

fn parse_socket_path(args: &[String]) -> PathBuf {
    for i in 0..args.len() {
        if args[i] == "--socket" && i + 1 < args.len() {
            return PathBuf::from(&args[i + 1]);
        }
    }
    PathBuf::from("/run/directioner/accountd.sock")
}

fn parse_audience(args: &[String]) -> Option<String> {
    for i in 0..args.len() {
        if args[i] == "--audience" && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
    }
    None
}

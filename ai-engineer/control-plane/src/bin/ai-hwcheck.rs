//! Hardware Compatibility Diagnostic CLI tool for Directioner-OS.
//!
//! Purely read-only diagnostic utility that inspects host hardware sysfs hierarchies
//! (PCI, USB, power, net) and reports driver binding, firmware availability, and
//! compatibility matching against known-good upstream Fedora bootc baseline.
//!
//! Supports `--json` flag for machine-readable output and accessibility tools.

use std::env;
use std::path::PathBuf;
use std::process::exit;

use control_plane::hwcheck::{HwChecker, HwStatus};

fn print_usage() {
    eprintln!(
        "Directioner-OS Hardware Compatibility Diagnostic Tool (ai-hwcheck)

USAGE:
    ai-hwcheck [OPTIONS]

OPTIONS:
    --json              Output structured JSON report (for accessibility/automation)
    --sysfs <PATH>      Custom sysfs root directory path (default: /sys)
    --etc <PATH>        Custom /etc directory path (default: /etc)
    --firmware <PATH>   Custom firmware directory path (default: /lib/firmware)
    -h, --help          Show this help message
"
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut json_mode = false;
    let mut sysfs_root = PathBuf::from("/sys");
    let mut etc_root = PathBuf::from("/etc");
    let mut firmware_root = PathBuf::from("/lib/firmware");

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json_mode = true;
            }
            "--sysfs" => {
                i += 1;
                if i < args.len() {
                    sysfs_root = PathBuf::from(&args[i]);
                } else {
                    eprintln!("Error: --sysfs requires a directory path");
                    exit(1);
                }
            }
            "--etc" => {
                i += 1;
                if i < args.len() {
                    etc_root = PathBuf::from(&args[i]);
                } else {
                    eprintln!("Error: --etc requires a directory path");
                    exit(1);
                }
            }
            "--firmware" => {
                i += 1;
                if i < args.len() {
                    firmware_root = PathBuf::from(&args[i]);
                } else {
                    eprintln!("Error: --firmware requires a directory path");
                    exit(1);
                }
            }
            "-h" | "--help" => {
                print_usage();
                exit(0);
            }
            unknown => {
                eprintln!("Error: unknown option '{}'", unknown);
                print_usage();
                exit(1);
            }
        }
        i += 1;
    }

    let checker = HwChecker::with_roots(sysfs_root, etc_root, firmware_root);
    let report = checker.inspect();

    if json_mode {
        println!("{}", report.render_json());
    } else {
        println!("{}", report.render_human_table());
    }

    if report.overall_status == HwStatus::Unsupported {
        exit(2);
    } else {
        exit(0);
    }
}

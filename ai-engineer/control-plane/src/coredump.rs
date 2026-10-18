use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoredumpRecord {
    pub pid: u64,
    pub uid: u64,
    pub gid: u64,
    pub signal: Option<String>,
    pub exe: String,
    pub unit: Option<String>,
    pub boot_id: Option<String>,
    pub timestamp: String,
    pub backtrace_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoredumpForensicsReport {
    pub generated_at: String,
    pub total_dumps: usize,
    pub records: Vec<CoredumpRecord>,
    pub summary: String,
}

pub fn run_coredumpctl(args: &[&str]) -> Result<String, String> {
    Command::new("coredumpctl")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .map_err(|e| format!("coredumpctl unavailable: {e}"))
}

pub fn list_coredumps() -> CoredumpForensicsReport {
    let now = chrono::Utc::now().to_rfc3339();
    let output = match run_coredumpctl(&["--json=pretty", "list"]) {
        Ok(o) => o,
        Err(e) => {
            return CoredumpForensicsReport {
                generated_at: now,
                total_dumps: 0,
                records: Vec::new(),
                summary: format!("coredumpctl unavailable: {e}"),
            };
        }
    };

    let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    let records: Vec<CoredumpRecord> = parsed
        .iter()
        .map(|v| {
            let ts = v.get("timestamp")
                .or_else(|| v.get("time"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            CoredumpRecord {
                pid: v.get("pid").and_then(|p| p.as_u64()).unwrap_or(0),
                uid: v.get("uid").and_then(|u| u.as_u64()).unwrap_or(0),
                gid: v.get("gid").and_then(|g| g.as_u64()).unwrap_or(0),
                signal: v.get("signal").and_then(|s| s.as_str()).map(|s| s.to_string()),
                exe: v.get("exe").and_then(|e| e.as_str()).unwrap_or("unknown").to_string(),
                unit: v.get("unit").and_then(|u| u.as_str()).map(|s| s.to_string()),
                boot_id: v.get("boot_id").and_then(|b| b.as_str()).map(|s| s.to_string()),
                timestamp: ts,
                backtrace_summary: None,
            }
        })
        .collect();

    let total = records.len();
    let summary = if total == 0 {
        "No coredumps found".to_string()
    } else {
        let exes: Vec<&str> = records.iter().map(|r| r.exe.as_str()).collect();
        format!("{total} coredump(s) found: {}", exes.join(", "))
    };

    CoredumpForensicsReport {
        generated_at: now,
        total_dumps: total,
        records,
        summary,
    }
}

pub fn get_backtrace_summary(pid: u64, _timestamp: &str) -> Option<String> {
    let out = Command::new("coredumpctl")
        .args(["info", &pid.to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut summary_lines = Vec::new();
    for line in text.lines() {
        if line.contains("Signal") || line.contains("Backtrace") || line.trim().starts_with('#') {
            summary_lines.push(line.trim().to_string());
            if summary_lines.len() >= 10 {
                break;
            }
        }
    }
    if summary_lines.is_empty() {
        None
    } else {
        Some(summary_lines.join("\n"))
    }
}

pub fn parse_coredump_from_fixture(fixture_json: &str) -> CoredumpForensicsReport {
    let now = chrono::Utc::now().to_rfc3339();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(fixture_json).unwrap_or_default();
    let records: Vec<CoredumpRecord> = parsed
        .iter()
        .map(|v| CoredumpRecord {
            pid: v.get("pid").and_then(|p| p.as_u64()).unwrap_or(0),
            uid: v.get("uid").and_then(|u| u.as_u64()).unwrap_or(0),
            gid: v.get("gid").and_then(|g| g.as_u64()).unwrap_or(0),
            signal: v.get("signal").and_then(|s| s.as_str()).map(|s| s.to_string()),
            exe: v.get("exe").and_then(|e| e.as_str()).unwrap_or("unknown").to_string(),
            unit: v.get("unit").and_then(|u| u.as_str()).map(|s| s.to_string()),
            boot_id: v.get("boot_id").and_then(|b| b.as_str()).map(|s| s.to_string()),
            timestamp: v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            backtrace_summary: v.get("backtrace_summary").and_then(|b| b.as_str()).map(|s| s.to_string()),
        })
        .collect();

    let total = records.len();
    let summary = if total == 0 {
        "No coredumps in fixture".to_string()
    } else {
        format!("{total} coredump(s) parsed from fixture")
    };

    CoredumpForensicsReport {
        generated_at: now,
        total_dumps: total,
        records,
        summary,
    }
}

pub struct CoredumpForensicsStore {
    path: std::path::PathBuf,
    max_records: usize,
}

impl CoredumpForensicsStore {
    pub fn new(path: std::path::PathBuf, max_records: usize) -> Self {
        Self { path, max_records }
    }

    pub fn save_record(&mut self, record: &CoredumpRecord) -> Result<(), std::io::Error> {
        let mut records = self.load_all()?;
        records.push(record.clone());
        if records.len() > self.max_records {
            let start = records.len() - self.max_records;
            records = records[start..].to_vec();
        }

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&self.path)?;
        for r in &records {
            let line = serde_json::to_string(r).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            use std::io::Write;
            writeln!(file, "{line}")?;
        }
        Ok(())
    }

    pub fn load_all(&self) -> Result<Vec<CoredumpRecord>, std::io::Error> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = std::fs::File::open(&self.path)?;
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;
        let mut records = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(rec) = serde_json::from_str(&line) {
                records.push(rec);
            }
        }
        Ok(records)
    }
}

use serde::{Deserialize, Serialize};
use std::fs;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessMetricsSample {
    pub elapsed_secs: f64,
    pub rss_kb: Option<u64>,
    pub open_fds: Option<u64>,
    pub thread_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoakReport {
    pub pid: u32,
    pub samples: Vec<ProcessMetricsSample>,
    pub duration_secs: f64,
    pub rss_growth_slope: f64,
    pub fd_growth_slope: f64,
    pub leaked: bool,
    pub rss_threshold_kb_per_sec: f64,
    pub fd_threshold_per_sec: f64,
    pub summary: String,
}

impl SoakReport {
    pub fn passed(&self) -> bool {
        !self.leaked
    }
}

pub struct SoakConfig {
    pub pid: u32,
    pub sample_interval: Duration,
    pub total_duration: Duration,
    pub max_rss_slope_kb_per_sec: f64,
    pub max_fd_slope_per_sec: f64,
}

impl SoakConfig {
    pub fn default_for_pid(pid: u32) -> Self {
        Self {
            pid,
            sample_interval: Duration::from_secs(5),
            total_duration: Duration::from_secs(60),
            max_rss_slope_kb_per_sec: 10.0,
            max_fd_slope_per_sec: 0.1,
        }
    }
}

pub fn read_pid_rss_kb(pid: u32) -> Option<u64> {
    let content = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in content.lines() {
        if line.starts_with("VmRSS:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].parse().ok();
            }
        }
    }
    None
}

pub fn read_pid_open_fds(pid: u32) -> Option<u64> {
    let fd_dir = format!("/proc/{pid}/fd");
    fs::read_dir(&fd_dir)
        .ok()
        .map(|entries| entries.filter_map(Result::ok).count() as u64)
}

pub fn read_pid_threads(pid: u32) -> Option<u64> {
    let content = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in content.lines() {
        if line.starts_with("Threads:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].parse().ok();
            }
        }
    }
    None
}

fn linear_slope(samples: &[(f64, f64)]) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }
    let n = samples.len() as f64;
    let sum_x: f64 = samples.iter().map(|(x, _)| x).sum();
    let sum_y: f64 = samples.iter().map(|(_, y)| y).sum();
    let sum_xy: f64 = samples.iter().map(|(x, y)| x * y).sum();
    let sum_xx: f64 = samples.iter().map(|(x, _)| x * x).sum();
    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-12 {
        return 0.0;
    }
    (n * sum_xy - sum_x * sum_y) / denom
}

pub fn run_soak(config: &SoakConfig) -> SoakReport {
    let start = Instant::now();
    let mut samples = Vec::new();

    loop {
        let elapsed = start.elapsed().as_secs_f64();
        samples.push(ProcessMetricsSample {
            elapsed_secs: elapsed,
            rss_kb: read_pid_rss_kb(config.pid),
            open_fds: read_pid_open_fds(config.pid),
            thread_count: read_pid_threads(config.pid),
        });

        if start.elapsed() >= config.total_duration {
            break;
        }
        std::thread::sleep(config.sample_interval);
    }

    let rss_data: Vec<(f64, f64)> = samples
        .iter()
        .filter_map(|s| s.rss_kb.map(|r| (s.elapsed_secs, r as f64)))
        .collect();
    let fd_data: Vec<(f64, f64)> = samples
        .iter()
        .filter_map(|s| s.open_fds.map(|f| (s.elapsed_secs, f as f64)))
        .collect();

    let rss_slope = linear_slope(&rss_data);
    let fd_slope = linear_slope(&fd_data);

    let rss_leak = rss_slope > config.max_rss_slope_kb_per_sec;
    let fd_leak = fd_slope > config.max_fd_slope_per_sec;
    let leaked = rss_leak || fd_leak;

    let duration_secs = start.elapsed().as_secs_f64();
    let summary = if leaked {
        format!(
            "SOAK FAILED: rss_slope={rss_slope:.3} KB/s (limit {:.3}), fd_slope={fd_slope:.5}/s (limit {:.5})",
            config.max_rss_slope_kb_per_sec, config.max_fd_slope_per_sec
        )
    } else {
        format!(
            "SOAK PASSED: rss_slope={rss_slope:.3} KB/s, fd_slope={fd_slope:.5}/s over {duration_secs:.0}s"
        )
    };

    SoakReport {
        pid: config.pid,
        samples,
        duration_secs,
        rss_growth_slope: rss_slope,
        fd_growth_slope: fd_slope,
        leaked,
        rss_threshold_kb_per_sec: config.max_rss_slope_kb_per_sec,
        fd_threshold_per_sec: config.max_fd_slope_per_sec,
        summary,
    }
}

pub fn read_soak_csv(report: &SoakReport) -> String {
    let mut out = String::from("elapsed_secs,rss_kb,open_fds,threads\n");
    for s in &report.samples {
        out.push_str(&format!(
            "{},{},{},{}\n",
            s.elapsed_secs,
            s.rss_kb.unwrap_or(0),
            s.open_fds.unwrap_or(0),
            s.thread_count.unwrap_or(0),
        ));
    }
    out
}

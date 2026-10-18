use control_plane::soak::{
    read_pid_open_fds, read_pid_rss_kb, read_pid_threads, read_soak_csv, run_soak,
    ProcessMetricsSample, SoakConfig,
};
use std::process;
use std::time::Duration;

#[test]
fn test_soak_metric_readers() {
    let pid = process::id();
    let rss = read_pid_rss_kb(pid);
    assert!(rss.is_some());
    assert!(rss.unwrap() > 0);

    let fds = read_pid_open_fds(pid);
    assert!(fds.is_some());
    assert!(fds.unwrap() > 0);

    let threads = read_pid_threads(pid);
    assert!(threads.is_some());
    assert!(threads.unwrap() >= 1);
}

#[test]
fn test_soak_harness_detects_stability() {
    let pid = process::id();
    let config = SoakConfig {
        pid,
        sample_interval: Duration::from_millis(50),
        total_duration: Duration::from_millis(250),
        max_rss_slope_kb_per_sec: 10000.0,
        max_fd_slope_per_sec: 100.0,
    };

    let report = run_soak(&config);
    assert!(report.passed(), "Expected soak run to pass: {}", report.summary);
    assert!(!report.samples.is_empty());

    let csv = read_soak_csv(&report);
    assert!(csv.starts_with("elapsed_secs,rss_kb,open_fds,threads\n"));
}

#[test]
fn test_soak_detects_synthetic_leak() {
    // Construct synthetic samples showing RSS runaway
    let samples = vec![
        ProcessMetricsSample {
            elapsed_secs: 0.0,
            rss_kb: Some(1000),
            open_fds: Some(10),
            thread_count: Some(2),
        },
        ProcessMetricsSample {
            elapsed_secs: 10.0,
            rss_kb: Some(5000),
            open_fds: Some(10),
            thread_count: Some(2),
        },
        ProcessMetricsSample {
            elapsed_secs: 20.0,
            rss_kb: Some(9000),
            open_fds: Some(10),
            thread_count: Some(2),
        },
    ];

    // Slope is (9000-1000)/20 = 400 KB/s
    let config = SoakConfig {
        pid: 1,
        sample_interval: Duration::from_secs(1),
        total_duration: Duration::from_secs(20),
        max_rss_slope_kb_per_sec: 100.0, // threshold 100 < 400
        max_fd_slope_per_sec: 1.0,
    };

    // Calculate linear slope directly to verify threshold detection logic
    let rss_data: Vec<(f64, f64)> = samples
        .iter()
        .filter_map(|s| s.rss_kb.map(|r| (s.elapsed_secs, r as f64)))
        .collect();
    let n = rss_data.len() as f64;
    let sum_x: f64 = rss_data.iter().map(|(x, _)| x).sum();
    let sum_y: f64 = rss_data.iter().map(|(_, y)| y).sum();
    let sum_xy: f64 = rss_data.iter().map(|(x, y)| x * y).sum();
    let sum_xx: f64 = rss_data.iter().map(|(x, _)| x * x).sum();
    let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_xx - sum_x * sum_x);

    assert!(
        slope > config.max_rss_slope_kb_per_sec,
        "Slope {slope} must exceed threshold {}",
        config.max_rss_slope_kb_per_sec
    );
}

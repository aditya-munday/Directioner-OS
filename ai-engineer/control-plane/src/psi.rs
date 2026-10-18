use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PsiValues {
    pub avg10: f64,
    pub avg60: f64,
    pub avg300: f64,
    pub total_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PsiReading {
    pub available: bool,
    pub some: Option<PsiValues>,
    pub full: Option<PsiValues>,
}

impl PsiReading {
    pub fn unavailable() -> Self {
        Self {
            available: false,
            some: None,
            full: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PsiSnapshot {
    pub cpu: PsiReading,
    pub memory: PsiReading,
    pub io: PsiReading,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PsiThresholds {
    pub cpu_some_avg10_warn: f64,
    pub memory_some_avg10_warn: f64,
    pub io_some_avg10_warn: f64,
    pub cpu_full_avg10_crit: f64,
    pub memory_full_avg10_crit: f64,
    pub io_full_avg10_crit: f64,
}

impl Default for PsiThresholds {
    fn default() -> Self {
        Self {
            cpu_some_avg10_warn: 30.0,
            memory_some_avg10_warn: 20.0,
            io_some_avg10_warn: 25.0,
            cpu_full_avg10_crit: 5.0,
            memory_full_avg10_crit: 10.0,
            io_full_avg10_crit: 15.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PsiPressureLevel {
    None,
    Warn,
    Critical,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PsiHealthSignal {
    pub cpu_level: PsiPressureLevel,
    pub memory_level: PsiPressureLevel,
    pub io_level: PsiPressureLevel,
    pub overall_level: PsiPressureLevel,
    pub score_contribution: i32,
    pub summary: String,
}

fn parse_psi_line(line: &str) -> Option<PsiValues> {
    let mut avg10 = None;
    let mut avg60 = None;
    let mut avg300 = None;
    let mut total_us = None;

    for token in line.split_whitespace() {
        if let Some(v) = token.strip_prefix("avg10=") {
            avg10 = v.parse().ok();
        } else if let Some(v) = token.strip_prefix("avg60=") {
            avg60 = v.parse().ok();
        } else if let Some(v) = token.strip_prefix("avg300=") {
            avg300 = v.parse().ok();
        } else if let Some(v) = token.strip_prefix("total=") {
            total_us = v.parse().ok();
        }
    }

    Some(PsiValues {
        avg10: avg10?,
        avg60: avg60?,
        avg300: avg300?,
        total_us: total_us?,
    })
}

pub fn read_psi_file(path: &str) -> PsiReading {
    match fs::read_to_string(path) {
        Err(_) => PsiReading::unavailable(),
        Ok(content) => {
            let mut lines = content.lines();
            let some_line = lines.next().unwrap_or("").trim_start_matches("some ");
            let full_line = lines.next().unwrap_or("").trim_start_matches("full ");
            PsiReading {
                available: true,
                some: parse_psi_line(some_line),
                full: parse_psi_line(full_line),
            }
        }
    }
}

pub fn read_psi_snapshot() -> PsiSnapshot {
    PsiSnapshot {
        cpu: read_psi_file("/proc/pressure/cpu"),
        memory: read_psi_file("/proc/pressure/memory"),
        io: read_psi_file("/proc/pressure/io"),
    }
}

pub fn read_psi_snapshot_from_strings(cpu: &str, memory: &str, io: &str) -> PsiSnapshot {
    fn parse_full(s: &str) -> PsiReading {
        if s.trim().is_empty() {
            return PsiReading::unavailable();
        }
        let mut lines = s.lines();
        let some_line = lines.next().unwrap_or("").trim_start_matches("some ");
        let full_line = lines.next().unwrap_or("").trim_start_matches("full ");
        let some = parse_psi_line(some_line);
        let full = parse_psi_line(full_line);
        if some.is_none() && full.is_none() {
            PsiReading::unavailable()
        } else {
            PsiReading {
                available: true,
                some,
                full,
            }
        }
    }
    PsiSnapshot {
        cpu: parse_full(cpu),
        memory: parse_full(memory),
        io: parse_full(io),
    }
}

fn level_for(some_avg10: Option<f64>, full_avg10: Option<f64>, some_warn: f64, full_crit: f64) -> PsiPressureLevel {
    let s = some_avg10.unwrap_or(0.0);
    let f = full_avg10.unwrap_or(0.0);
    if f >= full_crit {
        PsiPressureLevel::Critical
    } else if s >= some_warn {
        PsiPressureLevel::Warn
    } else {
        PsiPressureLevel::None
    }
}

pub fn evaluate_psi(snapshot: &PsiSnapshot, thresholds: &PsiThresholds) -> PsiHealthSignal {
    if !snapshot.cpu.available && !snapshot.memory.available && !snapshot.io.available {
        return PsiHealthSignal {
            cpu_level: PsiPressureLevel::Unavailable,
            memory_level: PsiPressureLevel::Unavailable,
            io_level: PsiPressureLevel::Unavailable,
            overall_level: PsiPressureLevel::Unavailable,
            score_contribution: 0,
            summary: "PSI not available on this kernel".to_string(),
        };
    }

    let cpu_level = if snapshot.cpu.available {
        level_for(
            snapshot.cpu.some.as_ref().map(|v| v.avg10),
            snapshot.cpu.full.as_ref().map(|v| v.avg10),
            thresholds.cpu_some_avg10_warn,
            thresholds.cpu_full_avg10_crit,
        )
    } else {
        PsiPressureLevel::Unavailable
    };

    let memory_level = if snapshot.memory.available {
        level_for(
            snapshot.memory.some.as_ref().map(|v| v.avg10),
            snapshot.memory.full.as_ref().map(|v| v.avg10),
            thresholds.memory_some_avg10_warn,
            thresholds.memory_full_avg10_crit,
        )
    } else {
        PsiPressureLevel::Unavailable
    };

    let io_level = if snapshot.io.available {
        level_for(
            snapshot.io.some.as_ref().map(|v| v.avg10),
            snapshot.io.full.as_ref().map(|v| v.avg10),
            thresholds.io_some_avg10_warn,
            thresholds.io_full_avg10_crit,
        )
    } else {
        PsiPressureLevel::Unavailable
    };

    let overall_level = [&cpu_level, &memory_level, &io_level]
        .iter()
        .fold(PsiPressureLevel::None, |acc, level| match (acc, level) {
            (PsiPressureLevel::Critical, _) | (_, PsiPressureLevel::Critical) => PsiPressureLevel::Critical,
            (PsiPressureLevel::Warn, _) | (_, PsiPressureLevel::Warn) => PsiPressureLevel::Warn,
            (PsiPressureLevel::Unavailable, _) | (_, PsiPressureLevel::Unavailable) => PsiPressureLevel::Unavailable,
            _ => PsiPressureLevel::None,
        });

    let score_contribution = match &overall_level {
        PsiPressureLevel::Critical => -30,
        PsiPressureLevel::Warn => -10,
        PsiPressureLevel::None | PsiPressureLevel::Unavailable => 0,
    };

    let summary = format!(
        "PSI pressure: cpu={cpu_level:?} mem={memory_level:?} io={io_level:?}"
    );

    PsiHealthSignal {
        cpu_level,
        memory_level,
        io_level,
        overall_level,
        score_contribution,
        summary,
    }
}

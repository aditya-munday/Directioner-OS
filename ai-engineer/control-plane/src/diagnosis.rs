use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosisClass {
    FailedUnit,
    BootLoop,
    DiskFull,
    Oom,
    NetworkLoss,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosisEvidence {
    pub timestamp: String,
    pub source: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosisReport {
    pub generated_at: String,
    pub class: DiagnosisClass,
    pub confidence: f64,
    pub summary: String,
    pub evidence: Vec<DiagnosisEvidence>,
}

impl DiagnosisReport {
    pub fn unknown(reason: &str) -> Self {
        Self {
            generated_at: Utc::now().to_rfc3339(),
            class: DiagnosisClass::Unknown,
            confidence: 0.0,
            summary: reason.to_string(),
            evidence: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JournalSnapshot {
    pub unit_states: HashMap<String, String>,
    pub recent_messages: Vec<JournalMessage>,
    pub disk_usage_percent: Option<f64>,
    pub memory_pressure_avg10: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JournalMessage {
    pub timestamp: String,
    pub unit: Option<String>,
    pub message: String,
    pub priority: u8,
}

pub fn diagnose_failed_unit(snapshot: &JournalSnapshot) -> DiagnosisReport {
    let now = Utc::now().to_rfc3339();
    let failed: Vec<_> = snapshot
        .unit_states
        .iter()
        .filter(|(_, state)| *state == "failed")
        .map(|(name, _)| name.clone())
        .collect();

    if failed.is_empty() {
        return DiagnosisReport {
            generated_at: now,
            class: DiagnosisClass::Unknown,
            confidence: 0.0,
            summary: "No failed units detected".to_string(),
            evidence: Vec::new(),
        };
    }

    let evidence: Vec<_> = failed
        .iter()
        .map(|u| DiagnosisEvidence {
            timestamp: now.clone(),
            source: "systemd.unit_states".to_string(),
            detail: format!("Unit {u} is in failed state"),
        })
        .collect();

    let confidence = (0.5 + (failed.len() as f64 * 0.1)).min(1.0);
    DiagnosisReport {
        generated_at: now,
        class: DiagnosisClass::FailedUnit,
        confidence,
        summary: format!("{} failed unit(s): {}", failed.len(), failed.join(", ")),
        evidence,
    }
}

pub fn diagnose_boot_loop(snapshot: &JournalSnapshot) -> DiagnosisReport {
    let now = Utc::now().to_rfc3339();
    let mut restart_counts: HashMap<String, usize> = HashMap::new();
    for msg in &snapshot.recent_messages {
        let m = msg.message.to_ascii_lowercase();
        if m.contains("start request repeated") || m.contains("reached restart limit") {
            if let Some(ref unit) = msg.unit {
                *restart_counts.entry(unit.clone()).or_insert(0) += 1;
            }
        }
    }

    let looping: Vec<_> = restart_counts
        .iter()
        .filter(|(_, count)| **count >= 2)
        .collect();

    if looping.is_empty() {
        return DiagnosisReport {
            generated_at: now,
            class: DiagnosisClass::Unknown,
            confidence: 0.0,
            summary: "No boot loop signals detected".to_string(),
            evidence: Vec::new(),
        };
    }

    let evidence: Vec<_> = looping
        .iter()
        .map(|(unit, count)| DiagnosisEvidence {
            timestamp: now.clone(),
            source: "journal.messages".to_string(),
            detail: format!("Unit {unit} restart signal observed {count} times"),
        })
        .collect();

    DiagnosisReport {
        generated_at: now,
        class: DiagnosisClass::BootLoop,
        confidence: 0.85,
        summary: format!("{} unit(s) in restart loop", looping.len()),
        evidence,
    }
}

pub fn diagnose_disk_full(snapshot: &JournalSnapshot) -> DiagnosisReport {
    let now = Utc::now().to_rfc3339();
    let disk_pct = match snapshot.disk_usage_percent {
        Some(p) => p,
        None => {
            return DiagnosisReport {
                generated_at: now,
                class: DiagnosisClass::Unknown,
                confidence: 0.0,
                summary: "Disk usage data not available".to_string(),
                evidence: Vec::new(),
            }
        }
    };

    let journal_enospc = snapshot.recent_messages.iter().any(|m| {
        let msg = m.message.to_ascii_lowercase();
        msg.contains("no space left") || msg.contains("enospc")
    });

    let confidence = if disk_pct >= 98.0 {
        0.95
    } else if disk_pct >= 90.0 {
        0.7
    } else if journal_enospc {
        0.6
    } else {
        0.0
    };

    if confidence == 0.0 {
        return DiagnosisReport {
            generated_at: now,
            class: DiagnosisClass::Unknown,
            confidence: 0.0,
            summary: format!("Disk at {disk_pct:.1}% — within normal thresholds"),
            evidence: Vec::new(),
        };
    }

    DiagnosisReport {
        generated_at: now.clone(),
        class: DiagnosisClass::DiskFull,
        confidence,
        summary: format!("Disk usage at {disk_pct:.1}% — critically high"),
        evidence: vec![DiagnosisEvidence {
            timestamp: now,
            source: "disk.usage".to_string(),
            detail: format!("Disk at {disk_pct:.1}%, ENOSPC signals: {journal_enospc}"),
        }],
    }
}

pub fn diagnose_oom(snapshot: &JournalSnapshot) -> DiagnosisReport {
    let now = Utc::now().to_rfc3339();
    let oom_msgs: Vec<_> = snapshot
        .recent_messages
        .iter()
        .filter(|m| {
            let msg = m.message.to_ascii_lowercase();
            msg.contains("out of memory") || msg.contains("oom-killer") || msg.contains("killed process")
        })
        .collect();

    if oom_msgs.is_empty() {
        return DiagnosisReport {
            generated_at: now,
            class: DiagnosisClass::Unknown,
            confidence: 0.0,
            summary: "No OOM signals detected".to_string(),
            evidence: Vec::new(),
        };
    }

    let evidence: Vec<_> = oom_msgs
        .iter()
        .map(|m| DiagnosisEvidence {
            timestamp: m.timestamp.clone(),
            source: "journal.messages".to_string(),
            detail: m.message.clone(),
        })
        .collect();

    DiagnosisReport {
        generated_at: now,
        class: DiagnosisClass::Oom,
        confidence: 0.9,
        summary: format!("{} OOM signal(s) detected in journal", oom_msgs.len()),
        evidence,
    }
}

pub fn diagnose_network_loss(snapshot: &JournalSnapshot) -> DiagnosisReport {
    let now = Utc::now().to_rfc3339();
    let net_msgs: Vec<_> = snapshot
        .recent_messages
        .iter()
        .filter(|m| {
            let msg = m.message.to_ascii_lowercase();
            msg.contains("network unreachable")
                || msg.contains("network is unreachable")
                || msg.contains("link is down")
                || msg.contains("no route to host")
                || msg.contains("name or service not known")
        })
        .collect();

    if net_msgs.is_empty() {
        return DiagnosisReport {
            generated_at: now,
            class: DiagnosisClass::Unknown,
            confidence: 0.0,
            summary: "No network loss signals detected".to_string(),
            evidence: Vec::new(),
        };
    }

    let evidence: Vec<_> = net_msgs
        .iter()
        .map(|m| DiagnosisEvidence {
            timestamp: m.timestamp.clone(),
            source: "journal.messages".to_string(),
            detail: m.message.clone(),
        })
        .collect();

    DiagnosisReport {
        generated_at: now,
        class: DiagnosisClass::NetworkLoss,
        confidence: 0.8,
        summary: format!("{} network loss signal(s) detected", net_msgs.len()),
        evidence,
    }
}

pub fn run_all_diagnoses(snapshot: &JournalSnapshot) -> Vec<DiagnosisReport> {
    let mut results = vec![
        diagnose_failed_unit(snapshot),
        diagnose_boot_loop(snapshot),
        diagnose_disk_full(snapshot),
        diagnose_oom(snapshot),
        diagnose_network_loss(snapshot),
    ];
    results.retain(|r| r.class != DiagnosisClass::Unknown || !r.evidence.is_empty());
    results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    results
}

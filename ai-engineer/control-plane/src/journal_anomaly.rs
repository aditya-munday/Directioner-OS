use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalAnomalyPattern {
    pub name: &'static str,
    pub keywords: &'static [&'static str],
    pub class: JournalAnomalyClass,
    pub severity: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JournalAnomalyClass {
    OomKill,
    Segfault,
    IoError,
    AuthFlood,
    Unknown,
}

pub const ANOMALY_PATTERNS: &[JournalAnomalyPattern] = &[
    JournalAnomalyPattern {
        name: "oom-kill",
        keywords: &["out of memory", "oom-killer", "killed process"],
        class: JournalAnomalyClass::OomKill,
        severity: "critical",
    },
    JournalAnomalyPattern {
        name: "segfault",
        keywords: &["segfault", "segmentation fault", "core dumped"],
        class: JournalAnomalyClass::Segfault,
        severity: "critical",
    },
    JournalAnomalyPattern {
        name: "io-error",
        keywords: &["i/o error", "input/output error", "blk_update_request", "medium error"],
        class: JournalAnomalyClass::IoError,
        severity: "warning",
    },
    JournalAnomalyPattern {
        name: "auth-flood",
        keywords: &["authentication failure", "failed password", "invalid user", "did not receive identification"],
        class: JournalAnomalyClass::AuthFlood,
        severity: "warning",
    },
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JournalAnomalyHit {
    pub timestamp: String,
    pub unit: Option<String>,
    pub message: String,
    pub pattern: String,
    pub class: JournalAnomalyClass,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JournalAnomalyReport {
    pub generated_at: String,
    pub total_messages_scanned: usize,
    pub hits: Vec<JournalAnomalyHit>,
    pub summary: String,
}

impl JournalAnomalyReport {
    pub fn is_clean(&self) -> bool {
        self.hits.is_empty()
    }
}

pub struct JournalMessage {
    pub timestamp: String,
    pub unit: Option<String>,
    pub message: String,
}

pub fn scan_journal(messages: &[JournalMessage]) -> JournalAnomalyReport {
    let now = chrono::Utc::now().to_rfc3339();
    let mut hits = Vec::new();

    for msg in messages {
        let lower = msg.message.to_ascii_lowercase();
        for pattern in ANOMALY_PATTERNS {
            let matched = pattern.keywords.iter().any(|kw| lower.contains(kw));
            if matched {
                hits.push(JournalAnomalyHit {
                    timestamp: msg.timestamp.clone(),
                    unit: msg.unit.clone(),
                    message: msg.message.clone(),
                    pattern: pattern.name.to_string(),
                    class: pattern.class.clone(),
                    severity: pattern.severity.to_string(),
                });
                break;
            }
        }
    }

    let summary = if hits.is_empty() {
        format!("No anomalies in {} messages", messages.len())
    } else {
        format!(
            "{} anomaly hit(s) in {} messages scanned",
            hits.len(),
            messages.len()
        )
    };

    JournalAnomalyReport {
        generated_at: now,
        total_messages_scanned: messages.len(),
        hits,
        summary,
    }
}

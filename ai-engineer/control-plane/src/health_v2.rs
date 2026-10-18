use serde::{Deserialize, Serialize};

use crate::health_model::{HealthStatus, SubsystemHealth};
use crate::psi::{PsiHealthSignal, PsiPressureLevel};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoredComponent {
    pub name: String,
    pub weight: f64,
    pub score: u32,
    pub confidence: f64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthScoringV2Report {
    pub evaluated_at: String,
    pub composite_score: Option<u32>,
    pub confidence: f64,
    pub status: HealthStatus,
    pub top_contributors: Vec<(String, i32)>,
    pub components: Vec<ScoredComponent>,
    pub summary: String,
    pub unknown_inputs: Vec<String>,
}

impl HealthScoringV2Report {
    pub fn passed_confidence_threshold(&self, min: f64) -> bool {
        self.confidence >= min
    }
}

#[derive(Debug, Clone, Default)]
pub struct HealthScoringV2Input {
    pub subsystems: Vec<SubsystemHealth>,
    pub psi: Option<PsiHealthSignal>,
    pub missing_inputs: Vec<String>,
}

pub fn score_v2(input: &HealthScoringV2Input) -> HealthScoringV2Report {
    let evaluated_at = chrono::Utc::now().to_rfc3339();
    let mut components: Vec<ScoredComponent> = Vec::new();
    let mut contributions: Vec<(String, i32)> = Vec::new();
    let mut unknown_inputs = input.missing_inputs.clone();

    let subsystem_weights: &[(&str, f64)] = &[
        ("services", 5.0),
        ("boot", 3.0),
        ("packages", 2.0),
    ];

    for (name, weight) in subsystem_weights {
        match input.subsystems.iter().find(|s| s.name == *name) {
            None => {
                unknown_inputs.push(format!("subsystem:{name}"));
                components.push(ScoredComponent {
                    name: name.to_string(),
                    weight: *weight,
                    score: 0,
                    confidence: 0.0,
                    notes: vec!["No telemetry available".to_string()],
                });
            }
            Some(sub) => {
                let (score, conf) = match sub.score {
                    None => {
                        unknown_inputs.push(format!("subsystem:{name}:score"));
                        (0u32, 0.0f64)
                    }
                    Some(s) => (s, 1.0),
                };
                let contribution = score as i32 - 100;
                if contribution < 0 {
                    contributions.push((name.to_string(), contribution));
                }
                components.push(ScoredComponent {
                    name: name.to_string(),
                    weight: *weight,
                    score,
                    confidence: conf,
                    notes: sub.evidence.iter().map(|e| e.description.clone()).collect(),
                });
            }
        }
    }

    if let Some(psi) = &input.psi {
        let psi_score_contribution = psi.score_contribution;
        if psi_score_contribution < 0 {
            contributions.push(("psi_pressure".to_string(), psi_score_contribution));
        }
        let psi_base = 100i32;
        let psi_score = (psi_base + psi_score_contribution).max(0) as u32;
        let psi_conf = if psi.overall_level == PsiPressureLevel::Unavailable { 0.0 } else { 1.0 };
        if psi_conf == 0.0 {
            unknown_inputs.push("psi_pressure".to_string());
        }
        components.push(ScoredComponent {
            name: "psi_pressure".to_string(),
            weight: 2.0,
            score: psi_score,
            confidence: psi_conf,
            notes: vec![psi.summary.clone()],
        });
    } else {
        unknown_inputs.push("psi_pressure".to_string());
    }

    let scored_components: Vec<&ScoredComponent> = components.iter().filter(|c| c.confidence > 0.0).collect();
    if scored_components.is_empty() {
        return HealthScoringV2Report {
            evaluated_at,
            composite_score: None,
            confidence: 0.0,
            status: HealthStatus::Unknown,
            top_contributors: Vec::new(),
            components,
            summary: "Insufficient telemetry to evaluate health (all inputs unknown)".to_string(),
            unknown_inputs,
        };
    }

    let total_weight: f64 = scored_components.iter().map(|c| c.weight).sum();
    let weighted_sum: f64 = scored_components.iter().map(|c| c.score as f64 * c.weight).sum();
    let raw_score = (weighted_sum / total_weight).round() as u32;
    let composite_score = raw_score.min(100);

    let confidence = scored_components.len() as f64 / components.len() as f64;

    let status = match composite_score {
        90..=100 => HealthStatus::Healthy,
        60..=89 => HealthStatus::Degraded,
        _ => HealthStatus::Critical,
    };

    contributions.sort_by_key(|(_, v)| *v);
    let top_contributors = contributions.into_iter().take(5).collect();

    let summary = format!(
        "Health score: {composite_score}/100 ({status}) — confidence: {:.0}% — {}/{} inputs observable",
        confidence * 100.0,
        scored_components.len(),
        components.len()
    );

    HealthScoringV2Report {
        evaluated_at,
        composite_score: Some(composite_score),
        confidence,
        status,
        top_contributors,
        components,
        summary,
        unknown_inputs,
    }
}

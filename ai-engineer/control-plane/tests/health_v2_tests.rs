use control_plane::health_model::{HealthStatus, SubsystemHealth};
use control_plane::health_v2::{score_v2, HealthScoringV2Input};
use control_plane::psi::{PsiHealthSignal, PsiPressureLevel};

#[test]
fn test_health_v2_unknown_lowers_confidence() {
    let subsystems = vec![
        SubsystemHealth {
            name: "services".to_string(),
            score: Some(100),
            status: HealthStatus::Healthy,
            summary: "All services running".to_string(),
            evidence: Vec::new(),
        },
        SubsystemHealth {
            name: "boot".to_string(),
            score: None, // unknown score
            status: HealthStatus::Unknown,
            summary: "Boot telemetry unavailable".to_string(),
            evidence: Vec::new(),
        },
        SubsystemHealth {
            name: "packages".to_string(),
            score: Some(100),
            status: HealthStatus::Healthy,
            summary: "Packages intact".to_string(),
            evidence: Vec::new(),
        },
    ];

    let psi = PsiHealthSignal {
        cpu_level: PsiPressureLevel::None,
        memory_level: PsiPressureLevel::None,
        io_level: PsiPressureLevel::None,
        overall_level: PsiPressureLevel::None,
        score_contribution: 0,
        summary: "Normal".to_string(),
    };

    let input = HealthScoringV2Input {
        subsystems,
        psi: Some(psi),
        missing_inputs: Vec::new(),
    };

    let report = score_v2(&input);
    // Because boot has no score, confidence must be lower than 1.0
    assert!(
        report.confidence < 1.0,
        "Unknown input must lower confidence: got {}",
        report.confidence
    );
    // Backwards compatibility: composite_score returns a valid u32 score
    assert!(report.composite_score.is_some());
    assert!(report.composite_score.unwrap() <= 100);
}

#[test]
fn test_health_v2_psi_unavailable_lowers_confidence() {
    let subsystems = vec![
        SubsystemHealth {
            name: "services".to_string(),
            score: Some(100),
            status: HealthStatus::Healthy,
            summary: "OK".to_string(),
            evidence: Vec::new(),
        },
        SubsystemHealth {
            name: "boot".to_string(),
            score: Some(100),
            status: HealthStatus::Healthy,
            summary: "OK".to_string(),
            evidence: Vec::new(),
        },
        SubsystemHealth {
            name: "packages".to_string(),
            score: Some(100),
            status: HealthStatus::Healthy,
            summary: "OK".to_string(),
            evidence: Vec::new(),
        },
    ];

    let psi_unavail = PsiHealthSignal {
        cpu_level: PsiPressureLevel::Unavailable,
        memory_level: PsiPressureLevel::Unavailable,
        io_level: PsiPressureLevel::Unavailable,
        overall_level: PsiPressureLevel::Unavailable,
        score_contribution: 0,
        summary: "PSI unavailable".to_string(),
    };

    let input = HealthScoringV2Input {
        subsystems,
        psi: Some(psi_unavail),
        missing_inputs: Vec::new(),
    };

    let report = score_v2(&input);
    assert!(report.confidence < 1.0);
}

#[test]
fn test_health_v2_explainability() {
    let subsystems = vec![
        SubsystemHealth {
            name: "services".to_string(),
            score: Some(50),
            status: HealthStatus::Degraded,
            summary: "2 failed units".to_string(),
            evidence: Vec::new(),
        },
        SubsystemHealth {
            name: "boot".to_string(),
            score: Some(100),
            status: HealthStatus::Healthy,
            summary: "OK".to_string(),
            evidence: Vec::new(),
        },
        SubsystemHealth {
            name: "packages".to_string(),
            score: Some(100),
            status: HealthStatus::Healthy,
            summary: "OK".to_string(),
            evidence: Vec::new(),
        },
    ];

    let input = HealthScoringV2Input {
        subsystems,
        psi: None,
        missing_inputs: Vec::new(),
    };

    let report = score_v2(&input);
    assert!(!report.top_contributors.is_empty());
    // top contributor should flag services penalty
    assert_eq!(report.top_contributors[0].0, "services");
    assert!(report.top_contributors[0].1 < 0);
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentKind {
    LaunchApplication,
    SetSystemSetting,
    QuerySystemStatus,
    ScheduleAlarm,
    ExecuteAction,
    DiagnoseIncident,
    MediaPlayback,
    NetworkSwitch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImpactRating {
    Readonly,
    UserScope,
    SystemConfig,
    PrivilegedMutation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentRequest {
    pub raw_utterance: String,
    pub kind: IntentKind,
    pub slots: HashMap<String, String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentResult {
    DraftPreview {
        action_summary: String,
        target_subsystem: String,
        dry_run_output: String,
    },
    Rejected {
        reason: String,
    },
}

pub struct IntentBroker {
    min_confidence: f32,
}

impl IntentBroker {
    pub fn new(min_confidence: f32) -> Self {
        IntentBroker { min_confidence }
    }

    pub fn classify_impact(&self, kind: &IntentKind) -> ImpactRating {
        match kind {
            IntentKind::QuerySystemStatus => ImpactRating::Readonly,
            IntentKind::MediaPlayback | IntentKind::ScheduleAlarm => ImpactRating::UserScope,
            IntentKind::LaunchApplication | IntentKind::SetSystemSetting => {
                ImpactRating::SystemConfig
            }
            IntentKind::ExecuteAction
            | IntentKind::DiagnoseIncident
            | IntentKind::NetworkSwitch => ImpactRating::PrivilegedMutation,
        }
    }

    pub fn dispatch(&self, req: IntentRequest) -> IntentResult {
        if req.confidence < self.min_confidence {
            return IntentResult::Rejected {
                reason: "confidence_too_low".to_string(),
            };
        }

        let impact = self.classify_impact(&req.kind);
        let target = req
            .slots
            .get("target")
            .cloned()
            .unwrap_or_else(|| "system".to_string());

        let summary = match &req.kind {
            IntentKind::LaunchApplication => {
                let app = req.slots.get("app_name").cloned().unwrap_or_default();
                format!("Would launch application: {}", app)
            }
            IntentKind::QuerySystemStatus => "Would query system status".to_string(),
            IntentKind::SetSystemSetting => {
                let key = req.slots.get("key").cloned().unwrap_or_default();
                let val = req.slots.get("val").cloned().unwrap_or_default();
                format!("Would set {} to {}", key, val)
            }
            IntentKind::ScheduleAlarm => {
                let time = req.slots.get("time").cloned().unwrap_or_default();
                format!("Would schedule alarm for {}", time)
            }
            IntentKind::ExecuteAction => {
                format!("Would execute: {}", req.raw_utterance)
            }
            IntentKind::DiagnoseIncident => {
                format!("Would diagnose: {}", target)
            }
            IntentKind::MediaPlayback => {
                let action = req.slots.get("action").cloned().unwrap_or_default();
                format!("Would perform media playback: {}", action)
            }
            IntentKind::NetworkSwitch => {
                let profile = req.slots.get("profile").cloned().unwrap_or_default();
                format!("Would switch network to: {}", profile)
            }
        };

        let dry_run_note = match impact {
            ImpactRating::PrivilegedMutation => {
                "Rule B1: privileged mutation requires explicit owner authorization before execution"
                    .to_string()
            }
            ImpactRating::SystemConfig => {
                "Conversational input produces draft only; system config changes require confirmation".to_string()
            }
            ImpactRating::UserScope => {
                "Conversational input produces draft only; confirm to proceed".to_string()
            }
            ImpactRating::Readonly => {
                "Conversational input produces draft only; read-only query pending confirmation"
                    .to_string()
            }
        };

        IntentResult::DraftPreview {
            action_summary: summary,
            target_subsystem: target,
            dry_run_output: dry_run_note,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req(kind: IntentKind, confidence: f32) -> IntentRequest {
        IntentRequest {
            raw_utterance: "test utterance".to_string(),
            kind,
            slots: HashMap::new(),
            confidence,
        }
    }

    #[test]
    fn no_conversational_input_produces_executed_result() {
        let broker = IntentBroker::new(0.5);
        let all_kinds = vec![
            IntentKind::LaunchApplication,
            IntentKind::SetSystemSetting,
            IntentKind::QuerySystemStatus,
            IntentKind::ScheduleAlarm,
            IntentKind::ExecuteAction,
            IntentKind::DiagnoseIncident,
            IntentKind::MediaPlayback,
            IntentKind::NetworkSwitch,
        ];

        for kind in all_kinds {
            let result = broker.dispatch(make_req(kind, 0.99));
            match result {
                IntentResult::DraftPreview { .. } | IntentResult::Rejected { .. } => {}
            }
        }
    }

    #[test]
    fn low_confidence_is_rejected() {
        let broker = IntentBroker::new(0.7);
        let result = broker.dispatch(make_req(IntentKind::QuerySystemStatus, 0.3));
        assert_eq!(
            result,
            IntentResult::Rejected {
                reason: "confidence_too_low".to_string()
            }
        );
    }

    #[test]
    fn high_confidence_produces_draft_not_executed() {
        let broker = IntentBroker::new(0.5);
        let result = broker.dispatch(make_req(IntentKind::QuerySystemStatus, 0.99));
        assert!(
            matches!(result, IntentResult::DraftPreview { .. }),
            "expected DraftPreview, got {:?}",
            result
        );
    }

    #[test]
    fn privileged_mutation_draft_mentions_rule_b1() {
        let broker = IntentBroker::new(0.5);
        let result = broker.dispatch(make_req(IntentKind::ExecuteAction, 0.99));
        if let IntentResult::DraftPreview { dry_run_output, .. } = result {
            assert!(
                dry_run_output.contains("Rule B1"),
                "expected Rule B1 mention in dry_run_output"
            );
        } else {
            panic!("expected DraftPreview");
        }
    }
}

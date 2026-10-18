use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentKind {
    LaunchApplication,
    SetSystemSetting,
    QuerySystemStatus,
    ScheduleAlarm,
    ExecuteAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentRequest {
    pub raw_utterance: String,
    pub kind: IntentKind,
    pub slots: HashMap<String, String>,
    pub confidence: f32,
    pub execution_authorized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntentResult {
    DraftPreview {
        action_summary: String,
        target_subsystem: String,
        dry_run_output: String,
    },
    Executed {
        result_message: String,
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

    pub fn dispatch(&self, req: IntentRequest) -> IntentResult {
        if req.confidence < self.min_confidence {
            return IntentResult::Rejected {
                reason: "confidence_too_low".to_string(),
            };
        }

        if req.kind == IntentKind::ExecuteAction && !req.execution_authorized {
            return IntentResult::DraftPreview {
                action_summary: format!("Draft execution for: {}", req.raw_utterance),
                target_subsystem: req.slots.get("target").cloned().unwrap_or_else(|| "system".to_string()),
                dry_run_output: "Preview only: Rule B1 prohibits conversational execution without explicit owner authorization".to_string(),
            };
        }

        match req.kind {
            IntentKind::LaunchApplication => {
                let app = req.slots.get("app_name").cloned().unwrap_or_default();
                IntentResult::Executed {
                    result_message: format!("Launching {}", app),
                }
            }
            IntentKind::QuerySystemStatus => IntentResult::Executed {
                result_message: "System status: all healthy".to_string(),
            },
            IntentKind::SetSystemSetting => {
                let key = req.slots.get("key").cloned().unwrap_or_default();
                let val = req.slots.get("val").cloned().unwrap_or_default();
                IntentResult::Executed {
                    result_message: format!("Set {} to {}", key, val),
                }
            }
            IntentKind::ScheduleAlarm => {
                let time = req.slots.get("time").cloned().unwrap_or_default();
                IntentResult::Executed {
                    result_message: format!("Alarm scheduled for {}", time),
                }
            }
            IntentKind::ExecuteAction => IntentResult::Executed {
                result_message: format!("Executed authorized action: {}", req.raw_utterance),
            },
        }
    }
}

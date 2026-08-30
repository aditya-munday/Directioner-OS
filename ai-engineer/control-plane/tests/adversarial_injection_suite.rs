//! Comprehensive Adversarial Injection Test Suite (Phase 4.0.1 Hardening).
//!
//! Tests 18 distinct adversarial injection payloads against PlanDraftValidator:
//! 1. Unicode homoglyph in capability kind key ("kіnd" with Cyrillic 'і')
//! 2. Unicode homoglyph in capability variant name ("FileWrіte" with Cyrillic 'і')
//! 3. Zero-width character smuggling in capability name ("File\u{200B}Write")
//! 4. Deeply nested JSON depth bomb
//! 5. Overlong intent field (>1024 chars)
//! 6. Overlong summary field (>4096 chars)
//! 7. Excessive step count (>50 steps)
//! 8. Out-of-range high step timeout (>300s)
//! 9. Out-of-range zero step timeout (0s)
//! 10. Empty intent string
//! 11. Empty step_id string
//! 12. Array-type confusion in string field (array passed for path)
//! 13. Object-type confusion in array field (object passed for steps)
//! 14. Null injection in required path field
//! 15. Null injection in required package field
//! 16. Null injection in root intent field
//! 17. Boolean type confusion in package field
//! 18. Forbidden null-byte injection

use control_plane::draft_validator::{PlanDraftValidator, ValidationError};

#[test]
fn test_adv_01_unicode_homoglyph_in_kind_key_rejected() {
    // Uses Cyrillic small letter 'і' (U+0456) instead of Latin 'i' (U+0069)
    let payload = r#"{
        "intent": "Restart web server",
        "summary": "Restarts nginx",
        "steps": [
            {
                "step_id": "s1",
                "description": "restart",
                "capability": {
                    "kіnd": "ServiceRestart",
                    "unit": "nginx.service"
                }
            }
        ]
    }"#;

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::MalformedJson(_))),
        "Unicode homoglyph in 'kind' key must fail deserialization: {res:?}"
    );
}

#[test]
fn test_adv_02_unicode_homoglyph_in_variant_name_rejected() {
    // Uses Cyrillic 'і' in "FileWrіte"
    let payload = r#"{
        "intent": "Write config",
        "summary": "Writes custom.conf",
        "steps": [
            {
                "step_id": "s1",
                "description": "write",
                "capability": {
                    "kind": "FileWrіte",
                    "path": "/etc/custom.conf",
                    "content": "key=val"
                }
            }
        ]
    }"#;

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::MalformedJson(_))),
        "Unicode homoglyph in variant name must fail variant matching: {res:?}"
    );
}

#[test]
fn test_adv_03_zero_width_space_in_variant_name_rejected() {
    // Uses zero-width space U+200B in "File\u{200B}Write"
    let payload = "{\n        \"intent\": \"Write config\",\n        \"summary\": \"Writes config\",\n        \"steps\": [\n            {\n                \"step_id\": \"s1\",\n                \"description\": \"write\",\n                \"capability\": {\n                    \"kind\": \"File\u{200B}Write\",\n                    \"path\": \"/etc/custom.conf\",\n                    \"content\": \"test\"\n                }\n            }\n        ]\n    }";

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::MalformedJson(_))),
        "Zero-width character in variant name must fail variant matching: {res:?}"
    );
}

#[test]
fn test_adv_04_deeply_nested_json_depth_bomb_rejected() {
    let mut payload = String::from("{\"intent\": ");
    for _ in 0..100 {
        payload.push_str("{\"nested\": ");
    }
    payload.push_str("\"payload\"");
    for _ in 0..100 {
        payload.push('}');
    }
    payload.push_str(", \"summary\": \"bomb\", \"steps\": []}");

    let res = PlanDraftValidator::validate_and_convert(&payload);
    assert!(
        matches!(res, Err(ValidationError::MalformedJson(_))),
        "Deeply nested JSON depth bomb must fail schema parsing: {res:?}"
    );
}

#[test]
fn test_adv_05_overlong_intent_string_rejected() {
    let overlong_intent = "A".repeat(1025);
    let payload = format!(
        r#"{{
        "intent": "{overlong_intent}",
        "summary": "Valid summary",
        "steps": [
            {{
                "step_id": "s1",
                "description": "ping",
                "capability": {{ "kind": "Ping" }}
            }}
        ]
    }}"#
    );

    let res = PlanDraftValidator::validate_and_convert(&payload);
    assert!(
        matches!(res, Err(ValidationError::SemanticConstraintViolation(ref msg)) if msg.contains("intent exceeds maximum length")),
        "Intent string >1024 chars must be rejected by semantic validator: {res:?}"
    );
}

#[test]
fn test_adv_06_overlong_summary_string_rejected() {
    let overlong_summary = "B".repeat(4097);
    let payload = format!(
        r#"{{
        "intent": "Valid intent",
        "summary": "{overlong_summary}",
        "steps": [
            {{
                "step_id": "s1",
                "description": "ping",
                "capability": {{ "kind": "Ping" }}
            }}
        ]
    }}"#
    );

    let res = PlanDraftValidator::validate_and_convert(&payload);
    assert!(
        matches!(res, Err(ValidationError::SemanticConstraintViolation(ref msg)) if msg.contains("summary exceeds maximum length")),
        "Summary string >4096 chars must be rejected by semantic validator: {res:?}"
    );
}

#[test]
fn test_adv_07_excessive_step_count_rejected() {
    let mut steps_json = Vec::new();
    for i in 0..51 {
        steps_json.push(format!(
            r#"{{ "step_id": "step-{i}", "description": "ping", "capability": {{ "kind": "Ping" }} }}"#
        ));
    }
    let payload = format!(
        r#"{{
        "intent": "Too many steps",
        "summary": "Over 50 steps",
        "steps": [{}]
    }}"#,
        steps_json.join(",")
    );

    let res = PlanDraftValidator::validate_and_convert(&payload);
    assert!(
        matches!(res, Err(ValidationError::SemanticConstraintViolation(ref msg)) if msg.contains("exceeds maximum ceiling of 50 steps")),
        "Step count >50 must be rejected: {res:?}"
    );
}

#[test]
fn test_adv_08_out_of_range_high_timeout_rejected() {
    let payload = r#"{
        "intent": "Tune parameter",
        "summary": "Sets param",
        "steps": [
            {
                "step_id": "s1",
                "description": "ping",
                "timeout_secs": 600,
                "capability": { "kind": "Ping" }
            }
        ]
    }"#;

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::SemanticConstraintViolation(ref msg)) if msg.contains("must be between 1 and 300 seconds")),
        "Timeout >300s must be rejected: {res:?}"
    );
}

#[test]
fn test_adv_09_out_of_range_zero_timeout_rejected() {
    let payload = r#"{
        "intent": "Tune parameter",
        "summary": "Sets param",
        "steps": [
            {
                "step_id": "s1",
                "description": "ping",
                "timeout_secs": 0,
                "capability": { "kind": "Ping" }
            }
        ]
    }"#;

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::SemanticConstraintViolation(ref msg)) if msg.contains("must be between 1 and 300 seconds")),
        "Timeout of 0s must be rejected: {res:?}"
    );
}

#[test]
fn test_adv_10_empty_intent_rejected() {
    let payload = r#"{
        "intent": "   ",
        "summary": "Valid summary",
        "steps": [
            {
                "step_id": "s1",
                "description": "ping",
                "capability": { "kind": "Ping" }
            }
        ]
    }"#;

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::SemanticConstraintViolation(ref msg)) if msg.contains("intent cannot be empty")),
        "Empty intent must be rejected: {res:?}"
    );
}

#[test]
fn test_adv_11_empty_step_id_rejected() {
    let payload = r#"{
        "intent": "Valid intent",
        "summary": "Valid summary",
        "steps": [
            {
                "step_id": "",
                "description": "ping",
                "capability": { "kind": "Ping" }
            }
        ]
    }"#;

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::SemanticConstraintViolation(ref msg)) if msg.contains("empty step_id")),
        "Empty step_id must be rejected: {res:?}"
    );
}

#[test]
fn test_adv_12_array_type_confusion_in_path_field_rejected() {
    let payload = r#"{
        "intent": "Write files",
        "summary": "Array confusion",
        "steps": [
            {
                "step_id": "s1",
                "description": "write",
                "capability": {
                    "kind": "FileWrite",
                    "path": ["/etc/shadow", "/etc/passwd"],
                    "content": "evil"
                }
            }
        ]
    }"#;

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::MalformedJson(_))),
        "Array passed for string path field must fail deserialization: {res:?}"
    );
}

#[test]
fn test_adv_13_object_type_confusion_in_steps_field_rejected() {
    let payload = r#"{
        "intent": "Execute single step",
        "summary": "Object confusion",
        "steps": {
            "step_id": "s1",
            "description": "ping",
            "capability": { "kind": "Ping" }
        }
    }"#;

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::MalformedJson(_))),
        "Object passed for steps array field must fail deserialization: {res:?}"
    );
}

#[test]
fn test_adv_14_null_injection_in_path_field_rejected() {
    let payload = r#"{
        "intent": "Write file",
        "summary": "Null path",
        "steps": [
            {
                "step_id": "s1",
                "description": "write",
                "capability": {
                    "kind": "FileWrite",
                    "path": null,
                    "content": "data"
                }
            }
        ]
    }"#;

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::MalformedJson(_))),
        "Null value in required string field 'path' must fail deserialization: {res:?}"
    );
}

#[test]
fn test_adv_15_null_injection_in_package_field_rejected() {
    let payload = r#"{
        "intent": "Install package",
        "summary": "Null package",
        "steps": [
            {
                "step_id": "s1",
                "description": "install",
                "capability": {
                    "kind": "PackageInstall",
                    "package": null
                }
            }
        ]
    }"#;

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::MalformedJson(_))),
        "Null value in required string field 'package' must fail deserialization: {res:?}"
    );
}

#[test]
fn test_adv_16_null_injection_in_intent_field_rejected() {
    let payload = r#"{
        "intent": null,
        "summary": "Null intent",
        "steps": [
            {
                "step_id": "s1",
                "description": "ping",
                "capability": { "kind": "Ping" }
            }
        ]
    }"#;

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::MalformedJson(_))),
        "Null value in required string field 'intent' must fail deserialization: {res:?}"
    );
}

#[test]
fn test_adv_17_boolean_type_confusion_in_package_field_rejected() {
    let payload = r#"{
        "intent": "Remove package",
        "summary": "Boolean confusion",
        "steps": [
            {
                "step_id": "s1",
                "description": "remove",
                "capability": {
                    "kind": "PackageRemove",
                    "package": true
                }
            }
        ]
    }"#;

    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::MalformedJson(_))),
        "Boolean value for string field 'package' must fail deserialization: {res:?}"
    );
}

#[test]
fn test_adv_18_forbidden_null_byte_injection_rejected() {
    let payload = "{\x00\"intent\": \"clean\", \"summary\": \"clean\", \"steps\": []}";
    let res = PlanDraftValidator::validate_and_convert(payload);
    assert!(
        matches!(res, Err(ValidationError::IllegalCharacters(_))),
        "Forbidden null byte must be rejected immediately at Layer 1: {res:?}"
    );
}

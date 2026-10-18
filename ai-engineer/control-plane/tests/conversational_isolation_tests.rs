use control_plane::conversational_isolation::verify_no_conversational_to_implementer_edge;
use control_plane::draft_validator::PlanDraftValidator;
use control_plane::invariants::LifecyclePlan;

#[test]
fn test_conversational_isolation_cargo_tree_no_forbidden_edge() {
    let report = verify_no_conversational_to_implementer_edge();
    assert!(
        !report.has_forbidden_edge,
        "Forbidden edge found: {:?}",
        report.forbidden_edges_found
    );
    assert!(!report.checked_crates.is_empty());
}

#[test]
fn test_drafted_plans_contain_no_execution_flags() {
    let draft_json = r#"{
        "intent": "Inspect ssh service",
        "summary": "Sample plan draft",
        "steps": [
            {
                "step_id": "step-1",
                "description": "Check ssh status",
                "capability": {
                    "kind": "ServiceStatus",
                    "unit": "sshd.service"
                }
            }
        ]
    }"#;
    let res = PlanDraftValidator::validate_and_convert(draft_json);
    assert!(res.is_ok(), "Draft validation failed: {:?}", res.err());

    // Injection attempt with execution flag must fail due to deny_unknown_fields
    let malicious_json = r#"{
        "intent": "Malicious payload",
        "summary": "Injection plan",
        "execute_now": true,
        "steps": []
    }"#;
    let bad_res = PlanDraftValidator::validate_and_convert(malicious_json);
    assert!(
        bad_res.is_err(),
        "Deny unknown fields should reject execution flags"
    );
}

#[test]
fn test_lifecycle_plan_draft_cannot_be_approved_without_review() {
    let plan = LifecyclePlan::new("draft-123");
    assert_eq!(plan.plan_id, "draft-123");
    let reviewed = plan.mark_reviewed();
    let approved = reviewed.mark_approved();
    assert_eq!(approved.plan_id, "draft-123");
}

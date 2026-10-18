use control_plane::invariants::LifecyclePlan;
use unit_name::UnitName;

#[test]
fn test_plan_type_state_progression() {
    let draft = LifecyclePlan::new("plan-100");
    assert_eq!(draft.plan_id, "plan-100");

    let reviewed = draft.mark_reviewed();
    assert_eq!(reviewed.plan_id, "plan-100");

    let approved = reviewed.mark_approved();
    assert_eq!(approved.plan_id, "plan-100");
}

#[test]
fn test_unit_name_newtype_invariants() {
    let u1 = UnitName::new("ssh.service");
    let u2 = UnitName::new("sshd");
    assert_eq!(u1, u2);
    assert_eq!(u1.as_str(), "sshd");

    let u3 = UnitName::new("systemd-udevd.service");
    assert_eq!(u3.as_str(), "systemd-udevd");
}

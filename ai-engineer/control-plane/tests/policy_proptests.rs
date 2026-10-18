use capability_layer::CapabilityKind;
use control_plane::policy::{ApprovalTier, PolicyEngine};
use proptest::prelude::*;

const PROTECTED_UNITS: &[&str] = &[
    "ssh",
    "sshd",
    "ssh.service",
    "sshd.service",
    "dbus",
    "dbus.service",
    "dbus-broker",
    "dbus-broker.service",
    "firewalld",
    "firewalld.service",
    "systemd-udevd",
    "systemd-udevd.service",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_blast_radius_always_bounded_0_to_100(
        path in "[a-zA-Z0-9_/.\\-]{0,32}",
        content in "[a-zA-Z0-9_/.\\-]{0,32}",
        append in any::<bool>(),
        unit in "[a-zA-Z0-9_/.\\-]{0,32}",
        package in "[a-zA-Z0-9_/.\\-]{0,32}",
        approved in any::<bool>(),
        key in "[a-zA-Z0-9_/.\\-]{0,32}",
        val in "[a-zA-Z0-9_/.\\-]{0,32}",
        slot in any::<u32>(),
    ) {
        let caps = vec![
            CapabilityKind::Ping,
            CapabilityKind::FileRead { path: path.clone() },
            CapabilityKind::FileWrite { path: path.clone(), content, append },
            CapabilityKind::FileDelete { path: path.clone() },
            CapabilityKind::ServiceStatus { unit: unit.clone() },
            CapabilityKind::ServiceRestart { unit: unit.clone(), approved },
            CapabilityKind::ServiceStop { unit: unit.clone(), approved },
            CapabilityKind::PackageInstall { package: package.clone(), approved },
            CapabilityKind::PackageRemove { package, approved },
            CapabilityKind::KernelModuleLoad { module: unit.clone(), args: Vec::new(), approved },
            CapabilityKind::KernelModuleUnload { module: unit, approved },
            CapabilityKind::KernelParamSet { key, value: val, persistent: append, approved },
            CapabilityKind::BootSlotPin { slot, approved },
            CapabilityKind::BootcRollback,
        ];

        for cap in caps {
            let report = PolicyEngine::evaluate_capability(&cap, None);
            prop_assert!(report.score <= 100);
            prop_assert!(report.tier >= ApprovalTier::Auto);
            prop_assert!(report.tier <= ApprovalTier::ExpertReview);
        }
    }

    #[test]
    fn prop_higher_scores_never_produce_lower_tier(s1 in 0u32..=100, s2 in 0u32..=100) {
        if s1 <= s2 {
            prop_assert!(ApprovalTier::from_score(s1) <= ApprovalTier::from_score(s2));
        } else {
            prop_assert!(ApprovalTier::from_score(s1) >= ApprovalTier::from_score(s2));
        }
    }

    #[test]
    fn prop_phase3_kernel_boot_actions_never_below_notify_floor(
        name in "[a-zA-Z0-9_/.\\-]{0,32}",
        val in "[a-zA-Z0-9_/.\\-]{0,32}",
        slot in any::<u32>(),
        approved in any::<bool>(),
        flag in any::<bool>(),
    ) {
        let caps = vec![
            CapabilityKind::KernelModuleLoad { module: name.clone(), args: Vec::new(), approved },
            CapabilityKind::KernelModuleUnload { module: name.clone(), approved },
            CapabilityKind::KernelParamSet { key: name, value: val, persistent: flag, approved },
            CapabilityKind::BootSlotPin { slot, approved },
        ];

        for cap in caps {
            let report = PolicyEngine::evaluate_capability(&cap, None);
            prop_assert!(report.tier >= ApprovalTier::Notify);
            prop_assert!(report.score >= 20);
        }
    }

    #[test]
    fn prop_protected_units_never_reach_auto_tier(
        idx in 0..PROTECTED_UNITS.len(),
        approved in any::<bool>(),
    ) {
        let unit = PROTECTED_UNITS[idx].to_string();

        let stop_report = PolicyEngine::evaluate_capability(
            &CapabilityKind::ServiceStop { unit: unit.clone(), approved },
            None,
        );
        prop_assert!(stop_report.tier > ApprovalTier::Auto);
        prop_assert!(stop_report.tier >= ApprovalTier::Confirm);

        let restart_report = PolicyEngine::evaluate_capability(
            &CapabilityKind::ServiceRestart { unit, approved },
            None,
        );
        prop_assert!(restart_report.tier > ApprovalTier::Auto);
        prop_assert!(restart_report.tier >= ApprovalTier::Confirm);
    }
}

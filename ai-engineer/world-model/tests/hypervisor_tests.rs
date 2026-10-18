use world_model::hypervisor::{
    detect_hypervisor_pure, parse_detect_virt_output, parse_dmi_strings, parse_hostnamectl_output,
    HypervisorPlatform,
};

#[test]
fn test_parse_detect_virt_all_platforms() {
    assert_eq!(parse_detect_virt_output("kvm"), HypervisorPlatform::Kvm);
    assert_eq!(parse_detect_virt_output("qemu"), HypervisorPlatform::Qemu);
    assert_eq!(parse_detect_virt_output("vmware"), HypervisorPlatform::Vmware);
    assert_eq!(parse_detect_virt_output("microsoft"), HypervisorPlatform::Hyperv);
    assert_eq!(parse_detect_virt_output("hyper-v"), HypervisorPlatform::Hyperv);
    assert_eq!(parse_detect_virt_output("oracle"), HypervisorPlatform::Virtualbox);
    assert_eq!(parse_detect_virt_output("virtualbox"), HypervisorPlatform::Virtualbox);
    assert_eq!(parse_detect_virt_output("none"), HypervisorPlatform::None);
    assert_eq!(parse_detect_virt_output("xen_unknown"), HypervisorPlatform::Unknown);
    assert_eq!(parse_detect_virt_output(""), HypervisorPlatform::Unknown);
}

#[test]
fn test_qemu_fixture_detection() {
    let report = detect_hypervisor_pure(
        Some("qemu"),
        Some("QEMU"),
        Some("Standard PC (Q35 + ICH9, 2009)"),
        Some("flags: fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat pse36 clflush mmx fxsr sse sse2 ss syscall nx pdpe1gb rdtscp lm constant_tsc rep_good nopl xtopology cpuid tsc_known_freq pni pclmulqdq ssse3 fma cx16 pcid sse4_1 sse4_2 x2apic movbe popcnt tsc_deadline_timer aes xsave avx f16c rdrand hypervisor lahf_lm abm 3dnowprefetch cpuid_fault invpcid_single pti fsgsbase tsc_adjust bmi1 avx2 smep bmi2 erms invpcid rdseed adx smap clflushopt xsaveopt xsavec xgetbv1 xsaves arat md_clear"),
    );

    assert_eq!(report.platform, HypervisorPlatform::Qemu);
    assert!(report.is_virtualized);
    assert_eq!(report.primary_source, "systemd-detect-virt");
}

#[test]
fn test_vmware_real_bundle_fixture_detection() {
    // Real bundle content from vmware-forensics/extracted/directioner-vmware-final-20260927T053641Z/01_system_identity.txt
    let identity_txt = r#"# UTC: 2026-09-27T05:36:41+00:00
# CMD: hostnamectl

   Static hostname: (unset)
Transient hostname: fedora
         Icon name: computer-vm
           Chassis: vm 🖴
        Machine ID: ac789ec2908d4577aa285c1b14a27de4
           Boot ID: c498d1d77c3646e089360265d7f19fb0
      Product UUID: bbd64d56-6c20-e2eb-40a5-d96953614052
    Virtualization: vmware
  Operating System: Fedora Linux 42 (Adams)
       CPE OS Name: cpe:/o:fedoraproject:fedora:42
    OS Support End: Wed 2026-05-13
OS Support Expired: 4month 2w 1d
            Kernel: Linux 6.19.14-101.fc42.x86_64
      Architecture: x86-64
   Hardware Vendor: VMware, Inc.
    Hardware Model: VMware Virtual Platform
   Hardware Serial: VMware-56 4d d6 bb 20 6c eb e2-40 a5 d9 69 53 61 40 52
  Firmware Version: 6.00
     Firmware Date: Tue 2026-02-17
      Firmware Age: 7month 1w 2d
"#;

    let platform = parse_hostnamectl_output(identity_txt);
    assert_eq!(platform, HypervisorPlatform::Vmware);

    let dmi_platform = parse_dmi_strings("VMware, Inc.", "VMware Virtual Platform");
    assert_eq!(dmi_platform, Some(HypervisorPlatform::Vmware));
}

#[test]
fn test_hyperv_and_virtualbox_dmi_detection() {
    assert_eq!(
        parse_dmi_strings("Microsoft Corporation", "Virtual Machine"),
        Some(HypervisorPlatform::Hyperv)
    );
    assert_eq!(
        parse_dmi_strings("innotek GmbH", "VirtualBox"),
        Some(HypervisorPlatform::Virtualbox)
    );
}

#[test]
fn test_bare_metal_detection() {
    let report = detect_hypervisor_pure(
        Some("none"),
        Some("Dell Inc."),
        Some("PowerEdge R650"),
        Some("flags: fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov"),
    );

    assert_eq!(report.platform, HypervisorPlatform::None);
    assert!(!report.is_virtualized);
    assert_eq!(report.primary_source, "systemd-detect-virt");
}

#[test]
fn test_unknown_hypervisor_reported_as_unknown() {
    let report = detect_hypervisor_pure(
        None,
        Some("Generic Proprietary Virt Vendor"),
        Some("Virtual Hypervisor Board"),
        Some("flags: fpu hypervisor"),
    );

    // Hypervisor bit is present, but vendor not recognized -> must report Unknown
    assert_eq!(report.platform, HypervisorPlatform::Unknown);
    assert!(report.is_virtualized);
    assert_eq!(report.primary_source, "cpuinfo_flag");
}

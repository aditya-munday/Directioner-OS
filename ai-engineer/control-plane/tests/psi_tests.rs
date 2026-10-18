use control_plane::psi::{
    evaluate_psi, read_psi_snapshot_from_strings, PsiPressureLevel, PsiThresholds,
};

#[test]
fn test_evaluate_psi_levels() {
    let thresholds = PsiThresholds::default();

    // Normal pressure
    let normal_snap = read_psi_snapshot_from_strings(
        "some avg10=0.5 avg60=0.2 avg300=0.1 total=100\nfull avg10=0.0 avg60=0.0 avg300=0.0 total=0\n",
        "some avg10=0.0 avg60=0.0 avg300=0.0 total=0\nfull avg10=0.0 avg60=0.0 avg300=0.0 total=0\n",
        "some avg10=0.1 avg60=0.0 avg300=0.0 total=50\nfull avg10=0.0 avg60=0.0 avg300=0.0 total=0\n",
    );
    let sig = evaluate_psi(&normal_snap, &thresholds);
    assert_eq!(sig.overall_level, PsiPressureLevel::None);
    assert_eq!(sig.score_contribution, 0);

    // Elevated memory pressure (avg10 >= 20.0) -> Warn
    let elevated_snap = read_psi_snapshot_from_strings(
        "some avg10=5.0 avg60=2.0 avg300=1.0 total=100\nfull avg10=0.0 avg60=0.0 avg300=0.0 total=0\n",
        "some avg10=25.0 avg60=10.0 avg300=5.0 total=5000\nfull avg10=0.0 avg60=0.0 avg300=0.0 total=0\n",
        "some avg10=1.0 avg60=0.0 avg300=0.0 total=50\nfull avg10=0.0 avg60=0.0 avg300=0.0 total=0\n",
    );
    let elevated_sig = evaluate_psi(&elevated_snap, &thresholds);
    assert_eq!(elevated_sig.overall_level, PsiPressureLevel::Warn);
    assert!(elevated_sig.score_contribution < 0);

    // Critical CPU pressure (full avg10 >= 5.0) -> Critical
    let crit_snap = read_psi_snapshot_from_strings(
        "some avg10=75.0 avg60=50.0 avg300=30.0 total=900000\nfull avg10=10.0 avg60=8.0 avg300=5.0 total=1000\n",
        "some avg10=0.0 avg60=0.0 avg300=0.0 total=0\nfull avg10=0.0 avg60=0.0 avg300=0.0 total=0\n",
        "some avg10=0.0 avg60=0.0 avg300=0.0 total=0\nfull avg10=0.0 avg60=0.0 avg300=0.0 total=0\n",
    );
    let crit_sig = evaluate_psi(&crit_snap, &thresholds);
    assert_eq!(crit_sig.overall_level, PsiPressureLevel::Critical);
    assert!(crit_sig.score_contribution <= -25);
}

#[test]
fn test_evaluate_psi_unavailable_kernel() {
    let thresholds = PsiThresholds::default();
    let unavail_snap = read_psi_snapshot_from_strings("", "", "");
    let sig = evaluate_psi(&unavail_snap, &thresholds);
    assert_eq!(sig.overall_level, PsiPressureLevel::Unavailable);
    assert!(sig.summary.contains("not available"));
}

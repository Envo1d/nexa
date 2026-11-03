use super::glow_amount;

#[test]
fn shell_glow_is_continuous_centered_and_fades_into_the_base() {
    assert!(glow_amount(0.4, 0.0) > glow_amount(0.4, 0.2));
    assert!(glow_amount(0.4, 0.2) > glow_amount(0.4, 0.4));
    assert_eq!(glow_amount(0.4, 1.0), 0.0);
    assert!((glow_amount(0.2, 0.1) - glow_amount(0.6, 0.1)).abs() < 0.0001);

    // No special cutoff at the 56px caption boundary in a 1000px shell.
    let before = glow_amount(0.4, 0.0559);
    let after = glow_amount(0.4, 0.0561);
    assert!((before - after).abs() < 0.001);
}

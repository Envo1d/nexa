use super::Diagnostics;

#[test]
fn observed_decoder_and_unavailable_state_remain_distinct() {
    let mut metrics = Diagnostics::default();
    for value in ["no", "d3d11va", "d3d11va-copy"] {
        metrics.update(1, Some(value.into()));
        assert!(
            metrics
                .summary()
                .contains(&format!("hwdec-current={value}"))
        );
    }
    metrics.update(0, Some("bad".into()));
    metrics.update(u64::MAX, Some("bad".into()));
    assert!(!metrics.summary().contains("bad"));
    metrics.update(1, None);
    assert!(metrics.summary().contains("hwdec-current=unknown"));
    metrics.update(7, Some("12".into()));
    metrics.clear();
    assert!(metrics.summary().contains("frame-drop-count=unknown"));
}

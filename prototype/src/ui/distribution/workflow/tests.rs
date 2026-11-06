use super::advance_position;

#[test]
fn finishing_the_whole_batch_preserves_the_range_that_close_commits() {
    let mut position = 0;
    assert!(!advance_position(&mut position, 4, 4));
    assert_eq!(position, 0);
    assert_eq!(position + 4, 4);
}

#[test]
fn finishing_the_last_file_after_advancing_does_not_count_it_twice() {
    let mut position = 0;
    assert!(advance_position(&mut position, 1, 3));
    assert!(advance_position(&mut position, 1, 3));
    assert!(!advance_position(&mut position, 1, 3));
    assert_eq!(position, 2);
    assert_eq!(position + 1, 3);
}

use super::format_size;

#[test]
fn sizes_follow_reference_megabyte_and_gigabyte_units() {
    assert_eq!(format_size(0), "0.00 МБ");
    assert_eq!(format_size(512 * 1024), "0.50 МБ");
    assert_eq!(format_size(3 * 1024 * 1024), "3.00 МБ");
    assert_eq!(format_size(1024 * 1024 * 1024), "1.00 ГБ");
}

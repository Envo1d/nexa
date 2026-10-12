//! Stable shared domain name normalization and natural filename ordering.

use unicode_normalization::UnicodeNormalization;

/// Version 1: canonical decomposition, full default Unicode case folding, NFC.
/// Preserve punctuation/accents/internal spacing; trim surrounding whitespace.
pub(crate) fn name_key(value: &str) -> String {
    let decomposed: String = value.trim().nfd().collect();
    caseless::default_case_fold_str(&decomposed).nfc().collect()
}

/// Natural, Unicode-insensitive filename ordering; directory prefixes do not affect names.
pub(crate) fn compare_names(a: &str, b: &str) -> std::cmp::Ordering {
    let a = name_key(a.rsplit(['/', '\\']).next().unwrap_or(a));

    let b = name_key(b.rsplit(['/', '\\']).next().unwrap_or(b));

    let mut a = a.as_str();

    let mut b = b.as_str();

    while !a.is_empty() && !b.is_empty() {
        let ac = a.chars().next().unwrap();

        let bc = b.chars().next().unwrap();

        if ac.is_ascii_digit() && bc.is_ascii_digit() {
            let an = a.bytes().take_while(u8::is_ascii_digit).count();

            let bn = b.bytes().take_while(u8::is_ascii_digit).count();

            let av = a[..an].trim_start_matches('0');

            let bv = b[..bn].trim_start_matches('0');

            let order = av.len().cmp(&bv.len()).then_with(|| av.cmp(bv));

            if !order.is_eq() {
                return order;
            }
            a = &a[an..];
            b = &b[bn..];
        } else {
            let order = ac.cmp(&bc);

            if !order.is_eq() {
                return order;
            }
            a = &a[ac.len_utf8()..];
            b = &b[bc.len_utf8()..];
        }
    }
    a.len().cmp(&b.len())
}

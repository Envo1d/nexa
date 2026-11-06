use super::*;

#[test]
fn resolves_explicit_and_system_language_with_english_fallback() {
    assert_eq!(resolve(1, "ru-RU"), "en");
    assert_eq!(resolve(2, "en-US"), "ru");
    assert_eq!(resolve(0, "RU_ru"), "ru");
    assert_eq!(resolve(0, "uk-UA"), "en");
    assert_eq!(resolve(0, ""), "en");
}

#[test]
fn catalogs_retain_reference_keys_and_gpui_russian_copy() {
    let ru: HashMap<String, String> =
        serde_json::from_str(include_str!("../../locales/ru.json")).unwrap();
    let en: HashMap<String, String> =
        serde_json::from_str(include_str!("../../locales/en.json")).unwrap();
    assert_eq!(ru.len(), en.len());
    assert!(ru.keys().all(|key| en.contains_key(key)));
    ENGLISH.store(true, Ordering::Relaxed);
    assert_eq!(text("Художник"), "Artist");
    assert_eq!(text("Произведения"), "Copyrights");
    assert_eq!(text("User metadata"), "User metadata");
    assert!(std::thread::spawn(english).join().unwrap());
    ENGLISH.store(false, Ordering::Relaxed);
    assert_eq!(text("Художник"), "Художник");
    assert!(!std::thread::spawn(english).join().unwrap());
}

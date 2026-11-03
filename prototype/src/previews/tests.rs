use super::*;

#[cfg(windows)]
#[test]
fn prepares_shell_thumbnail_from_an_mta_caller_without_changing_source() {
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

    struct Mta;

    impl Drop for Mta {
        fn drop(&mut self) {
            // SAFETY: the test initialized COM successfully on this same thread.
            unsafe { CoUninitialize() };
        }
    }

    thread::spawn(|| {
        // SAFETY: this fresh test thread has no prior COM apartment.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .unwrap();
        let _apartment = Mta;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("превью.bmp");
        let source = image::RgbImage::from_pixel(32, 32, image::Rgb([230, 70, 20]));
        source
            .save_with_format(&path, image::ImageFormat::Bmp)
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();

        let prepared = prepare(vec![path.clone()]).unwrap();

        assert_eq!(prepared.entries.len(), 1);
        assert_eq!(prepared.previews.len(), 1);
        let preview = prepared.previews[&0].as_bytes(0).unwrap();
        assert_eq!(&preview[..4], &[20, 70, 230, 255]);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    })
    .join()
    .unwrap();
}

#[test]
fn caps_preparation_and_ignores_unsupported_entries() {
    let mut paths = vec![PathBuf::from("unsupported.txt"); catalog::MAX_OPEN_FILES];
    paths.push(PathBuf::from("beyond-limit.png"));

    let prepared = prepare(paths).unwrap();

    assert!(prepared.entries.is_empty());
    assert!(prepared.previews.is_empty());
}

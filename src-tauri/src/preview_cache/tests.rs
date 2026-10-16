use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use super::*;

fn fixture() -> (tempfile::TempDir, PathBuf, PreviewCache) {
    let temporary = tempfile::tempdir().unwrap();

    let source = temporary.path().join("媒体 source.bin");
    fs::write(&source, b"original").unwrap();

    let cache = PreviewCache::new(temporary.path().join("cache"));
    (temporary, source, cache)
}

fn pixels(_: &Path, size: u32) -> Result<RgbImage, ThumbnailError> {
    Ok(RgbImage::from_pixel(
        size.min(4),
        size.min(3),
        image::Rgb([12, 34, 56]),
    ))
}

#[test]
fn lossless_encoding_preserves_varied_rgb_pixels() {
    let image = RgbImage::from_fn(31, 29, |x, y| {
        image::Rgb([
            (x * 7) as u8,
            (y * 8) as u8,
            ((x * 13 + y * 17) % 256) as u8,
        ])
    });

    let bytes = encode_preview(&image).unwrap();
    assert_eq!(&bytes[12..16], b"VP8L");

    let decoded = image::load_from_memory_with_format(&bytes, ImageFormat::WebP)
        .unwrap()
        .into_rgb8();
    assert_eq!(decoded, image);
}

#[test]
fn keys_are_deterministic_full_width_sha256_and_maintenance_owns_all_versions() {
    let (_temporary, source, cache) = fixture();

    let key = CacheKey::from_source(&source, 32).unwrap();
    assert_eq!(
        key.filename,
        CacheKey::from_source(&source, 32).unwrap().filename
    );
    assert!(key.filename.starts_with("v4-"));
    assert_eq!(key.filename.len(), 3 + 64 + 5);
    cache.load_with(&source, 32, pixels).unwrap();

    for version in ["v1", "v2"] {
        fs::write(
            cache
                .root
                .join(format!("{version}-{}.webp", "a".repeat(64))),
            b"old",
        )
        .unwrap();
    }

    fs::write(
        cache.root.join(format!("v3-{}.webp", "a".repeat(32))),
        b"old",
    )
    .unwrap();

    let unrelated = [
        format!("v3-{}.webp", "b".repeat(64)),
        format!("v2-{}.webp", "b".repeat(32)),
        format!("v3-{}.webp", "G".repeat(32)),
        format!("v4-{}.webp", "b".repeat(32)),
        format!("v5-{}.webp", "b".repeat(64)),
    ];

    for name in &unrelated {
        fs::write(cache.root.join(name), b"keep").unwrap();
    }
    assert_eq!(cache.usage().unwrap().entries, 4);
    assert_eq!(cache.clear().unwrap().removed.entries, 4);

    for name in unrelated {
        assert!(cache.root.join(name).exists());
    }
}

#[test]
fn round_trip_hits_without_generation_and_keeps_source() {
    let (_temporary, source, cache) = fixture();
    assert!(!cache.root.exists());

    let first = cache.load_with(&source, 32, pixels).unwrap();
    assert_eq!(first.origin, PreviewOrigin::Generated);
    assert!(first.persistence_error.is_none());

    let second = cache
        .load_with(&source, 32, |_, _| panic!("hit must not extract"))
        .unwrap();
    assert_eq!(second.origin, PreviewOrigin::Cache);
    assert_eq!(first.image, second.image);
    assert_eq!(fs::read(source).unwrap(), b"original");
    assert_eq!(cache.usage().unwrap().entries, 1);
    assert_eq!(fs::read_dir(&cache.root).unwrap().count(), 1);
}

#[test]
fn keys_distinguish_size_path_length_and_subsecond_revision() {
    let (_temporary, source, _cache) = fixture();

    let base = CacheKey::from_source(&source, 32).unwrap();
    assert_ne!(
        base.filename,
        CacheKey::from_source(&source, 64).unwrap().filename
    );

    let mut changed = base.revision.clone();
    // Windows SystemTime has 100 ns resolution; smaller increments round away.
    changed.modified += Duration::from_nanos(100);
    assert_ne!(
        base.filename,
        CacheKey::new(&source, changed, 32).unwrap().filename
    );

    let mut changed = base.revision.clone();
    changed.length += 1;
    assert_ne!(
        base.filename,
        CacheKey::new(&source, changed, 32).unwrap().filename
    );
    assert_ne!(
        base.filename,
        CacheKey::new(&source.with_extension("other"), base.revision, 32)
            .unwrap()
            .filename
    );
    assert!(matches!(
        CacheKey::from_source(&source, 0),
        Err(CacheError::InvalidSize)
    ));
    assert!(matches!(
        CacheKey::from_source(&source, u32::MAX),
        Err(CacheError::InvalidSize)
    ));
}

#[test]
fn corrupt_and_oversized_entries_regenerate_atomically() {
    let (_temporary, source, cache) = fixture();
    cache.load_with(&source, 32, pixels).unwrap();

    let key = CacheKey::from_source(&source, 32).unwrap();
    fs::write(cache.root.join(&key.filename), b"invalid WebP").unwrap();
    assert_eq!(
        cache.load_with(&source, 32, pixels).unwrap().origin,
        PreviewOrigin::Generated
    );

    let oversized = RgbImage::new(33, 1);
    cache.write(&key, &oversized).unwrap();
    assert_eq!(
        cache.load_with(&source, 32, pixels).unwrap().origin,
        PreviewOrigin::Generated
    );

    let file = File::create(cache.root.join(&key.filename)).unwrap();
    file.set_len(MAX_ENCODED_BYTES + 1).unwrap();
    drop(file);
    assert_eq!(
        cache.load_with(&source, 32, pixels).unwrap().origin,
        PreviewOrigin::Generated
    );
    assert_eq!(fs::read_dir(&cache.root).unwrap().count(), 1);
}

#[test]
fn persistence_failure_returns_usable_preview_and_diagnostic() {
    let (_temporary, source, cache) = fixture();
    fs::write(&cache.root, b"blocked cache directory").unwrap();

    let result = cache.load_with(&source, 32, pixels).unwrap();
    assert_eq!(result.origin, PreviewOrigin::Generated);
    assert_eq!(result.image, pixels(&source, 32).unwrap());
    assert!(matches!(result.persistence_error, Some(CacheError::Io(_))));
    assert_eq!(fs::read(&cache.root).unwrap(), b"blocked cache directory");
}

#[test]
fn rejects_changed_source_invalid_pixels_and_provider_failure() {
    let (_temporary, source, cache) = fixture();

    let result = cache.load_with(&source, 32, |path, size| {
        fs::write(path, b"different source length").unwrap();
        pixels(path, size)
    });
    assert!(matches!(result, Err(CacheError::SourceChanged)));
    assert!(!cache.root.exists());
    assert!(matches!(
        cache.load_with(&source, 32, |_, _| Ok(RgbImage::new(33, 1))),
        Err(CacheError::InvalidImage)
    ));
    assert!(matches!(
        cache.load_with(&source, 32, |_, _| Err(ThumbnailError::InvalidBitmap)),
        Err(CacheError::Thumbnail(_))
    ));
    assert!(matches!(
        cache.load_with(source.parent().unwrap(), 32, pixels),
        Err(CacheError::NotAFile)
    ));
}

#[test]
fn remaps_after_rename_and_preserves_existing_target() {
    let (_temporary, source, cache) = fixture();
    cache.load_with(&source, 32, pixels).unwrap();

    let old = CacheKey::from_source(&source, 32).unwrap();

    let destination = source.with_extension("renamed");
    fs::rename(&source, &destination).unwrap();

    let new = CacheKey::from_source(&destination, 32).unwrap();
    assert_eq!(cache.remap(&old, &new).unwrap(), RemapOutcome::Moved);
    assert_eq!(
        cache
            .load_with(&destination, 32, |_, _| panic!("remap must hit"))
            .unwrap()
            .origin,
        PreviewOrigin::Cache
    );
    assert_eq!(
        cache.remap(&old, &new).unwrap(),
        RemapOutcome::SourceMissing
    );
    cache
        .write(&old, &pixels(&destination, 32).unwrap())
        .unwrap();

    let target = fs::read(cache.root.join(&new.filename)).unwrap();
    assert_eq!(cache.remap(&old, &new).unwrap(), RemapOutcome::TargetExists);
    assert_eq!(fs::read(cache.root.join(&new.filename)).unwrap(), target);
    assert!(cache.root.join(&old.filename).exists());

    let other_size = CacheKey::from_source(&destination, 64).unwrap();
    assert!(matches!(
        cache.remap(&old, &other_size),
        Err(CacheError::IncompatibleRemap)
    ));
}

#[test]
fn maintenance_is_oldest_first_and_preserves_unowned_entries() {
    let (_temporary, source, cache) = fixture();
    assert_eq!(cache.usage().unwrap(), CacheUsage::default());

    for size in [32, 64] {
        cache.load_with(&source, size, pixels).unwrap();
    }

    let old = CacheKey::from_source(&source, 32).unwrap();

    let new = CacheKey::from_source(&source, 64).unwrap();
    File::options()
        .write(true)
        .open(cache.root.join(&old.filename))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(10)))
        .unwrap();
    fs::write(cache.root.join("unrelated.webp"), b"preserve").unwrap();
    fs::write(cache.root.join(".temporary"), b"preserve").unwrap();

    let directory = cache.root.join(format!("v1-{}.webp", "a".repeat(64)));
    fs::create_dir(&directory).unwrap();

    let new_size = fs::metadata(cache.root.join(&new.filename)).unwrap().len();

    let report = cache.prune(new_size).unwrap();
    assert_eq!(report.removed.entries, 1);
    assert!(!cache.root.join(old.filename).exists());
    assert!(cache.root.join(new.filename).exists());
    fs::write(cache.root.join(format!("v1-{}.webp", "b".repeat(64))), []).unwrap();

    let report = cache.clear().unwrap();
    assert_eq!(report.removed.entries, 2);
    assert_eq!(report.remaining, CacheUsage::default());
    assert!(directory.is_dir());
    assert_eq!(
        fs::read(cache.root.join("unrelated.webp")).unwrap(),
        b"preserve"
    );
    assert!(cache.root.join(".temporary").exists());
}

#[test]
fn shared_instance_serializes_concurrent_generation() {
    let (_temporary, source, cache) = fixture();

    let cache = Arc::new(cache);

    let calls = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|scope| {
        let mut handles = Vec::new();

        for _ in 0..4 {
            handles.push(scope.spawn(|| {
                cache
                    .load_with(&source, 32, |path, size| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        pixels(path, size)
                    })
                    .unwrap()
            }));
        }

        for handle in handles {
            assert!(handle.join().unwrap().persistence_error.is_none());
        }
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(cache.usage().unwrap().entries, 1);
}

#[test]
fn different_sources_extract_concurrently_without_global_cache_lock() {
    let (_temporary, source, cache) = fixture();

    let other = source.with_extension("second");
    fs::write(&other, b"source").unwrap();

    let (started_tx, started_rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let mut releases = Vec::new();

        let mut handles = Vec::new();

        for path in [&source, &other] {
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            releases.push(release_tx);

            let started = started_tx.clone();

            let cache = &cache;
            handles.push(scope.spawn(move || {
                cache
                    .load_with(path, 32, |path, size| {
                        started.send(()).unwrap();
                        release_rx.recv().unwrap();
                        pixels(path, size)
                    })
                    .unwrap()
            }));
        }

        let first = started_rx.recv_timeout(std::time::Duration::from_secs(5));

        let second = started_rx.recv_timeout(std::time::Duration::from_secs(5));

        for release in releases {
            release.send(()).unwrap();
        }

        for handle in handles {
            handle.join().unwrap();
        }
        assert!(
            first.is_ok() && second.is_ok(),
            "Extraction was globally serialized"
        );
    });
    assert_eq!(cache.usage().unwrap().entries, 2);
}

#[test]
fn source_revision_change_generates_a_new_entry() {
    let (_temporary, source, cache) = fixture();
    cache.load_with(&source, 32, pixels).unwrap();
    fs::write(&source, b"changed and longer").unwrap();
    assert_eq!(
        cache.load_with(&source, 32, pixels).unwrap().origin,
        PreviewOrigin::Generated
    );
    assert_eq!(cache.usage().unwrap().entries, 2);
    // An obsolete revision remains evictable instead of deleting during lookup.
    assert_eq!(cache.clear().unwrap().removed.entries, 2);
}

#[cfg(windows)]
#[test]
fn shell_thumbnail_round_trips_through_public_cache_api() {
    let temporary = tempfile::tempdir().unwrap();

    let source = temporary.path().join("превью 日本.bmp");

    let mut bytes = vec![0_u8; 62];
    bytes[0..2].copy_from_slice(b"BM");

    for (offset, value) in [(2, 62_u32), (10, 54), (14, 40), (18, 2), (22, 1), (34, 8)] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24_u16.to_le_bytes());
    bytes[54..60].copy_from_slice(&[0, 0, 255, 255, 0, 0]);
    fs::write(&source, &bytes).unwrap();

    let cache = PreviewCache::new(temporary.path().join("cache"));

    let first = cache.load_or_generate(&source, 64).unwrap();
    assert_eq!(first.origin, PreviewOrigin::Generated);
    assert!(first.persistence_error.is_none());
    // A separate instance also reads the persisted entry.
    let reopened = PreviewCache::new(cache.root.clone());

    let second = reopened.load_or_generate(&source, 64).unwrap();
    assert_eq!(second.origin, PreviewOrigin::Cache);
    assert_eq!(first.image, second.image);
    assert_eq!(fs::read(source).unwrap(), bytes);
}

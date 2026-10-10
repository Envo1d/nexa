//! Unused synchronous preview cache. Future callers must use a blocking worker
//! compatible with the Shell STA provider and share one instance per cache root.
//! Construct with the isolated AppPaths.cache_path; construction performs no IO.

use crate::media_thumbnail::{MAX_THUMBNAIL_EDGE, ThumbnailError, get_thumbnail};
use image::{ImageFormat, ImageReader, Limits, RgbImage};
use std::{
    fs::{self, File},
    io::{BufReader, Write},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};
use twox_hash::XxHash3_128;

const MAX_ENCODED_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Cache file operation failed")]
    Io(#[from] std::io::Error),
    #[error("WebP operation failed")]
    Image(#[from] image::ImageError),
    #[error("WebP encoding failed: {0}")]
    Encoding(String),
    #[error("Thumbnail extraction failed")]
    Thumbnail(#[from] ThumbnailError),
    #[error("Thumbnail size must be between 1 and {MAX_THUMBNAIL_EDGE}")]
    InvalidSize,
    #[error("Source must be a regular file")]
    NotAFile,
    #[error("Source changed while obtaining its preview")]
    SourceChanged,
    #[error("Preview dimensions are invalid")]
    InvalidImage,
    #[error("Cache entry is not a bounded regular file")]
    InvalidEntry,
    #[error("Cache operation lock is poisoned")]
    Poisoned,
    #[error("Remapped cache entries must have matching source revision and size")]
    IncompatibleRemap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Revision {
    length: u64,
    modified: SystemTime,
}

fn revision(path: &Path) -> Result<Revision, CacheError> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(CacheError::NotAFile);
    }
    Ok(Revision {
        length: metadata.len(),
        modified: metadata.modified()?,
    })
}

/// Capture before a rename; capture the destination afterwards for `remap`.
/// Identity uses exact absolute native path spelling, not filesystem aliases.
#[derive(Clone, Debug)]
pub struct CacheKey {
    filename: String,
    revision: Revision,
    size: u32,
}

impl CacheKey {
    pub fn from_source(path: &Path, size: u32) -> Result<Self, CacheError> {
        validate_size(size)?;
        Self::new(&std::path::absolute(path)?, revision(path)?, size)
    }

    fn new(path: &Path, revision: Revision, size: u32) -> Result<Self, CacheError> {
        let mut hash = XxHash3_128::with_seed(0);
        hash.write(b"nexa-preview-v3-xxh3-128-rgb-libwebp-lossless-quality75");
        let bytes = path.as_os_str().as_encoded_bytes();
        hash.write(&(bytes.len() as u64).to_le_bytes());
        hash.write(bytes);
        hash.write(&size.to_le_bytes());
        hash.write(&revision.length.to_le_bytes());
        let (negative, duration) = match revision.modified.duration_since(UNIX_EPOCH) {
            Ok(duration) => (0u8, duration),
            Err(error) => (1u8, error.duration()),
        };
        hash.write(&[negative]);
        hash.write(&duration.as_secs().to_le_bytes());
        hash.write(&duration.subsec_nanos().to_le_bytes());
        Ok(Self {
            filename: format!("v3-{:032x}.webp", hash.finish_128()),
            revision,
            size,
        })
    }
}

fn validate_size(size: u32) -> Result<(), CacheError> {
    if (1..=MAX_THUMBNAIL_EDGE).contains(&size) {
        Ok(())
    } else {
        Err(CacheError::InvalidSize)
    }
}

pub(crate) fn encode_preview(image: &RgbImage) -> Result<webp::WebPMemory, CacheError> {
    let _span = tracing::debug_span!("preview_encode", lossless = true, quality = 75).entered();
    tracing::debug!("Preview encoding started");
    let bytes = webp::Encoder::from_rgb(image.as_raw(), image.width(), image.height())
        .encode_simple(true, 75.0)
        .map_err(|error| CacheError::Encoding(format!("{error:?}")))?;
    tracing::debug!(encoded_bytes = bytes.len(), "Preview encoding completed");
    Ok(bytes)
}

fn validate_image(image: &RgbImage, size: u32) -> Result<(), CacheError> {
    if image.width() == 0 || image.height() == 0 || image.width() > size || image.height() > size {
        Err(CacheError::InvalidImage)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewOrigin {
    Cache,
    Generated,
}

/// A write failure is recoverable: the preview remains usable and the caller
/// can display/handle the explicit persistence diagnostic without retrying Shell.
#[derive(Debug)]
pub struct CachedPreview {
    pub image: RgbImage,
    pub origin: PreviewOrigin,
    pub persistence_error: Option<CacheError>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CacheUsage {
    pub entries: u64,
    pub bytes: u64,
}

#[derive(Debug, Default)]
pub struct MaintenanceReport {
    pub removed: CacheUsage,
    pub remaining: CacheUsage,
    pub failed_removals: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemapOutcome {
    Moved,
    SourceMissing,
    TargetExists,
    Unchanged,
}

pub struct PreviewCache {
    root: PathBuf,
    operations: Mutex<()>,
    generations: Mutex<std::collections::HashMap<String, std::sync::Weak<Mutex<()>>>>,
}

impl PreviewCache {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            operations: Mutex::new(()),
            generations: Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, ()>, CacheError> {
        self.operations.lock().map_err(|_| CacheError::Poisoned)
    }

    /// Cache errors fall back to generation. Source/provider errors are fatal.
    #[tracing::instrument(level = "debug", skip_all, fields(requested_edge = size), err)]
    pub fn load_or_generate(&self, path: &Path, size: u32) -> Result<CachedPreview, CacheError> {
        self.load_with(path, size, get_thumbnail)
    }

    fn load_with(
        &self,
        path: &Path,
        size: u32,
        generate: impl FnOnce(&Path, u32) -> Result<RgbImage, ThumbnailError>,
    ) -> Result<CachedPreview, CacheError> {
        let _span = tracing::debug_span!("preview_cache_load", requested_edge = size).entered();
        tracing::debug!("Preview lookup started");
        validate_size(size)?;
        let source = std::path::absolute(path)?;
        let before = revision(&source)?;
        let key = CacheKey::new(&source, before.clone(), size)?;
        // Deduplicate the same source while letting different sources extract in parallel.
        let generation = {
            let mut locks = self.generations.lock().map_err(|_| CacheError::Poisoned)?;
            locks.retain(|_, lock| lock.strong_count() > 0);
            match locks.get(&key.filename).and_then(std::sync::Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    let lock = std::sync::Arc::new(Mutex::new(()));
                    locks.insert(key.filename.clone(), std::sync::Arc::downgrade(&lock));
                    lock
                }
            }
        };
        let _generation = generation.lock().map_err(|_| CacheError::Poisoned)?;
        let cached = {
            let _lock = self.lock()?;
            self.read(&key)
        };
        match cached {
            Ok(Some(image)) => {
                if revision(&source)? != before {
                    return Err(CacheError::SourceChanged);
                }
                tracing::debug!("Preview cache hit");
                return Ok(CachedPreview {
                    image,
                    origin: PreviewOrigin::Cache,
                    persistence_error: None,
                });
            }
            Ok(None) => tracing::debug!("Preview cache miss"),
            Err(_) => tracing::warn!("Cache entry unavailable or invalid; regenerating"),
        }
        let image = generate(&source, size)?;
        validate_image(&image, size)?;
        if revision(&source)? != before {
            return Err(CacheError::SourceChanged);
        }
        // Slow Shell extraction does not hold the shared maintenance/read lock.
        let persistence_error = {
            let _lock = self.lock()?;
            self.write(&key, &image).err()
        };
        if persistence_error.is_some() {
            tracing::warn!("Preview generated but cache persistence failed");
        }
        tracing::debug!(
            persisted = persistence_error.is_none(),
            "Preview generation completed"
        );
        Ok(CachedPreview {
            image,
            origin: PreviewOrigin::Generated,
            persistence_error,
        })
    }

    fn read(&self, key: &CacheKey) -> Result<Option<RgbImage>, CacheError> {
        let path = self.root.join(&key.filename);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !metadata.file_type().is_file() || metadata.len() > MAX_ENCODED_BYTES {
            return Err(CacheError::InvalidEntry);
        }
        let mut reader =
            ImageReader::with_format(BufReader::new(File::open(path)?), ImageFormat::WebP);
        let mut limits = Limits::default();
        limits.max_image_width = Some(key.size);
        limits.max_image_height = Some(key.size);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode()?.into_rgb8();
        validate_image(&image, key.size)?;
        Ok(Some(image))
    }

    fn write(&self, key: &CacheKey, image: &RgbImage) -> Result<(), CacheError> {
        fs::create_dir_all(&self.root)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&self.root)?;
        temporary.write_all(&encode_preview(image)?)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(self.root.join(&key.filename))
            .map_err(|error| CacheError::Io(error.error))?;
        Ok(())
    }

    /// Does not overwrite destination entries, even if they are corrupt. Keys
    /// retain the original revision so removed old source paths need not exist.
    #[tracing::instrument(name = "preview_cache_remap", level = "debug", skip_all, err)]
    pub fn remap(&self, old: &CacheKey, new: &CacheKey) -> Result<RemapOutcome, CacheError> {
        tracing::debug!("Preview cache remap started");
        let _lock = self.lock()?;
        if old.size != new.size || old.revision != new.revision {
            return Err(CacheError::IncompatibleRemap);
        }
        if old.filename == new.filename {
            return Ok(RemapOutcome::Unchanged);
        }
        // Decode first to avoid transferring corrupt or oversized data.
        let Some(image) = self.read(old)? else {
            return Ok(RemapOutcome::SourceMissing);
        };
        let mut temporary = tempfile::NamedTempFile::new_in(&self.root)?;
        temporary.write_all(&encode_preview(&image)?)?;
        temporary.as_file().sync_all()?;
        match temporary.persist_noclobber(self.root.join(&new.filename)) {
            Ok(_) => {}
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Ok(RemapOutcome::TargetExists);
            }
            Err(error) => return Err(CacheError::Io(error.error)),
        }
        fs::remove_file(self.root.join(&old.filename))?;
        tracing::debug!("Preview cache entry remapped");
        Ok(RemapOutcome::Moved)
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn usage(&self) -> Result<CacheUsage, CacheError> {
        let _lock = self.lock()?;
        Ok(self
            .inventory()?
            .iter()
            .fold(CacheUsage::default(), |mut total, entry| {
                total.entries += 1;
                total.bytes = total.bytes.saturating_add(entry.1);
                total
            }))
    }

    /// Evicts by cache write time, not last access. Takes bytes, avoiding MB overflow.
    /// No detached thread: future callers choose a bounded blocking executor.
    #[tracing::instrument(name = "preview_cache_prune", skip_all, fields(max_bytes), err)]
    pub fn prune(&self, max_bytes: u64) -> Result<MaintenanceReport, CacheError> {
        tracing::info!("Cache maintenance started");
        let _lock = self.lock()?;
        let mut files = self.inventory()?;
        files.sort_unstable_by(|a, b| a.2.cmp(&b.2).then(a.0.cmp(&b.0)));
        let mut report = MaintenanceReport::default();
        for (_, bytes, _) in &files {
            report.remaining.entries += 1;
            report.remaining.bytes = report.remaining.bytes.saturating_add(*bytes);
        }
        for (path, bytes, _) in files {
            if max_bytes != 0 && report.remaining.bytes <= max_bytes {
                break;
            }
            match fs::remove_file(path) {
                Ok(()) => {
                    report.remaining.entries -= 1;
                    report.remaining.bytes -= bytes;
                    report.removed.entries += 1;
                    report.removed.bytes += bytes;
                }
                Err(_) => {
                    report.failed_removals += 1;
                    tracing::warn!("Cache entry eviction failed");
                }
            }
        }
        tracing::info!(
            removed = report.removed.entries,
            remaining_bytes = report.remaining.bytes,
            failed_removals = report.failed_removals,
            "Cache maintenance completed"
        );
        Ok(report)
    }

    pub fn clear(&self) -> Result<MaintenanceReport, CacheError> {
        self.prune(0)
    }

    fn inventory(&self) -> Result<Vec<(PathBuf, u64, SystemTime)>, CacheError> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some((hash, expected_length)) = name
                .strip_prefix("v1-")
                .or_else(|| name.strip_prefix("v2-"))
                .map(|hash| (hash, 64))
                .or_else(|| name.strip_prefix("v3-").map(|hash| (hash, 32)))
                .and_then(|(hash, length)| hash.strip_suffix(".webp").map(|hash| (hash, length)))
            else {
                continue;
            };
            if hash.len() != expected_length
                || !hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_file() {
                files.push((entry.path(), metadata.len(), metadata.modified()?));
            }
        }
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

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
    fn keys_are_deterministic_full_width_xxh3_and_maintenance_owns_all_versions() {
        let (_temporary, source, cache) = fixture();
        let key = CacheKey::from_source(&source, 32).unwrap();
        assert_eq!(
            key.filename,
            CacheKey::from_source(&source, 32).unwrap().filename
        );
        assert!(key.filename.starts_with("v3-"));
        assert_eq!(key.filename.len(), 3 + 32 + 5);
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
        let unrelated = [
            format!("v3-{}.webp", "b".repeat(64)),
            format!("v2-{}.webp", "b".repeat(32)),
            format!("v3-{}.webp", "G".repeat(32)),
            format!("v4-{}.webp", "b".repeat(32)),
        ];
        for name in &unrelated {
            fs::write(cache.root.join(name), b"keep").unwrap();
        }
        assert_eq!(cache.usage().unwrap().entries, 3);
        assert_eq!(cache.clear().unwrap().removed.entries, 3);
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
}

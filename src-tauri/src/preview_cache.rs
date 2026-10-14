//! Synchronous preview cache used by bounded Stage/Main blocking workers.
//! Callers must remain compatible with the Shell STA provider and share one instance per cache root.
//! Construct with the isolated AppPaths.cache_path; construction performs no IO.

use std::{
    fs::{self, File},
    io::{BufReader, Write},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

use image::{ImageFormat, ImageReader, Limits, RgbImage};
use twox_hash::XxHash3_128;

use crate::media_thumbnail::{MAX_THUMBNAIL_EDGE, ThumbnailError, get_thumbnail};

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
mod tests;

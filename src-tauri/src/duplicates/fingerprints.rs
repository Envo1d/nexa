use std::{fs::File, io::Read, path::Path};

use image::{RgbImage, imageops::FilterType};
use sha2::{Digest, Sha256};

use crate::{media_paths::checked_action_path, media_thumbnail};

pub(super) fn read(
    root: &Path,
    relative: &str,
    revision: Option<&str>,
    image: bool,
) -> Result<(String, Option<String>, String, bool), String> {
    let path = checked_action_path(root, relative)?;

    let before = identity(&path)?;

    if revision.is_some_and(|revision| revision != before) {
        return Err("File changed; rescan its source".into());
    }

    let mut file = File::open(&path).map_err(|_| "Cannot open file")?;

    let mut hash = Sha256::new();

    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let count = file.read(&mut buffer).map_err(|_| "Cannot hash file")?;

        if count == 0 {
            break;
        }

        hash.update(&buffer[..count]);
    }

    let (visual, retry) = if image { visual(&path) } else { (None, false) };

    // Recheck confinement and both open handle/path identities after thumbnail extraction.
    let checked = checked_action_path(root, relative)?;

    let handle = file.metadata().map_err(|_| "Cannot inspect file")?;

    let handle_identity = format!(
        "{}:{:?}",
        handle.len(),
        handle.modified().map_err(|_| "Cannot inspect revision")?
    );

    if checked != path || identity(&path)? != before || handle_identity != before {
        return Err("File changed during fingerprinting".into());
    }

    let sha256 = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    Ok((sha256, visual, before, retry))
}

pub(super) fn visual(path: &Path) -> (Option<String>, bool) {
    match media_thumbnail::get_thumbnail(path, 256) {
        Ok(image) => (perceptual(&image).map(|hash| format!("{hash:016x}")), false),
        Err(_) => (None, true),
    }
}

pub(super) fn current_revision(
    root: &Path,
    relative: &str,
    expected: Option<&str>,
) -> Result<String, String> {
    let path = checked_action_path(root, relative)?;

    let revision = identity(&path)?;

    if expected.is_some_and(|expected| revision != expected) {
        return Err("File changed; rescan its source".into());
    }

    Ok(revision)
}

fn identity(path: &Path) -> Result<String, String> {
    let metadata = path.metadata().map_err(|_| "Cannot inspect file")?;

    Ok(format!(
        "{}:{:?}",
        metadata.len(),
        metadata.modified().map_err(|_| "Cannot inspect revision")?
    ))
}

/// Low-frequency DCT hash. Flat/near-flat pictures lack enough signal for a useful match.
pub(super) fn perceptual(image: &RgbImage) -> Option<u64> {
    let small = image::imageops::resize(image, 32, 32, FilterType::Triangle);

    let pixels: Vec<f64> = small
        .pixels()
        .map(|p| 0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2]))
        .collect();

    let mean = pixels.iter().sum::<f64>() / 1024.0;

    let variance = pixels
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / 1024.0;

    if variance < 16.0 {
        return None;
    }

    let cosines: Vec<Vec<f64>> = (0..8)
        .map(|frequency| {
            (0..32)
                .map(|position| {
                    (std::f64::consts::PI * f64::from(2 * position + 1) * f64::from(frequency)
                        / 64.0)
                        .cos()
                })
                .collect()
        })
        .collect();

    let mut coefficients = Vec::with_capacity(63);

    for u in 0..8 {
        for v in 0..8 {
            if u == 0 && v == 0 {
                continue;
            }

            let mut sum = 0.0;

            for y in 0..32 {
                for x in 0..32 {
                    sum += pixels[y * 32 + x] * cosines[u][x] * cosines[v][y];
                }
            }

            coefficients.push(sum);
        }
    }

    let mut sorted = coefficients.clone();

    sorted.sort_by(f64::total_cmp);

    let median = sorted[31];

    Some(
        coefficients
            .iter()
            .enumerate()
            .fold(0, |hash, (index, value)| {
                hash | (u64::from(*value > median) << index)
            }),
    )
}

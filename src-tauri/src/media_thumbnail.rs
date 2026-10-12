//! Standalone Shell thumbnails. Call only on a blocking STA-compatible worker.
//! The result is opaque RGB, not source-image transparency or encoded cache data.
//! Shell may consult/populate its own system cache; Nexa writes no cache here.

use std::path::Path;

use image::RgbImage;

/// Bounds requested dimensions and conversion allocations (at most 16 MiB BGRA).
pub const MAX_THUMBNAIL_EDGE: u32 = 2048;

#[derive(Debug, thiserror::Error)]
pub enum ThumbnailError {
    #[error("Thumbnails are only supported on Windows")]
    UnsupportedPlatform,
    #[error("Thumbnail size must be between 1 and {MAX_THUMBNAIL_EDGE}")]
    InvalidSize,
    #[error("Media path contains a NUL character")]
    InvalidPath,
    #[error("Could not resolve or inspect the media file")]
    File(#[source] std::io::Error),
    #[error("Thumbnail requires a regular file")]
    NotAFile,
    #[error("Provider returned invalid or oversized bitmap dimensions")]
    InvalidBitmap,
    #[error("Could not allocate thumbnail pixels")]
    Allocation,
    #[error("GDI operation {operation} failed")]
    Gdi { operation: &'static str },
    #[error("GDI returned {copied} of {expected} bitmap rows")]
    IncompleteCopy { copied: i32, expected: u32 },
    #[cfg(windows)]
    #[error("Windows thumbnail operation {operation} failed ({hresult:#x})")]
    Windows {
        operation: &'static str,
        hresult: i32,
    },
}

impl ThumbnailError {
    fn category(&self) -> &'static str {
        match self {
            Self::UnsupportedPlatform => "unsupported_platform",
            Self::InvalidSize => "invalid_size",
            Self::InvalidPath => "invalid_path",
            Self::File(_) => "file_access",
            Self::NotAFile => "not_a_file",
            Self::InvalidBitmap => "invalid_bitmap",
            Self::Allocation => "allocation",
            Self::Gdi { .. } => "gdi",
            Self::IncompleteCopy { .. } => "incomplete_copy",
            #[cfg(windows)]
            Self::Windows { .. } => "windows",
        }
    }
}

/// Requests an aspect-preserving thumbnail inside `size` x `size`, without upscaling
/// or an icon fallback. Actual dimensions may be smaller. Does not save the result.
pub fn get_thumbnail(path: &Path, size: u32) -> Result<RgbImage, ThumbnailError> {
    let _span = tracing::debug_span!("get_thumbnail", requested_edge = size).entered();

    tracing::debug!("Thumbnail extraction started");

    #[cfg(windows)]
    let result = if (1..=MAX_THUMBNAIL_EDGE).contains(&size) {
        windows_provider::extract(path, size)
    } else {
        Err(ThumbnailError::InvalidSize)
    };

    #[cfg(not(windows))]
    let result = {
        let _ = path;
        Err::<RgbImage, _>(ThumbnailError::UnsupportedPlatform)
    };

    match &result {
        Ok(image) => tracing::debug!(
            width = image.width(),
            height = image.height(),
            "Thumbnail extraction completed"
        ),
        #[cfg(windows)]
        Err(ThumbnailError::Windows { operation, hresult }) => {
            tracing::warn!(operation, hresult, "Windows thumbnail extraction failed")
        }
        Err(error) => tracing::warn!(category = error.category(), "Thumbnail extraction failed"),
    }
    result
}

#[cfg(windows)]
mod windows_provider {
    use super::*;
    use crate::windows_com::Apartment;
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};
    use windows::{
        Win32::{
            Foundation::SIZE,
            Graphics::Gdi::{
                BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS,
                DeleteDC, DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC, HGDIOBJ,
            },
            UI::Shell::{
                IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY,
            },
        },
        core::HSTRING,
    };

    struct OwnedBitmap(HBITMAP);

    impl Drop for OwnedBitmap {
        fn drop(&mut self) {
            // SAFETY: uniquely owned bitmap, never selected into a DC by this module.
            if !unsafe { DeleteObject(HGDIOBJ::from(self.0)) }.as_bool() {
                tracing::warn!("Thumbnail bitmap cleanup failed");
            }
        }
    }

    struct MemoryDc(HDC);

    impl Drop for MemoryDc {
        fn drop(&mut self) {
            // SAFETY: this DC was created by CreateCompatibleDC on this thread.
            if !unsafe { DeleteDC(self.0) }.as_bool() {
                tracing::warn!("Thumbnail device context cleanup failed");
            }
        }
    }

    fn native_error(operation: &'static str, error: windows::core::Error) -> ThumbnailError {
        ThumbnailError::Windows {
            operation,
            hresult: error.code().0,
        }
    }

    pub(super) fn extract(path: &Path, size: u32) -> Result<RgbImage, ThumbnailError> {
        if path.as_os_str().encode_wide().any(|unit| unit == 0) {
            return Err(ThumbnailError::InvalidPath);
        }
        // Shell accepts normal absolute DOS/UNC syntax; canonicalize can add \\?\.
        let path = std::path::absolute(path).map_err(ThumbnailError::File)?;

        if !std::fs::metadata(&path)
            .map_err(ThumbnailError::File)?
            .is_file()
        {
            return Err(ThumbnailError::NotAFile);
        }

        let _apartment =
            Apartment::initialize().map_err(|error| native_error("initialize_com", error))?;
        // SAFETY: COM initialized, native absolute Unicode path has no embedded NUL.
        let factory: IShellItemImageFactory =
            unsafe { SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None) }
                .map_err(|error| native_error("create_image_factory", error))?;
        // SAFETY: bounded dimensions fit i32; thumbnail-only deliberately forbids icons.
        let bitmap = OwnedBitmap(
            unsafe {
                factory.GetImage(
                    SIZE {
                        cx: size as i32,
                        cy: size as i32,
                    },
                    SIIGBF_THUMBNAILONLY,
                )
            }
            .map_err(|error| native_error("get_thumbnail", error))?,
        );
        bitmap_to_rgb(&bitmap)
        // Bitmap and factory drop before the COM guard, including on errors.
    }

    fn dimensions(width: i32, height: i32) -> Result<(u32, u32, usize), ThumbnailError> {
        let width = u32::try_from(width).map_err(|_| ThumbnailError::InvalidBitmap)?;

        let height = height.unsigned_abs();

        if width == 0 || height == 0 || width > MAX_THUMBNAIL_EDGE || height > MAX_THUMBNAIL_EDGE {
            return Err(ThumbnailError::InvalidBitmap);
        }

        let length = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(ThumbnailError::InvalidBitmap)?;
        Ok((width, height, length))
    }

    fn bitmap_to_rgb(bitmap: &OwnedBitmap) -> Result<RgbImage, ThumbnailError> {
        let mut descriptor = BITMAP::default();
        // SAFETY: valid owned handle and a writable buffer exactly the size of BITMAP.
        let copied = unsafe {
            GetObjectW(
                HGDIOBJ::from(bitmap.0),
                size_of::<BITMAP>() as i32,
                Some((&mut descriptor as *mut BITMAP).cast::<c_void>()),
            )
        };

        if copied != size_of::<BITMAP>() as i32 {
            return Err(ThumbnailError::Gdi {
                operation: "get_bitmap_descriptor",
            });
        }

        let (width, height, length) = dimensions(descriptor.bmWidth, descriptor.bmHeight)?;

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // Request top-down rows.
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(length)
            .map_err(|_| ThumbnailError::Allocation)?;
        pixels.resize(length, 0);
        // SAFETY: null source creates a memory DC owned by this thread.
        let dc = unsafe { CreateCompatibleDC(None) };

        if dc.0.is_null() {
            return Err(ThumbnailError::Gdi {
                operation: "create_memory_dc",
            });
        }

        let dc = MemoryDc(dc);
        // SAFETY: bitmap is not selected into a DC; buffer fits all requested 32-bit
        // rows. At 32 bpp each row is already aligned to the required DWORD boundary.
        let copied = unsafe {
            GetDIBits(
                dc.0,
                bitmap.0,
                0,
                height,
                Some(pixels.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        };

        if copied != height as i32 {
            return Err(ThumbnailError::IncompleteCopy {
                copied,
                expected: height,
            });
        }
        bgrx_to_rgb(width, height, pixels)
    }

    fn bgrx_to_rgb(
        width: u32,
        height: u32,
        mut pixels: Vec<u8>,
    ) -> Result<RgbImage, ThumbnailError> {
        // Compact in place: never treat BI_RGB's reserved byte as valid alpha.
        let count = pixels.len() / 4;

        for index in 0..count {
            let source = index * 4;

            let [b, g, r] = [pixels[source], pixels[source + 1], pixels[source + 2]];
            pixels[index * 3..index * 3 + 3].copy_from_slice(&[r, g, b]);
        }
        pixels.truncate(count * 3);
        RgbImage::from_raw(width, height, pixels).ok_or(ThumbnailError::InvalidBitmap)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn rejects_invalid_dimensions_before_allocating() {
            for (width, height) in [
                (0, 1),
                (-1, 1),
                (1, 0),
                (1, i32::MIN),
                (i32::MAX, 1),
                (1, 2049),
            ] {
                assert!(matches!(
                    dimensions(width, height),
                    Err(ThumbnailError::InvalidBitmap)
                ));
            }
            assert_eq!(dimensions(2, -3).unwrap(), (2, 3, 24));
        }

        #[test]
        fn converts_channels_without_using_reserved_alpha() {
            let image = bgrx_to_rgb(2, 1, vec![3, 2, 1, 0, 6, 5, 4, 127]).unwrap();
            assert_eq!(image.as_raw(), &[1, 2, 3, 4, 5, 6]);
        }

        #[test]
        fn validates_size_and_file_input() {
            let directory = tempfile::tempdir().unwrap();

            for size in [0, MAX_THUMBNAIL_EDGE + 1, u32::MAX] {
                assert!(matches!(
                    get_thumbnail(directory.path(), size),
                    Err(ThumbnailError::InvalidSize)
                ));
            }
            assert!(matches!(
                get_thumbnail(directory.path(), 64),
                Err(ThumbnailError::NotAFile)
            ));
            assert!(matches!(
                get_thumbnail(&directory.path().join("missing.bmp"), 64),
                Err(ThumbnailError::File(_))
            ));
            assert!(matches!(
                get_thumbnail(Path::new("invalid\0.bmp"), 64),
                Err(ThumbnailError::InvalidPath)
            ));
        }

        #[test]
        fn copies_top_down_bitmap_and_releases_gdi_handle() {
            use windows::Win32::Graphics::Gdi::CreateDIBSection;

            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: 1,
                    biHeight: -2,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits = std::ptr::null_mut();

            let bitmap = OwnedBitmap(
                unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }
                    .unwrap(),
            );
            // SAFETY: DIB owns exactly eight writable bytes, no GDI drawing is pending.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    [0_u8, 0, 255, 0, 255, 0, 0, 0].as_ptr(),
                    bits.cast(),
                    8,
                );
            }

            let image = bitmap_to_rgb(&bitmap).unwrap();
            assert_eq!(image.dimensions(), (1, 2));
            assert_eq!(image.as_raw(), &[255, 0, 0, 0, 0, 255]);

            let handle = bitmap.0;
            drop(bitmap);

            let mut descriptor = BITMAP::default();
            assert_eq!(
                unsafe {
                    GetObjectW(
                        HGDIOBJ::from(handle),
                        size_of::<BITMAP>() as i32,
                        Some((&mut descriptor as *mut BITMAP).cast()),
                    )
                },
                0
            );
        }

        #[test]
        fn extracts_unicode_thumbnail_and_preserves_source_bytes() {
            let directory = tempfile::tempdir().unwrap();

            let path = directory.path().join("превью 日本.bmp");
            // Uncompressed 2x1 24-bit BMP: red followed by blue.
            let mut bytes = vec![0_u8; 62];
            bytes[0..2].copy_from_slice(b"BM");

            for (offset, value) in [(2, 62_u32), (10, 54), (14, 40), (18, 2), (22, 1), (34, 8)] {
                bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
            bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
            bytes[28..30].copy_from_slice(&24_u16.to_le_bytes());
            bytes[54..60].copy_from_slice(&[0, 0, 255, 255, 0, 0]);
            std::fs::write(&path, &bytes).unwrap();

            for _ in 0..3 {
                let image = get_thumbnail(&path, 64).unwrap();
                assert!(image.width() > 0 && image.width() <= 64);
                assert!(image.height() > 0 && image.height() <= 64);
            }
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }

        #[test]
        fn does_not_return_icon_for_file_without_thumbnail() {
            let directory = tempfile::tempdir().unwrap();

            let path = directory.path().join("plain.txt");
            std::fs::write(&path, b"not an image").unwrap();
            assert!(matches!(
                get_thumbnail(&path, 64),
                Err(ThumbnailError::Windows {
                    operation: "get_thumbnail",
                    ..
                })
            ));
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn reports_unsupported_platform() {
        assert!(matches!(
            get_thumbnail(Path::new("unused"), 64),
            Err(ThumbnailError::UnsupportedPlatform)
        ));
    }
}

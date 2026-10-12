//! Read-only technical metadata, independent of scanning, persistence and IPC.
//!
//! This synchronous API can invoke slow installed Shell handlers. Call it on a
//! blocking worker with an STA-compatible COM apartment, never the UI/async thread.
//! Windows handler coverage varies; success does not imply every field is known.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
}

/// Rate reported by the provider, not an inferred original stream time base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MediaMetadata {
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Windows 100 ns ticks truncated to whole milliseconds. Zero is valid.
    pub duration_ms: Option<u64>,
    pub frame_rate: Option<FrameRate>,
    pub codec: Option<String>,
    pub bitrate_bps: Option<u32>,
    /// Missing properties are normal; failed or malformed properties appear here.
    pub issues: Vec<PropertyIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataProperty {
    Width,
    Height,
    Duration,
    FrameRate,
    Codec,
    Bitrate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyIssue {
    ReadFailed {
        property: MetadataProperty,
        hresult: i32,
    },
    UnexpectedType {
        property: MetadataProperty,
        variant_type: u16,
    },
    InvalidZero {
        property: MetadataProperty,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("Media metadata is only supported on Windows")]
    UnsupportedPlatform,
    #[error("Could not resolve or inspect the media file")]
    File(#[source] std::io::Error),
    #[error("Metadata requires a regular file")]
    NotAFile,
    #[error("Media path contains a NUL character")]
    InvalidPath,
    #[cfg(windows)]
    #[error("Windows metadata operation {operation} failed ({hresult:#x})")]
    Windows {
        operation: &'static str,
        hresult: i32,
    },
}

/// Kind is supplied by the future scanner rather than guessed from an extension.
/// Returns partial metadata when individual properties fail; opening failures are errors.
pub fn query_media_metadata(path: &Path, kind: MediaKind) -> Result<MediaMetadata, MetadataError> {
    let _span = tracing::debug_span!("query_media_metadata", ?kind).entered();

    tracing::debug!("Metadata query started");

    #[cfg(windows)]
    let result = windows_provider::query(path, kind);

    #[cfg(not(windows))]
    let result = {
        let _ = path;
        Err::<MediaMetadata, _>(MetadataError::UnsupportedPlatform)
    };

    match &result {
        Ok(metadata) => tracing::debug!(
            known_fields = usize::from(metadata.width.is_some())
                + usize::from(metadata.height.is_some())
                + usize::from(metadata.duration_ms.is_some())
                + usize::from(metadata.frame_rate.is_some())
                + usize::from(metadata.codec.is_some())
                + usize::from(metadata.bitrate_bps.is_some()),
            issues = metadata.issues.len(),
            "Metadata query completed"
        ),
        #[cfg(windows)]
        Err(MetadataError::Windows { operation, hresult }) => {
            tracing::warn!(operation, hresult, "Windows metadata query failed");
        }
        Err(error) => {
            let category = match error {
                MetadataError::UnsupportedPlatform => "unsupported_platform",
                MetadataError::File(_) => "file_access",
                MetadataError::NotAFile => "not_a_file",
                MetadataError::InvalidPath => "invalid_path",
                #[cfg(windows)]
                MetadataError::Windows { .. } => "windows",
            };

            tracing::warn!(
                category,
                "Metadata query failed; caller may retry or mark extraction failed"
            );
        }
    }
    result
}

#[cfg(windows)]
fn codec_label(value: &str) -> String {
    let normalized = value.trim_matches(['{', '}']).to_ascii_lowercase();

    if let Some(prefix) = normalized.strip_suffix("-0000-0010-8000-00aa00389b71") {
        let known = match prefix {
            "34363248" | "31435641" => Some("H.264"),
            "43564548" | "35363248" | "31637668" | "31766568" => Some("HEVC"),
            "30395056" => Some("VP9"),
            "30385056" => Some("VP8"),
            "31305641" => Some("AV1"),
            "47504a4d" => Some("Motion JPEG"),
            _ => None,
        };

        if let Some(known) = known {
            return known.into();
        }
    }
    value.to_owned()
}

#[cfg(windows)]
mod windows_provider {
    use super::*;
    use crate::windows_com::Apartment;
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::{
            Foundation::PROPERTYKEY,
            Storage::EnhancedStorage::{
                PKEY_Image_HorizontalSize, PKEY_Image_VerticalSize, PKEY_Media_Duration,
                PKEY_Video_Compression, PKEY_Video_EncodingBitrate, PKEY_Video_FrameHeight,
                PKEY_Video_FrameRate, PKEY_Video_FrameWidth,
            },
            System::{
                Com::StructuredStorage::{PROPVARIANT, PropVariantClear},
                Variant::{VT_CLSID, VT_EMPTY, VT_LPWSTR, VT_NULL, VT_UI4, VT_UI8},
            },
            UI::Shell::{
                IShellItem2,
                PropertiesSystem::{GPS_BESTEFFORT, GPS_OPENSLOWITEM, IPropertyStore},
                SHCreateItemFromParsingName,
            },
        },
        core::HSTRING,
    };

    struct PropertyValue(PROPVARIANT);

    impl Drop for PropertyValue {
        fn drop(&mut self) {
            // SAFETY: GetValue returned an owned initialized PROPVARIANT.
            let result = unsafe { PropVariantClear(&mut self.0) };

            if let Err(error) = result {
                tracing::warn!(hresult = error.code().0, "Property value cleanup failed");
            }
        }
    }

    fn native_error(operation: &'static str, error: windows::core::Error) -> MetadataError {
        MetadataError::Windows {
            operation,
            hresult: error.code().0,
        }
    }

    pub(super) fn query(path: &Path, kind: MediaKind) -> Result<MediaMetadata, MetadataError> {
        if path.as_os_str().encode_wide().any(|unit| unit == 0) {
            return Err(MetadataError::InvalidPath);
        }
        // Keep regular DOS/UNC syntax: canonicalize adds a verbatim \\?\ prefix
        // which Shell parsing handlers can reject even for otherwise valid files.
        let path = std::path::absolute(path).map_err(MetadataError::File)?;

        if !std::fs::metadata(&path)
            .map_err(MetadataError::File)?
            .is_file()
        {
            return Err(MetadataError::NotAFile);
        }

        let _apartment =
            Apartment::initialize().map_err(|error| native_error("initialize_com", error))?;
        // SAFETY: COM initialized; native Unicode absolute path has no embedded NUL.
        let item: IShellItem2 =
            unsafe { SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None) }
                .map_err(|error| native_error("create_shell_item", error))?;
        // SAFETY: valid Shell item; read-only flags permit slow installed handlers.
        let store: IPropertyStore =
            unsafe { item.GetPropertyStore(GPS_BESTEFFORT | GPS_OPENSLOWITEM) }
                .map_err(|error| native_error("open_property_store", error))?;

        let mut metadata = MediaMetadata::default();

        let (width, height) = match kind {
            MediaKind::Image => (PKEY_Image_HorizontalSize, PKEY_Image_VerticalSize),
            MediaKind::Video => (PKEY_Video_FrameWidth, PKEY_Video_FrameHeight),
        };
        metadata.width = positive(
            read(
                &store,
                &width,
                MetadataProperty::Width,
                false,
                &mut metadata.issues,
            ),
            MetadataProperty::Width,
            &mut metadata.issues,
        );
        metadata.height = positive(
            read(
                &store,
                &height,
                MetadataProperty::Height,
                false,
                &mut metadata.issues,
            ),
            MetadataProperty::Height,
            &mut metadata.issues,
        );

        if kind == MediaKind::Video {
            metadata.bitrate_bps = positive(
                read(
                    &store,
                    &PKEY_Video_EncodingBitrate,
                    MetadataProperty::Bitrate,
                    false,
                    &mut metadata.issues,
                ),
                MetadataProperty::Bitrate,
                &mut metadata.issues,
            );
            metadata.codec = read_codec(&store, &mut metadata.issues);
            metadata.duration_ms = read(
                &store,
                &PKEY_Media_Duration,
                MetadataProperty::Duration,
                true,
                &mut metadata.issues,
            )
            .map(|ticks| ticks / 10_000);
            metadata.frame_rate = positive(
                read(
                    &store,
                    &PKEY_Video_FrameRate,
                    MetadataProperty::FrameRate,
                    false,
                    &mut metadata.issues,
                ),
                MetadataProperty::FrameRate,
                &mut metadata.issues,
            )
            .map(frame_rate);
        }

        if !metadata.issues.is_empty() {
            tracing::warn!(
                issue_count = metadata.issues.len(),
                "Returning partial metadata after property failures"
            );
        }
        // store/item drop before the apartment, including on all error paths.
        Ok(metadata)
    }

    fn read(
        store: &IPropertyStore,
        key: &PROPERTYKEY,
        property: MetadataProperty,
        wide: bool,
        issues: &mut Vec<PropertyIssue>,
    ) -> Option<u64> {
        // SAFETY: valid store and documented property key; returned value is owned.
        let value = match unsafe { store.GetValue(key) } {
            Ok(value) => PropertyValue(value),
            Err(error) => {
                issues.push(PropertyIssue::ReadFailed {
                    property,
                    hresult: error.code().0,
                });

                return None;
            }
        };
        decode(&value, property, wide, issues)
    }

    fn read_codec(store: &IPropertyStore, issues: &mut Vec<PropertyIssue>) -> Option<String> {
        // SAFETY: valid store; the wrapper owns and clears the returned variant.
        let value = match unsafe { store.GetValue(&PKEY_Video_Compression) } {
            Ok(value) => PropertyValue(value),
            Err(error) => {
                issues.push(PropertyIssue::ReadFailed {
                    property: MetadataProperty::Codec,
                    hresult: error.code().0,
                });

                return None;
            }
        };
        // SAFETY: discriminate before accessing pointers owned by the property value.
        unsafe {
            let inner = &value.0.Anonymous.Anonymous;

            match inner.vt {
                VT_EMPTY | VT_NULL => None,
                VT_LPWSTR if !inner.Anonymous.pwszVal.is_null() => {
                    let text = inner.Anonymous.pwszVal.to_string().ok()?;

                    let text = text.trim();
                    (!text.is_empty()).then(|| codec_label(text))
                }
                VT_CLSID if !inner.Anonymous.puuid.is_null() => {
                    Some(format!("{:?}", *inner.Anonymous.puuid))
                }
                vt => {
                    issues.push(PropertyIssue::UnexpectedType {
                        property: MetadataProperty::Codec,
                        variant_type: vt.0,
                    });
                    None
                }
            }
        }
    }

    fn decode(
        value: &PropertyValue,
        property: MetadataProperty,
        wide: bool,
        issues: &mut Vec<PropertyIssue>,
    ) -> Option<u64> {
        // SAFETY: vt is the discriminant; union members are read only after matching it.
        unsafe {
            let inner = &value.0.Anonymous.Anonymous;

            match inner.vt {
                VT_EMPTY | VT_NULL => None,
                VT_UI8 if wide => Some(inner.Anonymous.uhVal),
                VT_UI4 if !wide => Some(u64::from(inner.Anonymous.ulVal)),
                variant_type => {
                    issues.push(PropertyIssue::UnexpectedType {
                        property,
                        variant_type: variant_type.0,
                    });
                    None
                }
            }
        }
    }

    fn positive(
        value: Option<u64>,
        property: MetadataProperty,
        issues: &mut Vec<PropertyIssue>,
    ) -> Option<u32> {
        match value {
            Some(0) => {
                issues.push(PropertyIssue::InvalidZero { property });
                None
            }
            // Values originate exclusively from VT_UI4 above.
            value => value.map(|value| value as u32),
        }
    }

    fn frame_rate(raw: u32) -> FrameRate {
        let (mut a, mut b) = (raw, 1000);

        while b != 0 {
            (a, b) = (b, a % b);
        }
        FrameRate {
            numerator: raw / a,
            denominator: 1000 / a,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize};

        #[test]
        fn preserves_provider_rate_without_guessing_time_base() {
            assert_eq!(
                frame_rate(29970),
                FrameRate {
                    numerator: 2997,
                    denominator: 100
                }
            );
            assert_eq!(
                frame_rate(24000),
                FrameRate {
                    numerator: 24,
                    denominator: 1
                }
            );
        }

        #[test]
        fn rejects_zero_dimensions_and_rates() {
            let mut issues = Vec::new();
            assert_eq!(
                positive(Some(0), MetadataProperty::Width, &mut issues),
                None
            );
            assert_eq!(positive(None, MetadataProperty::Height, &mut issues), None);
            assert_eq!(issues.len(), 1);
        }

        #[test]
        fn distinguishes_missing_zero_duration_and_wrong_types() {
            let mut value = PropertyValue(PROPVARIANT::default());

            let mut issues = Vec::new();
            assert_eq!(
                decode(&value, MetadataProperty::Duration, true, &mut issues),
                None
            );
            // SAFETY: initializing the matching numeric member of our owned variant.
            unsafe {
                (*value.0.Anonymous.Anonymous).vt = VT_UI8;
                (*value.0.Anonymous.Anonymous).Anonymous.uhVal = 0;
            }
            assert_eq!(
                decode(&value, MetadataProperty::Duration, true, &mut issues),
                Some(0)
            );
            unsafe {
                (*value.0.Anonymous.Anonymous).Anonymous.uhVal = u64::MAX;
            }
            assert_eq!(
                decode(&value, MetadataProperty::Duration, true, &mut issues)
                    .map(|ticks| ticks / 10_000),
                Some(u64::MAX / 10_000)
            );
            assert_eq!(
                decode(&value, MetadataProperty::Width, false, &mut issues),
                None
            );
            assert_eq!(
                issues,
                vec![PropertyIssue::UnexpectedType {
                    property: MetadataProperty::Width,
                    variant_type: VT_UI8.0
                }]
            );
        }

        #[test]
        fn extracts_unicode_image_dimensions_without_modifying_file() {
            let directory = tempfile::tempdir().unwrap();

            let path = directory.path().join("зображення 日本.bmp");
            // Uncompressed 2x1 24-bit BMP with one padded pixel row.
            let mut bytes = vec![0_u8; 62];
            bytes[0..2].copy_from_slice(b"BM");
            bytes[2..6].copy_from_slice(&62_u32.to_le_bytes());
            bytes[10..14].copy_from_slice(&54_u32.to_le_bytes());
            bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
            bytes[18..22].copy_from_slice(&2_i32.to_le_bytes());
            bytes[22..26].copy_from_slice(&1_i32.to_le_bytes());
            bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
            bytes[28..30].copy_from_slice(&24_u16.to_le_bytes());
            bytes[34..38].copy_from_slice(&8_u32.to_le_bytes());
            std::fs::write(&path, &bytes).unwrap();

            for _ in 0..2 {
                let metadata = query_media_metadata(&path, MediaKind::Image).unwrap();
                assert_eq!((metadata.width, metadata.height), (Some(2), Some(1)));
                assert_eq!((metadata.duration_ms, metadata.frame_rate), (None, None));
                assert!(metadata.issues.is_empty(), "{:?}", metadata.issues);
            }
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }

        #[test]
        fn nested_sta_initialization_remains_balanced() {
            std::thread::spawn(|| {
                let outer = Apartment::initialize().unwrap();

                let inner = Apartment::initialize().unwrap();
                drop(inner);
                // The outer initialization must still keep this thread in STA.
                use windows::Win32::System::Com::COINIT_MULTITHREADED;
                assert!(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err());
                drop(outer);
                unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
                    .ok()
                    .unwrap();
                unsafe { CoUninitialize() };
            })
            .join()
            .unwrap();
        }

        #[test]
        fn rejects_missing_files_directories_and_nul_paths() {
            let directory = tempfile::tempdir().unwrap();
            assert!(matches!(
                query(&directory.path().join("missing.mp4"), MediaKind::Video),
                Err(MetadataError::File(_))
            ));
            assert!(matches!(
                query(directory.path(), MediaKind::Image),
                Err(MetadataError::NotAFile)
            ));
            assert!(matches!(
                query(Path::new("invalid\0.mp4"), MediaKind::Video),
                Err(MetadataError::InvalidPath)
            ));
        }

        #[test]
        fn incompatible_apartment_is_not_uninitialized() {
            use windows::Win32::System::Com::COINIT_MULTITHREADED;
            std::thread::spawn(|| {
                unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
                    .ok()
                    .unwrap();
                assert!(Apartment::initialize().is_err());
                // A second STA attempt must still fail: our failed guard did not tear down MTA.
                assert!(Apartment::initialize().is_err());
                unsafe { CoUninitialize() };
            })
            .join()
            .unwrap();
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn reports_unsupported_platform() {
        assert!(matches!(
            query_media_metadata(Path::new("unused"), MediaKind::Video),
            Err(MetadataError::UnsupportedPlatform)
        ));
    }
}

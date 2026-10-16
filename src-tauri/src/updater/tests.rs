use std::{ffi::OsString, fs, path::Path};

use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use velopack::VelopackAsset;

use super::{launch, source};

fn asset(bytes: &[u8]) -> VelopackAsset {
    VelopackAsset {
        PackageId: "Nexa".into(),
        Version: "2.2.1".into(),
        Type: "Full".into(),
        FileName: "Nexa-2.2.1-full.nupkg".into(),
        SHA256: Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        Size: bytes.len() as u64,
        ..Default::default()
    }
}

#[test]
fn rejects_unsigned_feed() {
    assert!(source::verify_feed("{\"Assets\":[]}", "invalid").is_err());
}

#[test]
fn updater_waits_for_current_process_and_keeps_logs_outside_replaceable_files() {
    let arguments = launch::arguments(
        Path::new("install/packages/Nexa-full.nupkg"),
        Path::new("install"),
        Path::new("install/packages"),
        Path::new("NexaNew/logs/velopack-update.log"),
        42,
    );

    let expected: Vec<OsString> = [
        "apply",
        "--silent",
        "--norestart",
        "--package",
        "install/packages/Nexa-full.nupkg",
        "--waitPid",
        "42",
        "--root",
        "install",
        "--packageDir",
        "install/packages",
        "--log",
        "NexaNew/logs/velopack-update.log",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();

    assert_eq!(arguments, expected);
    assert!(arguments.iter().any(|argument| argument == "--norestart"));
}

#[test]
fn verifies_signed_release_and_rejects_tampered_feed() {
    // Base64 preserves the exact signed bytes across Git newline conversion.
    let json = String::from_utf8(
        STANDARD
            .decode(include_str!("fixtures/feed.b64").trim())
            .unwrap(),
    )
    .unwrap();

    let signature = include_str!("fixtures/feed.sig");

    let feed = source::verify_feed(&json, signature).unwrap();

    assert_eq!(feed.Assets.len(), 1);
    assert_eq!(feed.Assets[0].PackageId, "Nexa");

    let tampered = json.replace("2.2.1", "9.9.9");

    assert_ne!(json, tampered);
    assert!(source::verify_feed(&tampered, signature).is_err());
}

#[test]
fn rejects_unsafe_or_unverifiable_package_entries() {
    let valid = asset(b"package");

    assert!(source::validate_asset(&valid).is_ok());

    for name in [
        "../Nexa-full.nupkg",
        "C:\\Nexa-full.nupkg",
        "Nexa-delta.nupkg",
    ] {
        let mut invalid = valid.clone();

        invalid.FileName = name.into();
        assert!(source::validate_asset(&invalid).is_err());
    }

    let mut invalid = valid.clone();

    invalid.SHA256.clear();
    assert!(source::validate_asset(&invalid).is_err());

    invalid = valid.clone();
    invalid.PackageId = "Other".into();
    assert!(source::validate_asset(&invalid).is_err());

    invalid = valid;
    invalid.Version = "../../other".into();
    assert!(source::validate_asset(&invalid).is_err());
}

#[test]
fn verifies_cached_bytes_and_rejects_same_size_tampering() {
    let directory = tempfile::tempdir().unwrap();

    let path = directory.path().join("package.nupkg");

    let release = asset(b"original");

    fs::write(&path, b"original").unwrap();
    assert!(source::verify_package(&path, &release).is_ok());

    fs::write(&path, b"modified").unwrap();
    assert!(source::verify_package(&path, &release).is_err());

    fs::write(&path, b"short").unwrap();
    assert!(source::verify_package(&path, &release).is_err());
}

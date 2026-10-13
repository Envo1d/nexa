use std::{fs::File, io::Read, path::Path, sync::mpsc::Sender};

use base64::{Engine, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};
use velopack::{
    Error, HttpOptions, VelopackAsset, VelopackAssetFeed,
    bundle::Manifest,
    download::{download_url_as_string, download_url_to_file},
    locator::{LocationContext, auto_locate_app_manifest},
    sources::UpdateSource,
};

const FEED_ROOT: &str = "https://github.com/Envo1d/nexa/releases/latest/download";

const RELEASE_ROOT: &str = "https://github.com/Envo1d/nexa/releases/download";

const PUBLIC_KEY: &str = include_str!("../../updater.pub");

pub(super) struct SignedSource;

fn invalid(reason: impl ToString) -> Error {
    Error::InvalidPackage(reason.to_string())
}

fn decode_text(encoded: &str) -> Result<String, Error> {
    String::from_utf8(STANDARD.decode(encoded.trim()).map_err(invalid)?).map_err(invalid)
}

pub(super) fn verify_feed(json: &str, signature: &str) -> Result<VelopackAssetFeed, Error> {
    let public_key = PublicKey::decode(&decode_text(PUBLIC_KEY)?).map_err(invalid)?;

    let signature = Signature::decode(&decode_text(signature)?).map_err(invalid)?;

    public_key
        .verify(json.as_bytes(), &signature, true)
        .map_err(invalid)?;

    let feed: VelopackAssetFeed = serde_json::from_str(json)?;

    for asset in &feed.Assets {
        validate_asset(asset)?;
    }

    Ok(feed)
}

pub(super) fn validate_asset(asset: &VelopackAsset) -> Result<(), Error> {
    let safe_name = !asset.FileName.is_empty()
        && asset
            .FileName
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        && asset.FileName.ends_with("-full.nupkg");

    let safe_version = !asset.Version.is_empty()
        && asset
            .Version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.');

    let valid_hash =
        asset.SHA256.len() == 64 && asset.SHA256.bytes().all(|byte| byte.is_ascii_hexdigit());

    if asset.PackageId != "Nexa"
        || asset.Type != "Full"
        || !safe_name
        || !safe_version
        || !valid_hash
        || asset.Size == 0
    {
        return Err(invalid("Invalid Nexa full-package feed entry"));
    }

    Ok(())
}

impl UpdateSource for SignedSource {
    fn get_release_feed(
        &self,
        channel: &str,
        _app: &Manifest,
        _staged_user_id: &str,
    ) -> Result<VelopackAssetFeed, Error> {
        if channel != "win-x64" {
            return Err(invalid("Unsupported update channel"));
        }

        let options = HttpOptions {
            TimeoutMilliseconds: 30_000,
            ..Default::default()
        };

        let url = format!("{FEED_ROOT}/releases.{channel}.json");

        let json = download_url_as_string(&url, Some(&options))?;

        let signature = download_url_as_string(&format!("{url}.sig"), Some(&options))?;

        verify_feed(&json, &signature)
    }

    fn download_release_entry(
        &self,
        asset: &VelopackAsset,
        local_file: &Path,
        progress_sender: Option<Sender<i16>>,
    ) -> Result<(), Error> {
        validate_asset(asset)?;

        let options = HttpOptions {
            TimeoutMilliseconds: 600_000,
            ..Default::default()
        };

        // Freeze the release URL even if a newer latest release is published.
        let url = format!("{RELEASE_ROOT}/v{}/{}", asset.Version, asset.FileName);

        download_url_to_file(&url, local_file, Some(&options), |percent| {
            if let Some(sender) = &progress_sender {
                let _ = sender.send(percent);
            }
        })
    }
}

pub(super) fn verify_cached_package(asset: &VelopackAsset) -> Result<(), Error> {
    validate_asset(asset)?;

    let locator = auto_locate_app_manifest(LocationContext::FromCurrentExe)?;

    let path = locator.get_packages_dir().join(&asset.FileName);

    let result = verify_package(&path, asset);

    if matches!(&result, Err(Error::InvalidPackage(_))) {
        // Only remove the confined cache entry selected from the signed feed.
        // A subsequent explicit retry can download a clean copy.
        match std::fs::remove_file(&path) {
            Ok(()) => tracing::warn!(
                "Corrupt cached update removed; a fresh download is available on retry"
            ),
            Err(error) => tracing::warn!(%error, "Could not remove corrupt cached update"),
        }
    }

    result
}

pub(super) fn verify_package(path: &Path, asset: &VelopackAsset) -> Result<(), Error> {
    let mut file = File::open(path)?;

    if file.metadata()?.len() != asset.Size {
        return Err(invalid("Cached package size does not match signed feed"));
    }

    let mut hash = Sha256::new();

    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let count = file.read(&mut buffer)?;

        if count == 0 {
            break;
        }

        hash.update(&buffer[..count]);
    }

    let actual = format!("{:x}", hash.finalize());

    if !actual.eq_ignore_ascii_case(&asset.SHA256) {
        return Err(invalid("Cached package SHA-256 does not match signed feed"));
    }

    Ok(())
}

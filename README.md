<div align="center">
  <img width="120" src="src-tauri/icons/icon.png" alt="Nexa logo">
  <h1>Nexa</h1>
  <p>A Windows desktop library for organizing images and videos.</p>
  <p>
    <a href="https://github.com/Envo1d/nexa/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/Envo1d/nexa?color=blueviolet"></a>
    <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%2010%2F11-0078d4">
    <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-green"></a>
    <img alt="Built with Rust and Vue" src="https://img.shields.io/badge/built%20with-Rust%20%2B%20Vue-orange">
  </p>
</div>

Nexa brings your media library, incoming files and metadata into one workspace. Built with **Tauri 2, Rust, Vue 3 and TypeScript**, it stores library records in SQLite and supports optional synchronization of filenames and folders with saved metadata.

This repository contains the rewritten Nexa application. The previous implementation is available at **[Envo1d/media-viewer](https://github.com/Envo1d/media-viewer)**.

## Features

- **Library browsing:** grid and list layouts, favorites, multi-selection, file details, date sorting and natural filename sorting.
- **Metadata search:** suggestions for copyrights, artists, characters and tags, with semicolon-separated search terms. Character suggestions include their copyright.
- **Metadata editing:** manage copyrights, an artist, copyright-qualified characters, tags and video titles for individual files or selections. Photo card titles list characters, with tags below.
- **Tags workspace:** browse and rename dictionaries, delete entries with confirmation, and find character names shared across different copyrights.
- **Stage inbox:** scan an incoming folder and distribute files to Library through a metadata wizard. Stage can be nested inside Library and is excluded from Library scans.
- **Optional disk synchronization:** reconcile filenames and directory structure with metadata, including configurable video and GIF subfolders. Progress, recovery and retry support are built in.
- **Native file actions:** open files, reveal them in Explorer, copy paths, delete with recycling where available, and open the Windows share sheet.
- **Previews and indexing:** lazy previews with a bounded disk cache, manual scans and optional scans on startup. Preview and technical-metadata support depend on installed Windows handlers.
- **Legacy import:** import records from the old Nexa database through Settings.
- **Desktop updates:** check manually or on startup, review the available release and confirm installation. Velopack applies verified packages after Nexa closes and restarts the app; update manifests are signed and packages are checked against their SHA-256 hashes.
- **Window persistence:** remember the main window's position and size between normal launches.

External filesystem changes are picked up by scans; a live filesystem watcher is not implemented. Browser mode uses session demo data; native library storage and file operations require the desktop application.

## Installation

Download `Nexa-win-x64-Setup.exe` from **[GitHub Releases](https://github.com/Envo1d/nexa/releases/latest)** and run it. Setup installs in one step for the current user using Velopack.

The installer installs for the current Windows user. If Microsoft Edge WebView2 is missing, setup downloads its bootstrapper, which requires an internet connection.

Existing NSIS installations require a one-time transition: close Nexa, uninstall the old application through Windows Installed apps, then run the new Setup. Preserve your NexaNew settings/database and media folders; no database import is needed. Old shortcuts should be replaced with the shortcuts created by the new setup. Older versions cannot install Velopack packages through their Tauri updater.

## Getting Started

1. Open Nexa and go to **Settings**.
2. Choose your **Library** folder and, optionally, a **Stage** folder for incoming media.
3. In **Indexing**, scan Library and Stage. Enable scanning on startup if desired.
4. Browse Main, search metadata, mark favorites and edit file metadata.
5. Use Stage's distribution wizard to assign metadata and add incoming files to Library.
6. Enable **Sync with disk** if you want Nexa to organize physical filenames and folders. With it disabled, distribution and edits save metadata without moving files.

To migrate from the previous application, use **Import old database** in Settings. The rewritten application uses separate settings, logs, cache and database storage from the legacy version.

## Development

Windows development requires Node.js/npm, Rust stable, Microsoft C++ Build Tools with the desktop C++ workload, and WebView2.

```powershell
git clone https://github.com/Envo1d/nexa.git
cd nexa
npm install
npm run tauri dev
```

For the browser demo:

```powershell
npm run dev
```

### Validation

```powershell
npm run check:ui
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked -- --test-threads=1
```

### Windows Installer Builds

```powershell
npm run build:windows
```

Installer builds require the .NET 8 runtime (or SDK) and an updater signing key matching `src-tauri/updater.pub`. The helper downloads a pinned, checksum-verified Velopack CLI into the ignored build directory. It reads the existing key from `%LOCALAPPDATA%\NexaNew\signing\updater.key`, or accepts `-SigningKeyPath` / `TAURI_SIGNING_PRIVATE_KEY`. Build artifacts are written under `src-tauri/target/release/bundle/velopack/<version>/`.

GitHub release builds upload Setup, the full `.nupkg`, `releases.win-x64.json` and its `.sig` to a draft release. Publish a higher version to make updates available; published release assets are never overwritten by CI. Development and unpackaged binaries require installation through Setup before checking for package updates.

## Feedback

Report bugs and request features through **[GitHub Issues](https://github.com/Envo1d/nexa/issues)**. Include the application version and steps to reproduce the issue.

## License

Nexa is distributed under the **[MIT License](LICENSE)**. Copyright (c) 2026 Envo1d.

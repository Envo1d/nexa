param(
    [string]$SigningKeyPath = (Join-Path $env:LOCALAPPDATA "NexaNew\signing\updater.key"),
    [string]$ReleaseNotesPath,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$repositoryPath = Split-Path -Parent $PSScriptRoot
$previousLocation = Get-Location
$previousKeyPath = $env:TAURI_SIGNING_PRIVATE_KEY_PATH

try {
    Set-Location -LiteralPath $repositoryPath
    node scripts/validate-release.mjs
    if ($LASTEXITCODE -ne 0) { throw "Release version validation failed" }

    if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
        if (-not (Test-Path -LiteralPath $SigningKeyPath -PathType Leaf)) {
            throw "Updater signing key not found. Supply -SigningKeyPath or TAURI_SIGNING_PRIVATE_KEY."
        }
        $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $SigningKeyPath
    }

    if (-not $SkipBuild) {
        npm.cmd run tauri build -- --ci --no-bundle -- --locked
        if ($LASTEXITCODE -ne 0) { throw "Windows application build failed" }
    }

    $releaseVersion = (Get-Content -LiteralPath package.json -Raw | ConvertFrom-Json).version
    $stagingPath = Join-Path $repositoryPath "src-tauri\target\velopack-stage"
    $outputPath = Join-Path $repositoryPath "src-tauri\target\release\bundle\velopack\$releaseVersion"

    # Stage only the application, never Cargo's entire release directory.
    New-Item -ItemType Directory -Path $stagingPath -Force | Out-Null
    New-Item -ItemType Directory -Path $outputPath -Force | Out-Null
    Copy-Item -LiteralPath "src-tauri\target\release\nexa.exe" -Destination (Join-Path $stagingPath "nexa.exe") -Force

    $packArguments = @(
        "pack", "--packId", "Nexa", "--packVersion", $releaseVersion,
        "--packDir", $stagingPath, "--mainExe", "nexa.exe",
        "--packTitle", "Nexa", "--packAuthors", "Envo1d",
        "--runtime", "win-x64", "--channel", "win-x64",
        "--framework", "webview2", "--delta", "None",
        "--icon", (Join-Path $repositoryPath "src-tauri\icons\icon.ico"),
        "--outputDir", $outputPath
    )
    if ($ReleaseNotesPath) {
        $packArguments += @("--releaseNotes", (Resolve-Path -LiteralPath $ReleaseNotesPath).Path)
    }
    & "$PSScriptRoot\velopack.ps1" @packArguments

    $feedPath = Join-Path $outputPath "releases.win-x64.json"
    npm.cmd run tauri -- signer sign --app-version $releaseVersion $feedPath
    if ($LASTEXITCODE -ne 0) { throw "Velopack feed signing failed" }

    node scripts/validate-velopack.mjs $outputPath
    if ($LASTEXITCODE -ne 0) { throw "Velopack artifact validation failed" }

    Write-Host "Velopack setup and signed update feed: $outputPath"
} finally {
    $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $previousKeyPath
    Set-Location -LiteralPath $previousLocation.Path
}

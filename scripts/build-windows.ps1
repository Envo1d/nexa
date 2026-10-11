param(
    [string]$SigningKeyPath = (Join-Path $env:LOCALAPPDATA "NexaNew\signing\updater.key")
)

$ErrorActionPreference = "Stop"
$repositoryPath = Split-Path -Parent $PSScriptRoot
$previousKey = $env:TAURI_SIGNING_PRIVATE_KEY
$previousLocation = Get-Location

try {
    Set-Location -LiteralPath $repositoryPath
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
        if (-not (Test-Path -LiteralPath $SigningKeyPath -PathType Leaf)) {
            throw "Updater signing key not found. Supply -SigningKeyPath or TAURI_SIGNING_PRIVATE_KEY."
        }
        $env:TAURI_SIGNING_PRIVATE_KEY = $SigningKeyPath
    }
    node scripts/validate-release.mjs
    if ($LASTEXITCODE -ne 0) { throw "Release version validation failed" }
    npm.cmd run tauri build -- --ci --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw "Windows installer build failed" }
} finally {
    $env:TAURI_SIGNING_PRIVATE_KEY = $previousKey
    Set-Location -LiteralPath $previousLocation.Path
}

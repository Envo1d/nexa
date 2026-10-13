param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Arguments
)

$ErrorActionPreference = "Stop"
$cliVersion = "1.2.161"
$repositoryPath = Split-Path -Parent $PSScriptRoot
$cliDirectory = Join-Path $repositoryPath "src-tauri\target\velopack-cli"
$cliPath = Join-Path $cliDirectory "tools\net8.0\any\vpk.dll"

if (-not (Test-Path -LiteralPath $cliPath -PathType Leaf)) {
    New-Item -ItemType Directory -Path $cliDirectory -Force | Out-Null
    $archivePath = Join-Path $cliDirectory "vpk.zip"
    Invoke-WebRequest "https://github.com/velopack/velopack/releases/download/$cliVersion/vpk.$cliVersion.nupkg" -OutFile $archivePath
    $archiveHash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
    if ($archiveHash -ne "2B56CE117F803FC70C103CB423BD040E395E40370F6FF6E10818E9FF9C26A323") {
        throw "Velopack CLI archive checksum mismatch."
    }
    Expand-Archive -LiteralPath $archivePath -DestinationPath $cliDirectory -Force
}

& dotnet $cliPath --skip-updates --legacyConsole --yes @Arguments
if ($LASTEXITCODE -ne 0) { throw "Velopack CLI failed with exit code $LASTEXITCODE" }

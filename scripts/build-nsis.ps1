param(
    [string] $Version = "",
    [string] $Stamp = (Get-Date -Format "yyyyMMdd-HHmm"),
    [switch] $SkipCliBuild,
    [switch] $ConvertMedia
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\_common-build.ps1"

if (-not $Version) {
    $Version = Get-TauriVersion
}

Write-Step "Building frontend"
Invoke-External -FilePath "npm.cmd" -ArgumentList @("run", "build") -WorkingDirectory $script:DesktopDir

Write-Step "Preparing release assets"
$assetArgs = @(
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    (Join-Path $PSScriptRoot "prepare-release-assets.ps1"),
    "-DistDir",
    (Join-Path $script:DesktopDir "dist")
)
if ($ConvertMedia) {
    $assetArgs += "-ConvertMedia"
}
& powershell @assetArgs
if ($LASTEXITCODE -ne 0) {
    throw "Asset preprocessing failed"
}

Initialize-CleanReleaseState -Version $Version -SkipCliBuild:$SkipCliBuild

Write-Step "Building NSIS installer"
$mergedConfig = Join-Path $script:ReleaseBuildDir "tauri.build.nsis.json"
Merge-TauriConfig -OverlayPath (Join-Path $script:TauriDir "tauri.nsis.conf.json") -OutputPath $mergedConfig -DisableBeforeBuildCommand | Out-Null
Invoke-External -FilePath "npm.cmd" -ArgumentList @("exec", "tauri", "--", "build", "--bundles", "nsis", "--config", $mergedConfig, "--ci") -WorkingDirectory $script:DesktopDir

Copy-BundleArtifacts -BundleName "nsis" -Extension "exe" -Channel "nsis" -Version $Version -Stamp $Stamp | Out-Null

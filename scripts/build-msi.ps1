param(
    [string] $Version = "",
    [string] $Stamp = (Get-Date -Format "yyyyMMdd-HHmm"),
    [string] $CertPath = $env:CONDUCTOR_CERT_PATH,
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

Write-Step "Building MSI installer"
$mergedConfig = Join-Path $script:ReleaseBuildDir "tauri.build.msi.json"
Merge-TauriConfig -OverlayPath (Join-Path $script:TauriDir "tauri.msi.conf.json") -OutputPath $mergedConfig -DisableBeforeBuildCommand | Out-Null
Invoke-External -FilePath "npm.cmd" -ArgumentList @("exec", "tauri", "--", "build", "--bundles", "msi", "--config", $mergedConfig, "--ci") -WorkingDirectory $script:DesktopDir

$bundleDirs = @(
    (Join-Path $script:TauriDir "target\release\bundle\msi"),
    (Join-Path $script:Root "target\release\bundle\msi")
)
$msiFiles = @()
foreach ($dir in $bundleDirs) {
    if (Test-Path -LiteralPath $dir) {
        $msiFiles += Get-ChildItem -LiteralPath $dir -Filter "*.msi" -File
    }
}

if ($CertPath) {
    foreach ($msi in $msiFiles) {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "sign-msi.ps1") -MsiPath $msi.FullName -CertPath $CertPath
        if ($LASTEXITCODE -ne 0) {
            throw "MSI signing failed: $($msi.FullName)"
        }
    }
} else {
    Write-Warning "CONDUCTOR_CERT_PATH is not set; MSI will be unsigned"
}

Copy-BundleArtifacts -BundleName "msi" -Extension "msi" -Channel "msi" -Version $Version -Stamp $Stamp | Out-Null

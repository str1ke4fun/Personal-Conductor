param(
    [string] $Version = "",
    [switch] $SkipCliBuild
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\_common-build.ps1"

if (-not $Version) {
    $Version = Get-TauriVersion
}

Initialize-CleanReleaseState -Version $Version -SkipCliBuild:$SkipCliBuild
Write-Host "Release state/template reset for v$Version" -ForegroundColor Green

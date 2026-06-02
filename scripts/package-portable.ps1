param(
    [string]$Version = "0.1.0",
    [string]$Stamp = (Get-Date -Format "yyyyMMdd-HHmm")
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$DesktopDir = Join-Path $Root "apps\desktop"
$ReleaseDir = Join-Path $Root "release"
$ReleaseBinDir = Join-Path $ReleaseDir "bin"
$ReleaseAppDir = Join-Path $ReleaseDir "app"
$ReleaseStateDir = Join-Path $ReleaseDir "state"
$ReleaseStateTemplateDir = Join-Path $ReleaseDir "state-template"
$ReleaseResourcesDir = Join-Path $ReleaseDir "resources"
$TargetReleaseDir = Join-Path $Root "target\release"
$CleanRoot = Join-Path $ReleaseDir "_build\clean-root-release"
$ZipPath = Join-Path $Root ("Personal-Conductor-v{0}-internal-{1}.zip" -f $Version, $Stamp)

function Write-Step($message) {
    Write-Host ""
    Write-Host "==> $message" -ForegroundColor Cyan
}

function Invoke-External {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList,
        [string]$WorkingDirectory
    )

    Push-Location $WorkingDirectory
    try {
        & $FilePath @ArgumentList
        if ($LASTEXITCODE -ne 0) {
            throw "Command failed: $FilePath $($ArgumentList -join ' ')"
        }
    } finally {
        Pop-Location
    }
}

function Reset-Directory {
    param([string]$Path)

    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
    New-Item -ItemType Directory -Path $Path | Out-Null
}

function Sync-Directory {
    param(
        [string]$Source,
        [string]$Destination
    )

    Reset-Directory -Path $Destination
    Get-ChildItem -LiteralPath $Source -Force | Copy-Item -Destination $Destination -Recurse -Force
}

function Clear-PortableState {
    param(
        [string]$StateDir,
        [string]$CleanDbPath,
        [string]$TemplateConfigPath
    )

    New-Item -ItemType Directory -Path $StateDir -Force | Out-Null

    Copy-Item -LiteralPath $CleanDbPath -Destination (Join-Path $StateDir "conductor.sqlite") -Force
    Copy-Item -LiteralPath $TemplateConfigPath -Destination (Join-Path $StateDir "config.json") -Force

    Reset-Directory -Path (Join-Path $StateDir "summaries")

    $dirsToDelete = @(
        "agent-runs",
        "subagent-runs",
        "exec-signals",
        "proposed_prompts",
        "notes",
        "asset-cutouts",
        "run-logs"
    )
    foreach ($relativeDir in $dirsToDelete) {
        $path = Join-Path $StateDir $relativeDir
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path -Recurse -Force
        }
    }

    $filesToDelete = @(
        ".task_signal",
        "conductor.sqlite-shm",
        "conductor.sqlite-wal",
        "conductor.sqlite.lock",
        "inject.log",
        "initiative_state.json",
        "persona_state.json",
        "proposals.json",
        "proposals.json.lock",
        "runtime-api.json",
        "runtime_token.txt",
        "scene_state.json",
        "skills.json"
    )
    foreach ($relativeFile in $filesToDelete) {
        $path = Join-Path $StateDir $relativeFile
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path -Force
        }
    }

    Set-Content -LiteralPath (Join-Path $StateDir "events.ndjson") -Value "" -NoNewline
    Set-Content -LiteralPath (Join-Path $StateDir "on-stop.log") -Value "" -NoNewline
}

Write-Step "Stopping running Personal Conductor processes"
Get-Process -Name "conductor-desktop", "conductor" -ErrorAction SilentlyContinue | Stop-Process -Force

Write-Step "Building frontend"
Invoke-External -FilePath "npm.cmd" -ArgumentList @("run", "build") -WorkingDirectory $DesktopDir

Write-Step "Building release binaries"
Invoke-External -FilePath "cargo" -ArgumentList @("build", "--release", "-p", "conductor-cli") -WorkingDirectory $Root
Invoke-External -FilePath "cargo" -ArgumentList @("build", "--release", "-p", "conductor-desktop", "-j", "1") -WorkingDirectory $Root

Write-Step "Generating a clean release database"
Reset-Directory -Path $CleanRoot
$previousRoot = $env:CONDUCTOR_ROOT
try {
    $env:CONDUCTOR_ROOT = $CleanRoot
    Invoke-External -FilePath (Join-Path $TargetReleaseDir "conductor.exe") -ArgumentList @("proposal", "list") -WorkingDirectory $Root
} finally {
    if ($null -eq $previousRoot) {
        Remove-Item Env:CONDUCTOR_ROOT -ErrorAction SilentlyContinue
    } else {
        $env:CONDUCTOR_ROOT = $previousRoot
    }
}

$cleanDbPath = Join-Path $CleanRoot "state\conductor.sqlite"
$templateConfigPath = Join-Path $ReleaseStateTemplateDir "config.json"

if (-not (Test-Path -LiteralPath $cleanDbPath)) {
    throw "Clean database was not created: $cleanDbPath"
}
Invoke-External -FilePath "python" -ArgumentList @(
    "-c",
    "import sqlite3, sys; conn = sqlite3.connect(sys.argv[1]); conn.execute('PRAGMA wal_checkpoint(TRUNCATE)'); conn.close()",
    $cleanDbPath
) -WorkingDirectory $Root

Write-Step "Syncing portable package payload"
Copy-Item -LiteralPath (Join-Path $TargetReleaseDir "conductor.exe") -Destination (Join-Path $ReleaseBinDir "conductor.exe") -Force
Copy-Item -LiteralPath (Join-Path $TargetReleaseDir "conductor-desktop.exe") -Destination (Join-Path $ReleaseBinDir "conductor-desktop.exe") -Force

Sync-Directory -Source (Join-Path $DesktopDir "dist") -Destination $ReleaseAppDir
Sync-Directory -Source (Join-Path $DesktopDir "public\avatar") -Destination (Join-Path $ReleaseResourcesDir "avatar")
Sync-Directory -Source (Join-Path $DesktopDir "src-tauri\resources\live2d") -Destination (Join-Path $ReleaseResourcesDir "live2d")

Write-Step "Resetting release state"
Clear-PortableState -StateDir $ReleaseStateDir -CleanDbPath $cleanDbPath -TemplateConfigPath $templateConfigPath
Copy-Item -LiteralPath $cleanDbPath -Destination (Join-Path $ReleaseStateTemplateDir "conductor.sqlite") -Force

Write-Step "Creating portable zip"
if (Test-Path -LiteralPath $ZipPath) {
    Remove-Item -LiteralPath $ZipPath -Force
}
$zipInputs = Get-ChildItem -LiteralPath $ReleaseDir -Force |
    Where-Object { $_.Name -ne "_build" } |
    ForEach-Object { $_.FullName }
Compress-Archive -Path $zipInputs -DestinationPath $ZipPath -Force

Write-Step "Done"
Write-Host "Portable package: $ZipPath" -ForegroundColor Green

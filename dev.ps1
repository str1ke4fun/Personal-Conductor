# Personal Conductor - Development Environment Launcher
# Usage: .\dev.ps1 [--release]

param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
$DevPort = 1420

function Write-Step($step, $total, $msg) {
    Write-Host "[$step/$total] " -ForegroundColor Cyan -NoNewline
    Write-Host $msg
}

function Write-OK($msg) {
    Write-Host "  [OK] $msg" -ForegroundColor Green
}

function Write-Fail($msg) {
    Write-Host "  [X] $msg" -ForegroundColor Red
    exit 1
}

function Stop-DevSession($Port) {
    $listenerPids = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty OwningProcess -Unique

    foreach ($listenerPid in $listenerPids) {
        if (-not $listenerPid) {
            continue
        }

        Write-Host "  Killing PID $listenerPid (port $Port)" -ForegroundColor Yellow
        taskkill /PID $listenerPid /T /F *> $null
    }

    taskkill /IM conductor-desktop.exe /T /F *> $null
    Start-Sleep -Seconds 1

    $remainingPids = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty OwningProcess -Unique
    if ($remainingPids) {
        Write-Fail "Port $Port is still in use by PID(s): $($remainingPids -join ', ')"
    }
}

Write-Host "`nPersonal Conductor - Development Mode" -ForegroundColor Magenta
Write-Host "======================================`n" -ForegroundColor Magenta

# Step 0: Clean up leftover dev listeners
Write-Step 0 3 "Cleaning up previous session..."
Stop-DevSession $DevPort
Write-OK "Environment clean"

# Step 1: Check prerequisites
Write-Step 1 3 "Checking prerequisites..."

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Fail "Node.js not found. Install from https://nodejs.org/"
}
Write-OK "Node.js $(node --version)"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Fail "Rust/Cargo not found. Install from https://rustup.rs/"
}
Write-OK "cargo $(cargo --version)"

# Step 2: Install npm dependencies
Write-Step 2 3 "Installing dependencies..."

$desktopDir = Join-Path $Root "apps\desktop"
$nodeModules = Join-Path $desktopDir "node_modules"

if (-not (Test-Path $nodeModules) -or (Get-ChildItem $nodeModules -ErrorAction SilentlyContinue | Measure-Object).Count -eq 0) {
    Push-Location $desktopDir
    try {
        npm install
        if ($LASTEXITCODE -ne 0) {
            Write-Fail "npm install failed"
        }
    } finally {
        Pop-Location
    }
    Write-OK "Dependencies installed"
} else {
    Write-OK "Dependencies already installed"
}

# Step 3: Set environment and start
Write-Step 3 3 "Starting Tauri dev..."

$env:CONDUCTOR_ROOT = $Root
$env:PATH = "$Root\target\debug;$env:PATH"

Write-Host "`nLaunching application...`n" -ForegroundColor Green

Push-Location $desktopDir
$tauriArgs = @("tauri", "dev")
if ($Release) {
    $tauriArgs += "--release"
}

try {
    & npx @tauriArgs
    $tauriExit = $LASTEXITCODE
} finally {
    Pop-Location
}

if ($tauriExit -ne 0) {
    exit $tauriExit
}

Write-Host "`nDevelopment session ended.`n" -ForegroundColor Magenta

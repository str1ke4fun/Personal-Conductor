param(
    [string]$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Push-Location (Join-Path $Root "apps\desktop")
try {
    npm install
    npm run build
    npm run tauri build
}
finally {
    Pop-Location
}


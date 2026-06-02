$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$desktopRoot = Join-Path $repoRoot "apps\desktop"
$outLog = Join-Path $desktopRoot ".tauri-dev.out.log"
$errLog = Join-Path $desktopRoot ".tauri-dev.err.log"

foreach ($process in Get-Process -Name "conductor-desktop" -ErrorAction SilentlyContinue) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
}

$vitePortPid = (netstat -ano -p tcp |
    Select-String -Pattern "127\.0\.0\.1:1420\s+.*LISTENING\s+(\d+)" |
    ForEach-Object { [int]$_.Matches[0].Groups[1].Value } |
    Select-Object -First 1)

if ($vitePortPid) {
    Stop-Process -Id $vitePortPid -Force -ErrorAction SilentlyContinue
}

Remove-Item -LiteralPath $outLog, $errLog -ErrorAction SilentlyContinue

Start-Process `
    -FilePath "npm.cmd" `
    -ArgumentList @("run", "tauri", "--", "dev") `
    -WorkingDirectory $desktopRoot `
    -RedirectStandardOutput $outLog `
    -RedirectStandardError $errLog `
    -WindowStyle Hidden

Write-Host "Desktop dev server starting from $desktopRoot"
Write-Host "Logs:"
Write-Host "  $outLog"
Write-Host "  $errLog"

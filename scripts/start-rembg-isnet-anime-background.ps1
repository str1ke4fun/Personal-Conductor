$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $repoRoot "2Dworkspace\rembg-cutouts"
$runner = Join-Path $repoRoot "scripts\run-rembg-isnet-anime.ps1"
$stdout = Join-Path $outDir "isnet-anime-background.stdout.log"
$stderr = Join-Path $outDir "isnet-anime-background.stderr.log"

New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$process = Start-Process `
    -FilePath "powershell.exe" `
    -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $runner) `
    -WindowStyle Hidden `
    -RedirectStandardOutput $stdout `
    -RedirectStandardError $stderr `
    -PassThru

[PSCustomObject]@{
    Id = $process.Id
    ProcessName = $process.ProcessName
    Output = Join-Path $outDir "7xX_7umRXHBq.isnet-anime-rerun.png"
    Preview = Join-Path $outDir "7xX_7umRXHBq.isnet-anime-rerun.preview-dark.png"
    Log = Join-Path $outDir "isnet-anime-rerun.log"
    ErrorLog = Join-Path $outDir "isnet-anime-rerun.err.log"
    BackgroundStdout = $stdout
    BackgroundStderr = $stderr
}

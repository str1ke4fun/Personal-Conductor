$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

$source = Join-Path $repoRoot "2Dworkspace\7xX_7umRXHBq.png"
$outDir = Join-Path $repoRoot "2Dworkspace\rembg-cutouts"
$output = Join-Path $outDir "7xX_7umRXHBq.isnet-anime-rerun.png"
$log = Join-Path $outDir "isnet-anime-rerun.log"
$err = Join-Path $outDir "isnet-anime-rerun.err.log"
$rembg = Join-Path $repoRoot "tools\Rembg\rembg.exe"

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Remove-Item -LiteralPath $output -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath (Join-Path $outDir "7xX_7umRXHBq.isnet-anime-rerun.preview-dark.png") -Force -ErrorAction SilentlyContinue

$process = Start-Process `
    -FilePath $rembg `
    -ArgumentList @("i", "-m", "isnet-anime", $source, $output) `
    -NoNewWindow `
    -Wait `
    -PassThru `
    -RedirectStandardOutput $log `
    -RedirectStandardError $err

if ($process.ExitCode -ne 0) {
    Remove-Item -LiteralPath $output -Force -ErrorAction SilentlyContinue
    throw "rembg exited with code $($process.ExitCode). See $err"
}

if (Test-Path $output) {
    & "C:\Program Files\ImageMagick-7.1.2-Q16-HDRI\magick.exe" `
        $output `
        -background "#202020" `
        -alpha remove `
        -alpha off `
        (Join-Path $outDir "7xX_7umRXHBq.isnet-anime-rerun.preview-dark.png")
}

param(
    [switch]$SkipExisting
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

$sourceDir = Join-Path $repoRoot "2Dworkspace"
$outDir = Join-Path $repoRoot "2Dworkspace\rembg-cutouts"
$rembg = Join-Path $repoRoot "tools\Rembg\rembg.exe"
$magick = "C:\Program Files\ImageMagick-7.1.2-Q16-HDRI\magick.exe"

New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$extensions = @(".png", ".jpg", ".jpeg", ".webp")
$sources = Get-ChildItem -LiteralPath $sourceDir -File |
    Where-Object { $_.Extension.ToLowerInvariant() -in $extensions } |
    Sort-Object Name

foreach ($source in $sources) {
    $stem = [IO.Path]::GetFileNameWithoutExtension($source.Name)
    $output = Join-Path $outDir "$stem.isnet-anime.png"
    $preview = Join-Path $outDir "$stem.isnet-anime.preview-dark.png"
    $log = Join-Path $outDir "$stem.isnet-anime.log"
    $err = Join-Path $outDir "$stem.isnet-anime.err.log"

    if ($SkipExisting -and (Test-Path $output) -and (Test-Path $preview)) {
        Write-Host "skip existing: $($source.Name)"
        continue
    }

    Remove-Item -LiteralPath $output -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $preview -Force -ErrorAction SilentlyContinue

    Write-Host "rembg isnet-anime: $($source.Name)"
    $process = Start-Process `
        -FilePath $rembg `
        -ArgumentList @("i", "-m", "isnet-anime", $source.FullName, $output) `
        -NoNewWindow `
        -Wait `
        -PassThru `
        -RedirectStandardOutput $log `
        -RedirectStandardError $err

    if ($process.ExitCode -ne 0) {
        Remove-Item -LiteralPath $output -Force -ErrorAction SilentlyContinue
        Write-Warning "failed: $($source.Name), exit=$($process.ExitCode), err=$err"
        continue
    }

    if (Test-Path $output) {
        & $magick `
            $output `
            -background "#202020" `
            -alpha remove `
            -alpha off `
            $preview
    }
}

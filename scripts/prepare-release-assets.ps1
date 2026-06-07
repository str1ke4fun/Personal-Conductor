param(
    [string] $DistDir = "apps/desktop/dist",
    [switch] $ConvertMedia
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
if (-not [System.IO.Path]::IsPathRooted($DistDir)) {
    $DistDir = Join-Path $Root $DistDir
}

if (-not (Test-Path -LiteralPath $DistDir)) {
    Write-Warning "Dist directory does not exist, skipping asset preprocessing: $DistDir"
    return
}

function Remove-IfExists {
    param([string] $Path)
    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
}

$toDelete = @(
    "live2d/hiyori/hiyori_free_en.zip",
    "live2d/hiyori/hiyori_pro_en.zip",
    "live2d/hiyori/hiyori_free_en/ReadMe.txt",
    "live2d/hiyori/hiyori_pro_en/ReadMe.txt"
)

foreach ($relative in $toDelete) {
    Remove-IfExists -Path (Join-Path $DistDir $relative)
}

Get-ChildItem -Path $DistDir -Recurse -File -Include "*.cmo3", "*.can3" -ErrorAction SilentlyContinue |
    Remove-Item -Force

if ($ConvertMedia) {
    $cwebp = Get-Command cwebp -ErrorAction SilentlyContinue
    if ($cwebp) {
        Get-ChildItem -Path $DistDir -Recurse -File -Include "*.png" | ForEach-Object {
            $output = [System.IO.Path]::ChangeExtension($_.FullName, ".webp")
            & $cwebp.Source -z 9 -lossless $_.FullName -o $output
            if ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $output)) {
                Remove-Item -LiteralPath $_.FullName -Force
            }
        }
    } else {
        Write-Warning "cwebp was not found; PNG conversion skipped"
    }

    $ffmpeg = Get-Command ffmpeg -ErrorAction SilentlyContinue
    if ($ffmpeg) {
        Get-ChildItem -Path $DistDir -Recurse -File -Include "*.mp4" | ForEach-Object {
            $output = [System.IO.Path]::ChangeExtension($_.FullName, ".webm")
            & $ffmpeg.Source -y -i $_.FullName -c:v libvpx-vp9 -crf 40 -b:v 0 -row-mt 1 -an $output
            if ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $output)) {
                Remove-Item -LiteralPath $_.FullName -Force
            }
        }
    } else {
        Write-Warning "ffmpeg was not found; MP4 conversion skipped"
    }
} else {
    Write-Host "Media conversion skipped. Pass -ConvertMedia after frontend references are updated." -ForegroundColor Yellow
}

$total = (Get-ChildItem -Path $DistDir -Recurse -File | Measure-Object Length -Sum).Sum
Write-Host ("dist after preprocess: {0:N1} MB" -f ($total / 1MB)) -ForegroundColor Green

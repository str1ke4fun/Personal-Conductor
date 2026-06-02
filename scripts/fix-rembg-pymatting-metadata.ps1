$ErrorActionPreference = "Stop"

$rembgRoot = if ($args.Count -gt 0) { $args[0] } else { "C:\Program Files (x86)\Rembg" }
$distInfo = Join-Path $rembgRoot "_internal\pymatting-1.1.14.dist-info"
New-Item -ItemType Directory -Force -Path $distInfo | Out-Null

@"
Metadata-Version: 2.1
Name: pymatting
Version: 1.1.14
Summary: Metadata shim for bundled rembg executable.
"@ | Set-Content -LiteralPath (Join-Path $distInfo "METADATA") -Encoding UTF8

@"
Wheel-Version: 1.0
Generator: local-metadata-shim
Root-Is-Purelib: true
Tag: py3-none-any
"@ | Set-Content -LiteralPath (Join-Path $distInfo "WHEEL") -Encoding UTF8

& (Join-Path $rembgRoot "rembg.exe") --help

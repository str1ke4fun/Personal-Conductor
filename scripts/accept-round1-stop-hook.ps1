param(
    [string]$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [string]$AcceptanceRoot = "",
    [switch]$Release
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-Path {
    param(
        [string]$Path,
        [string]$Message
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw $Message
    }
}

function Assert-TextContains {
    param(
        [string]$Path,
        [string]$Needle,
        [string]$Message
    )

    $text = Get-Content -LiteralPath $Path -Raw
    if (-not $text.Contains($Needle)) {
        throw $Message
    }
}

if ([string]::IsNullOrWhiteSpace($AcceptanceRoot)) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $AcceptanceRoot = Join-Path $Root ".acceptance\round1-stop-hook-$stamp-$PID"
}

$AcceptanceRoot = [System.IO.Path]::GetFullPath($AcceptanceRoot)
$stateDir = Join-Path $AcceptanceRoot "state"
$summariesDir = Join-Path $stateDir "summaries"
$mockProject = Join-Path $AcceptanceRoot "mock-project"
$transcriptPath = Join-Path $AcceptanceRoot "mock-transcript.jsonl"
$payloadPath = Join-Path $AcceptanceRoot "mock-stop-payload.json"
$docPath = Join-Path $mockProject "doc-A.md"

New-Item -ItemType Directory -Force -Path $summariesDir, $mockProject | Out-Null

@"
# Doc A

Second paragraph rewritten in a more conversational tone.
"@ | Set-Content -LiteralPath $docPath -Encoding UTF8

$transcriptLines = @(
    @{ type = "user"; message = @{ role = "user"; content = "Please make doc-A.md easier to read." } },
    @{ type = "assistant"; message = @{ role = "assistant"; content = @(@{ type = "text"; text = "Updated doc-A.md and made the second paragraph more conversational." }) } }
)
$transcriptLines |
    ForEach-Object { $_ | ConvertTo-Json -Compress -Depth 8 } |
    Set-Content -LiteralPath $transcriptPath -Encoding UTF8

$payload = @{
    hook_event_name = "Stop"
    session_id = "mock-round1-session"
    transcript_path = $transcriptPath
    cwd = $mockProject
}
$payloadJson = $payload | ConvertTo-Json -Compress -Depth 8
$payloadJson | Set-Content -LiteralPath $payloadPath -Encoding UTF8

$profile = if ($Release) { "release" } else { "debug" }
$targetDir = if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    Join-Path $Root "target"
}
else {
    $env:CARGO_TARGET_DIR
}
$exe = Join-Path $targetDir "$profile\conductor.exe"
if (-not (Test-Path -LiteralPath $exe)) {
    $buildArgs = @("build", "-p", "conductor-cli")
    if ($Release) {
        $buildArgs += "--release"
    }
    Push-Location $Root
    try {
        & cargo @buildArgs
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}

$oldConductorRoot = $env:CONDUCTOR_ROOT
$env:CONDUCTOR_ROOT = $AcceptanceRoot
try {
    $payloadJson | & $exe hook stop
    if ($LASTEXITCODE -ne 0) {
        throw "conductor hook stop failed with exit code $LASTEXITCODE"
    }
}
finally {
    if ($null -eq $oldConductorRoot) {
        Remove-Item Env:\CONDUCTOR_ROOT -ErrorAction SilentlyContinue
    }
    else {
        $env:CONDUCTOR_ROOT = $oldConductorRoot
    }
}

$tasksMd = Join-Path $stateDir "tasks.md"
$events = Join-Path $stateDir "events.ndjson"
Assert-Path $tasksMd "state/tasks.md was not created"
Assert-TextContains $tasksMd "review-doc" "state/tasks.md does not contain review-doc"
Assert-Path $events "state/events.ndjson was not created"

$eventLines = @(Get-Content -LiteralPath $events | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
if ($eventLines.Count -lt 1) {
    throw "state/events.ndjson is empty"
}

foreach ($line in $eventLines) {
    $null = $line | ConvertFrom-Json
}

$summaryFiles = @(Get-ChildItem -LiteralPath $summariesDir -File -Filter "*.md")
if ($summaryFiles.Count -lt 1) {
    throw "no summary markdown file was created under state/summaries"
}

Write-Host "Round1 Stop hook acceptance passed."
Write-Host "Acceptance root: $AcceptanceRoot"
Write-Host "Transcript: $transcriptPath"
Write-Host "Payload: $payloadPath"
Write-Host "Tasks: $tasksMd"
Write-Host "Events: $events"
Write-Host "Summary: $($summaryFiles[0].FullName)"

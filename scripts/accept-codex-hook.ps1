param(
    [string]$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [string]$AcceptanceRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($AcceptanceRoot)) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $AcceptanceRoot = Join-Path $Root ".acceptance\codex-hook-$stamp-$PID"
}

$AcceptanceRoot = [System.IO.Path]::GetFullPath($AcceptanceRoot)
$work = Join-Path $AcceptanceRoot "work"
New-Item -ItemType Directory -Force -Path $work | Out-Null
Set-Content -Encoding UTF8 -Path (Join-Path $work "doc.md") -Value "draft"

$transcript = Join-Path $AcceptanceRoot "transcript.jsonl"
Set-Content -Encoding UTF8 -Path $transcript -Value '{"role":"assistant","text":"codex finished the requested update"}'

$oldRoot = $env:CONDUCTOR_ROOT
$env:CONDUCTOR_ROOT = $AcceptanceRoot
try {
    Push-Location $Root
    try {
        $savedEap = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        cargo build -p conductor-cli 2>$null | Out-Null
        $ErrorActionPreference = $savedEap
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }

        $promptPayload = @{
            cwd = $work
            session_id = "codex-smoke-session"
            terminal_hint = "codex-smoke-terminal"
            user_message = "Refactor the authentication module"
            transcript_path = $transcript
        } | ConvertTo-Json -Compress

        $promptOutput = $promptPayload | .\target\debug\conductor.exe hook codex user-prompt-submit 2>$null
        if ($promptOutput) {
            throw "codex user-prompt-submit hook must stay silent; stdout would disturb Codex context. Output: $promptOutput"
        }

        $afterPrompt = .\target\debug\conductor.exe list --all
        if (-not ($afterPrompt -match "InProgress")) {
            throw "codex user-prompt-submit did not create an in_progress task."
        }

        $taskId = (($afterPrompt | Select-Object -First 1) -split " ")[0]
        $shown = .\target\debug\conductor.exe show $taskId
        $shownText = $shown -join "`n"
        if (-not ($shownText -match '"source":\s*"codex"')) {
            throw "Task created by codex user-prompt-submit does not have source=codex. Got: $shownText"
        }

        $permPayload = @{
            cwd = $work
            session_id = "codex-smoke-session"
            terminal_hint = "codex-smoke-terminal"
            permission_summary = "Requested write access to src/auth.rs"
        } | ConvertTo-Json -Compress

        $permOutput = $permPayload | .\target\debug\conductor.exe hook codex permission-request 2>$null
        if ($permOutput) {
            throw "codex permission-request hook must stay silent. Output: $permOutput"
        }

        $afterPerm = .\target\debug\conductor.exe show $taskId
        $afterPermText = $afterPerm -join "`n"
        if (-not ($afterPermText -match "write access to src/auth.rs")) {
            throw "codex permission-request did not update permission_summary. Got: $afterPermText"
        }

        $toolPayload = @{
            cwd = $work
            session_id = "codex-smoke-session"
            terminal_hint = "codex-smoke-terminal"
            tool_name = "Bash"
            decision = "approved"
        } | ConvertTo-Json -Compress

        $toolOutput = $toolPayload | .\target\debug\conductor.exe hook codex post-tool-use 2>$null
        if ($toolOutput) {
            throw "codex post-tool-use hook must stay silent. Output: $toolOutput"
        }

        $afterTool = .\target\debug\conductor.exe show $taskId
        $afterToolText = $afterTool -join "`n"
        if (-not ($afterToolText -match "Bash")) {
            throw "codex post-tool-use did not update task context. Got: $afterToolText"
        }

        $stopPayload = @{
            cwd = $work
            session_id = "codex-smoke-session"
            terminal_hint = "codex-smoke-terminal"
            transcript_path = $transcript
            summary = "Codex finished refactoring the authentication module"
        } | ConvertTo-Json -Compress

        $stopOutput = $stopPayload | .\target\debug\conductor.exe hook codex stop 2>$null
        if ($stopOutput) {
            throw "codex stop hook must stay silent. Output: $stopOutput"
        }

        $afterStop = .\target\debug\conductor.exe list --all
        if (-not ($afterStop -match "Pending")) {
            throw "codex stop hook did not update the task to pending."
        }

        $afterStopShown = .\target\debug\conductor.exe show $taskId
        $afterStopText = $afterStopShown -join "`n"
        if (-not ($afterStopText -match "codex-smoke-session")) {
            throw "Task did not retain Codex session_id."
        }
        if (-not ($afterStopText -match "Codex finished refactoring")) {
            throw "Task did not retain Codex completion summary."
        }
    }
    finally {
        Pop-Location
    }
}
finally {
    if ($null -eq $oldRoot) {
        Remove-Item Env:\CONDUCTOR_ROOT -ErrorAction SilentlyContinue
    }
    else {
        $env:CONDUCTOR_ROOT = $oldRoot
    }
}

Write-Host "Codex hook smoke test passed."
Write-Host "Acceptance root: $AcceptanceRoot"

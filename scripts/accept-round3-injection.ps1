param(
    [string]$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [string]$AcceptanceRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($AcceptanceRoot)) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $AcceptanceRoot = Join-Path $Root ".acceptance\round3-claude-task-flow-$stamp-$PID"
}

$AcceptanceRoot = [System.IO.Path]::GetFullPath($AcceptanceRoot)
$work = Join-Path $AcceptanceRoot "work"
New-Item -ItemType Directory -Force -Path $work | Out-Null
Set-Content -Encoding UTF8 -Path (Join-Path $work "doc.md") -Value "draft"

$transcript = Join-Path $AcceptanceRoot "transcript.jsonl"
Set-Content -Encoding UTF8 -Path $transcript -Value '{"role":"assistant","text":"finished the requested draft update"}'

$oldRoot = $env:CONDUCTOR_ROOT
$env:CONDUCTOR_ROOT = $AcceptanceRoot
try {
    Push-Location $Root
    try {
        cargo build -p conductor-cli | Out-Null

        $promptPayload = @{
            cwd = $work
            session_id = "accept-round3-session"
            terminal_hint = "accept-round3-terminal"
            user_message = "实现 Claude Code 任务闭环"
            transcript_path = $transcript
        } | ConvertTo-Json -Compress

        $promptOutput = $promptPayload | .\target\debug\conductor.exe hook user-prompt-submit
        if ($promptOutput) {
            throw "UserPromptSubmit hook must stay silent; stdout would enter Claude Code context."
        }

        $afterPrompt = .\target\debug\conductor.exe list --all
        if (-not ($afterPrompt -match "InProgress")) {
            throw "UserPromptSubmit did not create an in_progress task."
        }

        Start-Sleep -Milliseconds 1000
        Set-Content -Encoding UTF8 -Path (Join-Path $work "doc.md") -Value "draft updated"

        $stopPayload = @{
            cwd = $work
            session_id = "accept-round3-session"
            terminal_hint = "accept-round3-terminal"
            transcript_path = $transcript
            summary = "Claude finished the requested draft update"
        } | ConvertTo-Json -Compress

        $stopOutput = $stopPayload | .\target\debug\conductor.exe hook stop
        if ($stopOutput) {
            throw "Stop hook must stay silent."
        }

        $afterStop = .\target\debug\conductor.exe list --all
        if (-not ($afterStop -match "Pending")) {
            throw "Stop hook did not update the same task to pending."
        }

        $taskId = (($afterStop | Select-Object -First 1) -split " ")[0]
        $shown = .\target\debug\conductor.exe show $taskId
        if (-not (($shown -join "`n") -match "accept-round3-session")) {
            throw "Task did not retain Claude session_id."
        }
        if (-not (($shown -join "`n") -match "Claude finished the requested draft update")) {
            throw "Task did not retain Claude completion summary."
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

Write-Host "Round3 Claude task flow acceptance passed."
Write-Host "Acceptance root: $AcceptanceRoot"

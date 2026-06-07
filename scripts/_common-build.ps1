$ErrorActionPreference = "Stop"

$script:Root = Split-Path -Parent $PSScriptRoot
$script:DesktopDir = Join-Path $script:Root "apps\desktop"
$script:TauriDir = Join-Path $script:DesktopDir "src-tauri"
$script:ReleaseDir = Join-Path $script:Root "release"
$script:ReleaseBuildDir = Join-Path $script:ReleaseDir "_build"
$script:ReleaseArtifactsDir = Join-Path $script:ReleaseDir "artifacts"
$script:ReleaseStateDir = Join-Path $script:ReleaseDir "state"
$script:ReleaseStateTemplateDir = Join-Path $script:ReleaseDir "state-template"
$script:BundledStateTemplateDir = Join-Path $script:TauriDir "resources\state-template"
$script:TargetReleaseDir = Join-Path $script:Root "target\release"

function Write-Step {
    param([Parameter(Mandatory)] [string] $Message)

    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Write-Utf8File {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [AllowEmptyString()] [string] $Content
    )

    $parent = Split-Path -Parent $Path
    if ($parent) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    $encoding = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function Get-FullPath {
    param([Parameter(Mandatory)] [string] $Path)
    return [System.IO.Path]::GetFullPath($Path)
}

function Assert-UnderProjectRoot {
    param([Parameter(Mandatory)] [string] $Path)

    $root = Get-FullPath -Path $script:Root
    $full = Get-FullPath -Path $Path
    $prefix = if ($root.EndsWith("\")) { $root } else { "$root\" }
    if ($full -ne $root -and -not $full.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to modify path outside project root: $full"
    }
}

function Invoke-External {
    param(
        [Parameter(Mandatory)] [string] $FilePath,
        [string[]] $ArgumentList = @(),
        [string] $WorkingDirectory = $script:Root
    )

    Push-Location $WorkingDirectory
    try {
        & $FilePath @ArgumentList
        if ($LASTEXITCODE -ne 0) {
            throw "Command failed with exit code ${LASTEXITCODE}: $FilePath $($ArgumentList -join ' ')"
        }
    } finally {
        Pop-Location
    }
}

function Reset-Directory {
    param([Parameter(Mandatory)] [string] $Path)

    Assert-UnderProjectRoot -Path $Path
    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
}

function Get-TauriVersion {
    $configPath = Join-Path $script:TauriDir "tauri.conf.json"
    $config = Read-JsonFile -Path $configPath
    return [string] $config.version
}

function Read-JsonFile {
    param([Parameter(Mandatory)] [string] $Path)
    return Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
}

function ConvertTo-PlainData {
    param($Value)

    if ($null -eq $Value) {
        return $null
    }

    if ($Value -is [System.Management.Automation.PSCustomObject]) {
        $result = [ordered]@{}
        foreach ($property in $Value.PSObject.Properties) {
            $result[$property.Name] = ConvertTo-PlainData -Value $property.Value
        }
        return $result
    }

    if ($Value -is [System.Collections.IDictionary]) {
        $result = [ordered]@{}
        foreach ($key in $Value.Keys) {
            $result[$key] = ConvertTo-PlainData -Value $Value[$key]
        }
        return $result
    }

    if ($Value -is [System.Collections.IEnumerable] -and $Value -isnot [string]) {
        $items = @()
        foreach ($item in $Value) {
            $items += ,(ConvertTo-PlainData -Value $item)
        }
        return ,$items
    }

    return $Value
}

function Merge-Data {
    param(
        [Parameter(Mandatory)] $Base,
        [Parameter(Mandatory)] $Overlay
    )

    if ($Base -is [System.Collections.IDictionary] -and $Overlay -is [System.Collections.IDictionary]) {
        $merged = [ordered]@{}
        foreach ($key in $Base.Keys) {
            $merged[$key] = $Base[$key]
        }
        foreach ($key in $Overlay.Keys) {
            if ($merged.Contains($key)) {
                $merged[$key] = Merge-Data -Base $merged[$key] -Overlay $Overlay[$key]
            } else {
                $merged[$key] = $Overlay[$key]
            }
        }
        return $merged
    }

    if ($Overlay -is [System.Collections.IEnumerable] -and $Overlay -isnot [string]) {
        return ,$Overlay
    }

    return $Overlay
}

function Merge-TauriConfig {
    param(
        [Parameter(Mandatory)] [string] $OverlayPath,
        [Parameter(Mandatory)] [string] $OutputPath,
        [switch] $DisableBeforeBuildCommand
    )

    $basePath = Join-Path $script:TauriDir "tauri.conf.json"
    $base = ConvertTo-PlainData -Value (Read-JsonFile -Path $basePath)
    $overlay = ConvertTo-PlainData -Value (Read-JsonFile -Path $OverlayPath)
    $merged = Merge-Data -Base $base -Overlay $overlay

    if ($DisableBeforeBuildCommand) {
        if (-not $merged.Contains("build") -or $null -eq $merged["build"]) {
            $merged["build"] = [ordered]@{}
        }
        $merged["build"]["beforeBuildCommand"] = $null
        $merged["build"]["beforeBundleCommand"] = $null
    }

    $json = $merged | ConvertTo-Json -Depth 100
    Write-Utf8File -Path $OutputPath -Content $json
    return $OutputPath
}

function Save-TemplateConfigSnapshot {
    param([Parameter(Mandatory)] [string] $Destination)

    $preferred = Join-Path $script:ReleaseStateTemplateDir "config.json"
    $fallback = Join-Path $script:Root "state\config.json"
    $source = if (Test-Path -LiteralPath $preferred) { $preferred } elseif (Test-Path -LiteralPath $fallback) { $fallback } else { $null }
    if (-not $source) {
        throw "No config template found. Expected $preferred or $fallback"
    }

    $config = Read-JsonFile -Path $source
    if ($config.PSObject.Properties.Name -contains "llm" -and $null -ne $config.llm) {
        $config.llm.apiKey = $null
        $config.llm.apiKeySet = $false
    }

    $json = $config | ConvertTo-Json -Depth 100
    Write-Utf8File -Path $Destination -Content $json
    return $Destination
}

function Invoke-SqliteCheckpoint {
    param([Parameter(Mandatory)] [string] $DbPath)

    $python = Get-Command python -ErrorAction SilentlyContinue
    if (-not $python) {
        Write-Warning "python was not found; skipping SQLite WAL checkpoint"
        return
    }

    $code = @'
import sqlite3
import sys

db = sys.argv[1]
conn = sqlite3.connect(db)
conn.execute('PRAGMA wal_checkpoint(TRUNCATE)')
conn.close()
'@
    & $python.Source -c $code $DbPath
    if ($LASTEXITCODE -ne 0) {
        throw "SQLite checkpoint failed: $DbPath"
    }
}

function Invoke-CleanDatabase {
    param(
        [Parameter(Mandatory)] [string] $CleanRoot,
        [switch] $SkipCliBuild
    )

    Reset-Directory -Path $CleanRoot

    if (-not $SkipCliBuild) {
        Write-Step "Building conductor CLI for clean database generation"
        Invoke-External -FilePath "cargo" -ArgumentList @("build", "--release", "-p", "conductor-cli") -WorkingDirectory $script:Root
    }

    $conductorExe = Join-Path $script:TargetReleaseDir "conductor.exe"
    if (-not (Test-Path -LiteralPath $conductorExe)) {
        throw "conductor.exe is missing: $conductorExe"
    }

    Write-Step "Generating clean SQLite database"
    $previousRoot = $env:CONDUCTOR_ROOT
    try {
        $env:CONDUCTOR_ROOT = $CleanRoot
        Invoke-External -FilePath $conductorExe -ArgumentList @("proposal", "list") -WorkingDirectory $script:Root
    } finally {
        if ($null -eq $previousRoot) {
            Remove-Item Env:CONDUCTOR_ROOT -ErrorAction SilentlyContinue
        } else {
            $env:CONDUCTOR_ROOT = $previousRoot
        }
    }

    $cleanDbPath = Join-Path $CleanRoot "state\conductor.sqlite"
    if (-not (Test-Path -LiteralPath $cleanDbPath)) {
        throw "Clean database was not created: $cleanDbPath"
    }

    Invoke-SqliteCheckpoint -DbPath $cleanDbPath
    return $cleanDbPath
}

function Reset-CleanStatePayload {
    param(
        [Parameter(Mandatory)] [string] $StateDir,
        [Parameter(Mandatory)] [string] $CleanDbPath,
        [Parameter(Mandatory)] [string] $TemplateConfigPath,
        [Parameter(Mandatory)] [string] $Version
    )

    Reset-Directory -Path $StateDir
    Copy-Item -LiteralPath $CleanDbPath -Destination (Join-Path $StateDir "conductor.sqlite") -Force
    Copy-Item -LiteralPath $TemplateConfigPath -Destination (Join-Path $StateDir "config.json") -Force

    New-Item -ItemType Directory -Path (Join-Path $StateDir "summaries") -Force | Out-Null
    Write-Utf8File -Path (Join-Path $StateDir "events.ndjson") -Content ""
    Write-Utf8File -Path (Join-Path $StateDir "on-stop.log") -Content ""
    Write-Utf8File -Path (Join-Path $StateDir "tasks.json") -Content "{`"tasks`":[]}`n"
    Write-Utf8File -Path (Join-Path $StateDir "tasks.md") -Content "# Tasks`n"
    Write-Utf8File -Path (Join-Path $StateDir "release-version.txt") -Content "$Version`n"

    $desktopState = [ordered]@{
        pet = [ordered]@{
            x = $null
            y = $null
            width = 320
            height = 420
            scale = 1.0
            locked = $false
        }
    } | ConvertTo-Json -Depth 10
    Write-Utf8File -Path (Join-Path $StateDir "desktop.json") -Content "$desktopState`n"

    foreach ($suffix in @("-wal", "-shm", ".lock")) {
        $path = Join-Path $StateDir "conductor.sqlite$suffix"
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path -Force
        }
    }
}

function Test-CleanSqlite {
    param([Parameter(Mandatory)] [string] $DbPath)

    $python = Get-Command python -ErrorAction SilentlyContinue
    if (-not $python) {
        Write-Warning "python was not found; skipping SQLite row-count verification"
        return
    }

    $code = @'
import sqlite3
import sys

db = sys.argv[1]
tables = [
    'tasks',
    'task_lists',
    'agent_tasklist_items',
    'chat_messages',
    'chat_sessions',
    'chat_turns',
    'chat_turn_events',
    'chat_message_projections',
    'memory_candidates',
    'memory_entries',
    'conversation_summaries',
    'memory_chunks',
    'memory_embeddings',
    'action_proposals',
    'tool_runs',
    'agent_runs',
    'agent_mailbox_messages',
    'todos',
    'tool_calls',
    'command_runs',
    'codex_sessions',
    'goal_runs',
    'goal_cycles',
    'goal_hints',
    'dispatch_plans',
    'agent_tasks',
    'agent_run_refs',
    'agent_messages',
    'work_leases',
    'agent_heartbeats',
    'runtime_events',
    'workspace_projection_state',
    'route_decisions',
]

conn = sqlite3.connect(db)
existing = {
    row[0]
    for row in conn.execute(
        'select name from sqlite_master where type = \'table\' and name not like \'sqlite_%\''
    )
}
failed = []
for table in tables:
    if table not in existing:
        continue
    count = conn.execute(f'select count(*) from {table}').fetchone()[0]
    print(f'{table}={count}')
    if count:
        failed.append((table, count))
conn.close()
if failed:
    print('Non-empty release database tables:', failed, file=sys.stderr)
    sys.exit(1)
'@
    & $python.Source -c $code $DbPath
    if ($LASTEXITCODE -ne 0) {
        throw "Clean SQLite verification failed: $DbPath"
    }

    foreach ($suffix in @("-wal", "-shm")) {
        $sidecar = "$DbPath$suffix"
        if (Test-Path -LiteralPath $sidecar) {
            throw "SQLite sidecar should not be present in release template: $sidecar"
        }
    }
}

function Initialize-CleanReleaseState {
    param(
        [string] $Version = (Get-TauriVersion),
        [switch] $SkipCliBuild
    )

    New-Item -ItemType Directory -Path $script:ReleaseBuildDir -Force | Out-Null
    $templateConfigSnapshot = Join-Path $script:ReleaseBuildDir "state-template-config.json"
    Save-TemplateConfigSnapshot -Destination $templateConfigSnapshot | Out-Null

    $cleanRoot = Join-Path $script:ReleaseBuildDir "clean-root-release"
    $cleanDbPath = Invoke-CleanDatabase -CleanRoot $cleanRoot -SkipCliBuild:$SkipCliBuild

    Write-Step "Resetting release state templates"
    Reset-CleanStatePayload -StateDir $script:ReleaseStateDir -CleanDbPath $cleanDbPath -TemplateConfigPath $templateConfigSnapshot -Version $Version
    Reset-CleanStatePayload -StateDir $script:ReleaseStateTemplateDir -CleanDbPath $cleanDbPath -TemplateConfigPath $templateConfigSnapshot -Version $Version
    Reset-CleanStatePayload -StateDir $script:BundledStateTemplateDir -CleanDbPath $cleanDbPath -TemplateConfigPath $templateConfigSnapshot -Version $Version
    Write-Utf8File -Path (Join-Path $script:BundledStateTemplateDir ".gitkeep") -Content ""

    Test-CleanSqlite -DbPath (Join-Path $script:ReleaseStateTemplateDir "conductor.sqlite")
    Test-CleanSqlite -DbPath (Join-Path $script:BundledStateTemplateDir "conductor.sqlite")
}

function Copy-BundleArtifacts {
    param(
        [Parameter(Mandatory)] [string] $BundleName,
        [Parameter(Mandatory)] [string] $Extension,
        [Parameter(Mandatory)] [string] $Channel,
        [Parameter(Mandatory)] [string] $Version,
        [Parameter(Mandatory)] [string] $Stamp
    )

    New-Item -ItemType Directory -Path $script:ReleaseArtifactsDir -Force | Out-Null
    $bundleDirs = @(
        (Join-Path $script:TauriDir "target\release\bundle\$BundleName"),
        (Join-Path $script:Root "target\release\bundle\$BundleName")
    )

    $files = @()
    foreach ($dir in $bundleDirs) {
        if (Test-Path -LiteralPath $dir) {
            $files += Get-ChildItem -LiteralPath $dir -Filter "*.$Extension" -File
        }
    }

    if (-not $files -or $files.Count -eq 0) {
        throw "No .$Extension artifacts found for $BundleName"
    }

    $files = $files | Sort-Object LastWriteTime -Descending
    $copied = @()
    for ($i = 0; $i -lt $files.Count; $i++) {
        $file = $files[$i]
        $extra = ""
        if ($files.Count -gt 1) {
            $safeName = $file.BaseName -replace "[^A-Za-z0-9._-]+", "-"
            $extra = "-$safeName"
        }
        $destination = Join-Path $script:ReleaseArtifactsDir ("Personal-Conductor-{0}-v{1}-{2}{3}.{4}" -f $Channel, $Version, $Stamp, $extra, $Extension)
        Copy-Item -LiteralPath $file.FullName -Destination $destination -Force
        $copied += $destination
        Write-Host ("{0}: {1} ({2:N1} MB)" -f $Channel.ToUpperInvariant(), $destination, ($file.Length / 1MB)) -ForegroundColor Green
    }

    return $copied
}

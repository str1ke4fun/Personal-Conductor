# Personal Conductor

Personal Conductor is a Windows-first desktop companion for coding, task triage, and lightweight agent orchestration.
It combines a Tauri desktop shell, a React frontend, a Rust core, local SQLite persistence, and avatar-driven status rendering for an always-on assistant experience.

## What It Does

- Floating desktop companion with tray integration and multiple work surfaces.
- Task, chat, settings, and workbench windows backed by a shared runtime.
- Local task, chat, memory, and goal state stored in SQLite.
- Hook-friendly CLI for external tooling and automation flows.
- Live2D and avatar asset pipeline for status-aware visual presentation.
- Portable Windows packaging flow for internal release builds.

## Repository Layout

```text
apps/desktop/          React + Vite frontend and Tauri desktop shell
crates/conductor-core/ Core runtime, SQLite layer, tools, memory, goals
crates/conductor-cli/  CLI entry point and hook-facing commands
crates/conductor-sense/Foreground and activity sensing support
docs/                  Product, architecture, and implementation notes
release/               Portable release skeleton and release docs
scripts/               Build, packaging, and local helper scripts
```

## Development Environment

- OS: Windows 10/11 x64
- Rust toolchain: stable, workspace edition 2021
- Node.js: 20+ recommended
- npm: required for the desktop frontend

## Run In Development

```powershell
.\dev.ps1
```

Alternative entrypoints:

- `start-dev.bat`
- `cd apps/desktop && npm run build`
- `cargo build -p conductor-cli`
- `cargo build -p conductor-desktop`

The development launchers set `CONDUCTOR_ROOT` to the repository root so runtime state is written under `state/`.

## Build

Frontend:

```powershell
cd apps\desktop
npm run build
```

CLI:

```powershell
cargo build --release -p conductor-cli
```

Desktop:

```powershell
cargo build --release -p conductor-desktop -j 1
```

`-j 1` is currently the safer release build setting for this workspace on Windows.

## Portable Packaging

The repository includes a repeatable portable packaging script:

```powershell
.\scripts\package-portable.ps1
```

This script will:

- build the frontend
- build release binaries
- generate a clean SQLite baseline with schema
- sanitize release state
- sync assets into `release/`
- create a portable zip in the repository root

The latest generated internal package is expected to look like:

```text
Personal-Conductor-v0.1.0-internal-YYYYMMDD-HHMM.zip
```

## Portable Startup

For packaged builds, start the app through:

```text
release\启动 Personal Conductor.cmd
```

That launcher sets `CONDUCTOR_ROOT` to the extracted release directory and bootstraps `state/` from `state-template/` when needed.

## Runtime Data

Runtime state is local-first and includes:

- `state/conductor.sqlite`
- `state/config.json`
- `state/events.ndjson`
- `state/summaries/`

These files are intentionally excluded from source control.

## CI

GitHub Actions CI is defined in `.github/workflows/ci.yml` and currently runs:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- frontend type-check and Vitest

## Notes

- This project is currently Windows-focused.
- Public installer packaging is not fully wired yet; the portable release flow is the current supported distribution route.
- Local secrets, conversation history, and memory state should not be committed.

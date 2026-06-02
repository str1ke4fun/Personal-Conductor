# Personal Conductor

[中文说明](README.zh-CN.md)

![Qinghe desk companion](apps/desktop/public/avatar/document_secretary/board-doc-relex.png)

Personal Conductor is a Windows-first desktop pet companion for coding, task follow-up, and lightweight workspace assistance.
It combines a Tauri desktop shell, a React frontend, a Rust core, local persistence, and avatar-driven status rendering into a desk-side assistant that stays visible while you work.

## What It Is

- A floating desktop pet with tray integration and multiple companion panels
- A workbench window for chat, goals, and task context
- Local-first runtime state stored in SQLite under `state/`
- Avatar and Live2D assets for status-aware presentation
- Portable packaging flow for internal Windows releases

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

## Development

Requirements:

- Windows 10/11 x64
- Rust stable
- Node.js 20+
- npm

Start development mode:

```powershell
.\dev.ps1
```

Useful commands:

```powershell
cd apps\desktop
npm run build

cargo build -p conductor-cli
cargo build -p conductor-desktop
```

## Portable Builds

Create the standard portable zip:

```powershell
.\scripts\package-portable.ps1
```

Packaged builds bootstrap runtime data from `release/state-template/` and then write fresh local data into `state/`.

## Runtime Data

Runtime state is intentionally local and should not be committed:

- `state/conductor.sqlite`
- `state/config.json`
- `state/events.ndjson`
- `state/summaries/`

## Notes

- This project is currently Windows-focused.
- The current distribution route is the portable zip package, not a full installer.
- Release packages should be sanitized before distribution so they do not include local chat history, memory state, or test residue.

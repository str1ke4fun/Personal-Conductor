# Personal Conductor v0.1.1

## Installer changes

- Adds parallel Windows installer tracks: NSIS for internal current-user builds and WiX MSI for formal releases.
- Moves release builds to `%LOCALAPPDATA%\PersonalConductor` by default while preserving `CONDUCTOR_ROOT` for development and scripted clean-state generation.
- Bundles a generated `state-template` for first launch initialization.

## State hygiene

- Release SQLite is generated from migrations in a clean root.
- Chat, goal, agent-run, tool-call, memory, event, and lightweight runtime state are verified empty before packaging.
- Existing user AppData is preserved on upgrade; templates are copied only when a state file is missing.

## GitHub release flow

- `main` builds produce an NSIS artifact.
- `v*.*.*` tags build MSI artifacts and upload them to GitHub Releases.
- MSI signing is automatic when `CONDUCTOR_CERT_BASE64` and `CONDUCTOR_CERT_PASSWORD` secrets are configured.

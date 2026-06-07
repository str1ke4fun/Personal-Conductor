# Release Checklist v0.1.1

- [ ] Run `scripts/reset-release-state.ps1 -Version 0.1.1`.
- [ ] Confirm `release/state-template/conductor.sqlite` has zero rows in memory, chat, goal, agent, tool, and runtime event tables.
- [ ] Run `scripts/build-nsis.ps1 -Version 0.1.1`.
- [ ] Smoke test the NSIS installer on a clean Windows user profile.
- [ ] Run `scripts/build-msi.ps1 -Version 0.1.1`.
- [ ] Sign MSI with Authenticode certificate before public distribution.
- [ ] Push tag `v0.1.1` and verify GitHub Release artifact upload.

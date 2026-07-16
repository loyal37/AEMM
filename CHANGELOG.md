# Changelog

All notable changes to AEMM are documented here. The project follows Keep a Changelog conventions and will adopt semantic versioning once distributable releases begin.

## [Unreleased]

### Added

- Initial product, architecture, data model, workflow, and security design.
- EFMI/3DMigoto loader observations and explicit adaptation boundaries.
- Phase-based implementation backlog and contributor/agent guidance.
- Tauri 2, React, TypeScript, and Vite application foundation with a modern dark desktop shell.
- Dashboard, Mods, mod details, Profiles, and Settings route foundations.
- Typed application bootstrap and settings commands over a thin Tauri transport layer.
- Core ports for games, mods, deployment, conflicts, and profiles.
- Versioned settings with validation, atomic persistence, recovery, and separated managed storage roots.
- Daily rolling structured logs and an async SQLite database with embedded migrations.
- Initial mod/profile/deployment schema and default Profile seed.
- Path-safety utilities and Phase 1 backend tests.
- Endfield registry/known-root discovery with canonical product identity validation.
- EFMI loader validation with separate structure-valid and launch-ready states.
- Persisted game/loader paths, direct and EFMI launch modes, validated directory opening, and shell-free process launch.
- Native folder selection plus game management UI on Settings, Dashboard, and the application sidebar.
- Phase 2 false-positive, stale-loader, persistence, and launch-containment tests.

### Security

- Reject unsafe relative paths, Windows reserved names, parent traversal, managed roots as deletion targets, and overlapping repository/staging roots.
- Revalidate game and loader executables as direct children of canonical working directories immediately before launch.
- Treat every frontend-selected path as untrusted and refuse to persist it until the matching backend adapter succeeds.

### Changed

- Report unknown game versions explicitly instead of presenting Unity engine or launcher application versions as the Endfield game version.

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

### Security

- Reject unsafe relative paths, Windows reserved names, parent traversal, managed roots as deletion targets, and overlapping repository/staging roots.

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
- Owned mod repositories with a versioned marker, canonical path types, and safe direct-child resolution.
- Asynchronous mod inventory scanning with normalized paths, file roles, BLAKE3 hashes, content fingerprints, issue reporting, and persisted incremental Hash reuse.
- Validated author `mod.json` parsing, original-document preservation, safe preview resolution, and deterministic inferred metadata.
- Transactional mod/metadata/file synchronization, missing-mod lifecycle handling, local display/category/description/favorite/notes/tags overrides, and Phase 3 SQLite migration.
- Thin mod scan/list/local-metadata commands with TypeScript DTOs and invoke clients for the upcoming Mods UI.
- Phase 3 repository, metadata, incremental scan, database consistency, duplicate-ID, and 1,000-mod performance regression tests.
- Phase 4 Mods workspace with virtualized card/list views, search, category/status/favorite filters, all required sorting modes, selection, and batch favorites.
- Full mod details with author metadata, local-only overrides/notes/tags, lifecycle state, preview, file statistics, and a virtualized Hash inventory.
- UUID-based detail, safe preview, contained open-directory, and transactional favorite commands.
- Live Dashboard mod totals, favorite counts, and recent-install activity.
- Deterministic browser-preview mod fixtures plus responsive visual/interaction coverage at normal and minimum window sizes.

### Security

- Reject unsafe relative paths, Windows reserved names, parent traversal, managed roots as deletion targets, and overlapping repository/staging roots.
- Revalidate game and loader executables as direct children of canonical working directories immediately before launch.
- Treat every frontend-selected path as untrusted and refuse to persist it until the matching backend adapter succeeds.
- Refuse to adopt non-empty custom repositories without a valid AEMM ownership marker, and skip links, junctions, reparse points, non-regular entries, unsafe relative names, and escaping preview paths during scans.
- Restrict preview reads to contained repository files under 2 MiB with recognized raster signatures; reject SVG/HTML and never accept a frontend filesystem path for preview or directory opening.
- Display author website metadata as untrusted text instead of creating an executable external link.

### Changed

- Report unknown game versions explicitly instead of presenting Unity engine or launcher application versions as the Endfield game version.
- Preserve vanished repository records and user overrides by marking them broken instead of deleting database state during synchronization.
- Keep enabled/conflict UI explicitly unavailable until deployment and analyzer phases can provide truthful state.

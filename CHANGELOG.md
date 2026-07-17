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
- ZIP, 7z, RAR, and folder import adapters with signature detection, dependency/license review, and explicit third-party notices.
- Owned staging roots and UUID operation journals, secure package-root detection, immutable install confirmation plans, duplicate checks, progress events, and cancellation.
- Same-volume atomic repository commits, verified cross-volume partial copies, transactional database synchronization, rollback receipts, and startup recovery.
- Desktop import UX with native archive/folder pickers, window drag-and-drop, progress, warnings/blockers, confirmation, completion, and rollback-aware error handling.
- Phase 5 tests for real ZIP/7z/RAR decoding, Zip Slip, 7z traversal, ZIP symlinks, archive quotas, duplicate IDs, rollback, and interrupted installation recovery.
- EFMI copy deployment strategy with disabled-prefix staging, BLAKE3 verification, manifest ownership, atomic activation/revoke, and startup reconciliation.
- Active Profile persistence, transactional deployment records, single/batch enable-disable, Mods/detail switches, enabled filters, F10 refresh guidance, and Dashboard counts.
- Optional external-package compatibility harness, validated with the checksum-pinned RabbitFX v2.2 EFMI package without redistributing or executing it.
- Versioned target-path and EFMI INI conflict analyzers over immutable active-Profile deployment snapshots.
- Explicit namespace, TextureOverride/ShaderOverride Hash, match/handling, and direct resource-file evidence with official-template false-positive fixtures.
- Live conflict totals, affected-only filtering, card/list markers, and per-mod participant/file/Profile-order details.
- Shared deployment/conflict serialization plus post-enable conflict warnings and TanStack Query cache reconciliation.
- Transactional Profile create, rename, copy, delete, and active-state queries with ordered desired memberships.
- Rollback-capable full-set Profile switching through EFMI revoke tombstones, target fingerprint deployment, and one SQLite activity/manifest commit.
- Profiles workspace, top-bar quick switch, Dashboard active Profile, mutation feedback, and deterministic browser-preview interactions.

- Persisted dark/system theme, locale, log level, and restartable onboarding preferences with live system color-scheme response.
- i18next/react-i18next shell and onboarding localization, with English feature pages explicitly retained as Preview work.
- Skip navigation, visible keyboard focus, reduced-motion support, and delayed global query/mutation progress feedback.
- Pointer and keyboard Profile order editing with accessible move controls and transactional exact-membership persistence.
- Route-level feature code splitting that removes the initial Vite large-chunk warning.

### Security

- Reject unsafe relative paths, Windows reserved names, parent traversal, managed roots as deletion targets, and overlapping repository/staging roots.
- Revalidate game and loader executables as direct children of canonical working directories immediately before launch.
- Treat every frontend-selected path as untrusted and refuse to persist it until the matching backend adapter succeeds.
- Refuse to adopt non-empty custom repositories without a valid AEMM ownership marker, and skip links, junctions, reparse points, non-regular entries, unsafe relative names, and escaping preview paths during scans.
- Restrict preview reads to contained repository files under 2 MiB with recognized raster signatures; reject SVG/HTML and never accept a frontend filesystem path for preview or directory opening.
- Display author website metadata as untrusted text instead of creating an executable external link.
- Reject absolute/UNC/device/archive-parent paths, Windows control/reserved names, case-insensitive file/ancestor collisions, links/reparse points, encrypted or multipart packages, ZIP overlapping data, excessive expansion, and out-of-policy output sizes before installation.
- Never pass archive entry paths to native extraction destinations: all installed files are created through AEMM-controlled `create_new` paths inside marker-owned staging.
- Revalidate the staged fingerprint, logical ID, file count, size, duplicate state, journal ownership, and empty final destination immediately before commit; never overwrite existing repository content.
- Require a verified EFMI `Mods`/`DISABLED*` policy, create deployment files without overwrite, and keep partial/revoke directories excluded from loader discovery.
- Refuse deployment cleanup on marker/root/inventory/Hash mismatch, links or extra paths; delete only manifest-listed files and expected empty parents instead of recursive directory contents.
- Canonicalize and contain every deployed INI read, reject links/reparse components, require the on-disk AEMM marker to match SQLite, and cap conflict parsing at 4 MiB per file, 256 files per mod, and 64 MiB per report.
- Protect the active Profile from deletion, forbid deployment records on inactive Profiles, and re-check switch snapshots inside the final database transaction.

- Refuse path-bearing game or storage changes through the generic preferences command; dedicated validated workflows remain the only path mutation boundary.

### Changed

- Report unknown game versions explicitly instead of presenting Unity engine or launcher application versions as the Endfield game version.
- Preserve vanished repository records and user overrides by marking them broken instead of deleting database state during synchronization.
- Expose AEMM Profile order while leaving the EFMI conflict winner unset until loader precedence is independently verified.
- Process orphan active deployments before revoke tombstones during startup recovery, allowing interrupted Profile switches with shared mod IDs to converge safely.

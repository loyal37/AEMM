# Development Tasks

Status: `[ ]` not started, `[~]` in progress, `[x]` completed.

## Phase 1 — Foundation

- [x] Analyze product requirements and document architecture/data flows.
- [x] Inspect the supplied EFMI loader layout without executing binaries.
- [x] Scaffold Tauri 2 + React + TypeScript + Vite.
- [x] Add application router, dark design system, sidebar, and page shells.
- [x] Add modular Rust directory structure and thin commands.
- [x] Add validated versioned configuration service.
- [x] Add structured rolling file logging.
- [x] Add SQLite pool, WAL/foreign-key settings, and embedded migrations.
- [x] Add typed frontend invoke client and startup status handling.
- [x] Add unit tests for Phase 1 core services.
- [x] Run formatting, build, Clippy, tests, and code/security review.
- [x] Publish the Phase 1 foundation to `loyal37/AEMM`.

## Phase 2 — Game path management

- [x] Define verified CN identity markers; explicitly retain international layout and authoritative game-version sources as open adaptation work.
- [x] Implement `GameAdapter` registry/known-root discovery and canonical validation.
- [x] Implement EFMI loader adapter, stale-launch detection, and configurable direct/EFMI launch modes.
- [x] Add automatic detection, manual folder selection, path persistence, honest version status, open-directory, and launch UI.
- [x] Add tests for false-positive paths, spoofed identity, stale loader configuration, and unsafe executable resolution.
- [x] Run formatting, build, Clippy, tests, desktop startup smoke test, security review, and visual browser-preview review.

## Phase 3 — Mod scanning and database

- [x] Implement repository root ownership and safe path types.
- [x] Implement asynchronous scanner and metadata inference.
- [x] Persist mods, author/local metadata, files, hashes, and timestamps.
- [x] Add incremental scanning and 1,000+ mod performance fixtures.
- [x] Add thin scan/list/local-metadata commands and typed frontend contracts.
- [x] Run formatting, build, strict Clippy, tests, and Phase 3 security/database review.

## Phase 4 — Mods UI

- [x] Implement list/card views, search, filtering, sorting, selection, and favorites.
- [x] Add responsive TanStack Virtual card/list rendering for 1,000+ mod repositories.
- [x] Add mod detail route, original/local metadata separation, local editing, and virtualized files.
- [x] Add UUID-only safe preview/open-directory commands and transactional batch favorite updates.
- [x] Connect Dashboard installed/favorite statistics and recent installs.
- [x] Run production build, strict TypeScript/Rust checks, backend tests, interaction tests, security review, and 1440/960 visual review.

## Phase 5 — Import and installation

- [x] Add ZIP/7z/RAR/folder input adapters with dependency/license review.
- [x] Implement secure staged extraction and archive bomb limits.
- [x] Implement root detection, immutable install plans, progress events, transaction journal, and rollback.
- [x] Add archive/folder pickers, Tauri drag-and-drop, confirmation, progress, blocker, success, and rollback-aware error UI.
- [x] Run production build, strict Clippy, 48 backend tests, archive-security review, and startup recovery review.

## Phase 6 — Enable and disable

- [x] Validate the supplied real EFMI loader policy, official EFMI Tools export guidance, and a checksum-verified public EFMI package without executing content.
- [x] Implement `efmi.copy.v1` with disabled staging, create-new copy, Hash verification, ownership manifests, atomic activation, transactional revoke, and startup recovery.
- [x] Implement safe single/256-item batch enable-disable, active-Profile persistence, list/detail controls, enabled filtering/counts, and F10 loader-refresh guidance.
- [x] Add modified/extra-file refusal, batch rollback, database-state, interrupted-revoke, and opt-in real-package compatibility tests.

## Phase 7 — Conflict detection

- [x] Implement exact target-path analyzer over active deployment manifests.
- [x] Implement bounded EFMI namespace/override/resource analyzer from official-template-shaped fixtures.
- [x] Add Dashboard/list/detail conflict UI and explicitly unverified EFMI winner semantics.
- [x] Run production build, strict Clippy, 60 backend tests, path/parser security review, and 1440/960 visual review.

## Phase 8 — Profiles

- [x] Implement validated create/delete/rename/copy and live active-Profile queries.
- [x] Implement rollback-capable full-set Profile reconciliation and crash-recovery ordering.
- [x] Add Profiles workspace, top-bar quick switch, Dashboard state, browser-preview interactions, and 1440/960 visual coverage.
- [x] Run production build, strict Clippy, 65 backend tests, database/recovery review, and interaction checks.

## Phase 9 — UX completion

- [x] Add persisted dark/system theme and supported-locale settings without exposing path mutation through the generic preferences command.
- [x] Add shell/onboarding localization, restartable onboarding, skip navigation, focus/reduced-motion support, and global activity feedback.
- [x] Add pointer/keyboard Profile order editing with transactional exact-membership validation.
- [x] Split feature routes and run production build, strict Rust checks, browser interactions, and 1440/960 visual review.

## Phase 10 — Audit and optimization

- [ ] Full security, performance, database-consistency, recovery, and API review.

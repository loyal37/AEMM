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

- [ ] Add ZIP/7z/RAR/folder input adapters with dependency/license review.
- [ ] Implement secure staged extraction and archive bomb limits.
- [ ] Implement root detection, immutable install plans, progress events, transaction journal, and rollback.

## Phase 6 — Enable and disable

- [ ] Validate deployment behavior with real EFMI mods.
- [ ] Implement at least one safe deployment strategy and manifest-backed revoke.
- [ ] Implement batch enable/disable and loader refresh guidance.

## Phase 7 — Conflict detection

- [ ] Implement target-path analyzer.
- [ ] Implement EFMI INI/resource analyzer from representative fixtures.
- [ ] Add conflict UI and verified load-order semantics.

## Phase 8 — Profiles

- [ ] Implement create/delete/rename/copy/switch.
- [ ] Implement rollback-capable profile reconciliation.

## Phase 9 — UX completion

- [ ] Accessibility, localization, theme, settings, onboarding, and progress UX.

## Phase 10 — Audit and optimization

- [ ] Full security, performance, database-consistency, recovery, and API review.

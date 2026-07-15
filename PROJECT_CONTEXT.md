# Project Context

## Project goal

Endfield Mod Manager (AEMM) is a maintainable Windows 10/11 desktop manager for *Arknights: Endfield* mods. It provides a safe local mod repository, pluggable deployment into a loader/game layout, profiles, metadata, conflict analysis, and eventually online discovery, updates, dependencies, version management, multi-game support, and cloud profile sync.

## Technology stack

- Desktop shell: Tauri 2
- Backend: Rust 2024, Tokio, SQLx/SQLite, Serde, thiserror, tracing
- Frontend: React 19, TypeScript, Vite, React Router, TanStack Query, Lucide icons
- Target: Windows 10 and Windows 11 (`x86_64-pc-windows-msvc` initially)
- Package manager: pnpm (the npm-compatible scripts remain `npm run ...` compatible after `npm install`)

## Current implementation status

- Phase 1 foundation is implemented and validated locally on Windows 11.
- The target GitHub repository `loyal37/AEMM` was empty at the start of Phase 1 and is ready for the initial publication.
- The local EFMI loader layout at `C:\Users\MR\Desktop\EFMI` has been inspected read-only.
- The Tauri development application starts successfully and creates a versioned `config.json`, migrated `mods.db`, repository/staging roots, and a rolling log file.

### Phase 1 delivered

- React router, responsive dark application shell, Dashboard, Mods, mod details, Profiles, and Settings page foundations.
- Typed frontend invoke client with a browser-preview fallback and desktop bootstrap health state.
- Modular Rust ports for game adapters, mod scanning/metadata/installation, deployment strategies, conflicts, and profiles.
- Thin Tauri commands backed by `AppServices`.
- Versioned, validated settings with atomic replacement, interrupted-write recovery, and storage-root separation checks.
- Structured console and daily rolling file logging.
- Async SQLite pool with WAL, foreign keys, embedded migration, normalized initial schema, and a default Profile.
- Path safety helpers and unit tests for parent traversal, reserved names, lexical containment, canonical containment, and root rejection.
- Reproducible pnpm and Cargo lock files.

## Important decisions

1. AEMM owns a canonical mod repository; enabled content is deployed to a game/loader target by a `ModDeploymentStrategy` implementation. Disabling reverses deployment and preserves the repository copy.
2. Tauri commands are thin adapters over `AppServices`; core logic is UI-independent.
3. SQLite stores relational/queryable state. `config.json` stores machine-specific paths and application preferences.
4. Author metadata and AEMM-local overrides are separate models and database tables. AEMM never rewrites an author's `mod.json`.
5. Installation is planned as a staged transaction: validate input, safely extract/copy to an owned staging root, inspect, plan, confirm, commit, deploy if requested, update the database, and roll back on failure.
6. Deployment and conflict detection are capability interfaces because EFMI/3DMigoto semantics differ from ordinary file-replacement mods.
7. Frontend/server contracts use explicit DTOs. Database rows and domain entities are not exposed directly to the UI.
8. Phase 1 uses a versioned SQL migration directory from the beginning.

## EFMI observations (read-only, 2026-07-15)

The supplied folder appears to be an Endfield Model Importer (EFMI) / 3DMigoto layout:

- `d3dx.ini` targets `Endfield.exe` and lists `3DMigotoLoader.exe`/`d3d11.dll` components.
- Loader startup may be mediated by XXMI Launcher; the local `launch` setting points to an Endfield executable but is machine-specific.
- `include_recursive = Mods` recursively discovers mod INI files.
- `exclude_recursive = DISABLED*` gives EFMI a native folder-prefix disable convention.
- F10 reloads fixes/configuration after mod changes.
- The supplied `Mods` directory contains no sample mods, so archive root heuristics and real mod layouts still need fixtures.
- The supplied binaries are unsigned and were not executed.

These observations justify an `EfmiGameAdapter` and an EFMI-specific deployment/conflict analyzer later. They must not leak into generic deployment interfaces.

## Known issues and open questions

- The exact national/international installation paths, executable names, registry/install manifests, and version sources are not confirmed.
- It is not yet confirmed whether AEMM should launch via XXMI Launcher, `3DMigotoLoader.exe`, or a configurable command for every supported installation.
- 3DMigoto conflict semantics can involve INI section names, hashes, resources, and command lists—not only identical relative file paths. The first conflict engine must therefore support analyzer plugins.
- EFMI recursive include/load ordering is not yet verified. AEMM must not claim deterministic load priority until this is tested against real mods and the loader.
- Symlink/junction support, required privileges, anti-cheat implications, and loader compatibility need safe empirical validation.
- No representative mod archives were present in the supplied EFMI folder.

## Next plan

1. Phase 2: implement configurable Endfield/EFMI game adapters, detection candidates, directory validation, version reading, open-directory, and safe launch commands.
2. Collect anonymized CN/global game layouts and representative EFMI mod fixtures before Phase 3/5.
3. Decide the repository license before accepting external source redistribution or contributions.

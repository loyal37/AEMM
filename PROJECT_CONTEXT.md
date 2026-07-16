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

- Phase 1 foundation and Phase 2 game path management are implemented and validated locally on Windows 11.
- The Phase 1 foundation was published to `loyal37/AEMM` on the `main` branch on 2026-07-16 (initial commit `3680f9f`).
- The local EFMI loader layout at `C:\Users\MR\Desktop\EFMI` has been inspected read-only.
- The Tauri development application starts successfully and creates a versioned `config.json`, migrated `mods.db`, repository/staging roots, and a rolling log file.
- The verified CN installation on this workstation is discovered from the Hypergryph Launcher uninstall registry entry and validates at `D:\Hypergryph Launcher\games\EndField Game`.

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

### Phase 2 delivered

- A UI-independent `EndfieldAdapter` that discovers launcher registry and known-root candidates on a blocking worker, canonicalizes paths, and requires `Endfield.exe`, exact `Endfield_Data/app.info` identity, `UnityPlayer.dll`, and `GameAssembly.dll`.
- Region inference that labels only verified Hypergryph Launcher/GRYPHLINK layouts as CN and leaves other valid manual layouts unknown; no international layout is claimed without a fixture.
- Explicit unavailable game-version state. The Windows file version (`2021.3.34f5` on the inspected install) is a Unity engine version and is not misreported as the game version.
- An `EfmiAdapter` that validates `3DMigotoLoader.exe`, `d3d11.dll`, `d3dx.ini`, and the contained `Mods` directory, then separately reports whether `Loader.launch` matches the configured game executable.
- `GameService` orchestration for detection, validation-before-persistence, status, launch-mode configuration, open-directory resolution, and safe process spawning.
- Thin Tauri game commands, native directory selection through the official dialog plugin, and backend-only validated path opening.
- Settings and Dashboard UI for automatic detection, manual selection, status/evidence, EFMI setup, launch mode, open directory, and one-click launch.
- Sixteen passing Rust tests, including false-positive identity checks, stale EFMI launch paths, and launch-spec containment tests.

## Important decisions

1. AEMM owns a canonical mod repository; enabled content is deployed to a game/loader target by a `ModDeploymentStrategy` implementation. Disabling reverses deployment and preserves the repository copy.
2. Tauri commands are thin adapters over `AppServices`; core logic is UI-independent.
3. SQLite stores relational/queryable state. `config.json` stores machine-specific paths and application preferences.
4. Author metadata and AEMM-local overrides are separate models and database tables. AEMM never rewrites an author's `mod.json`.
5. Installation is planned as a staged transaction: validate input, safely extract/copy to an owned staging root, inspect, plan, confirm, commit, deploy if requested, update the database, and roll back on failure.
6. Deployment and conflict detection are capability interfaces because EFMI/3DMigoto semantics differ from ordinary file-replacement mods.
7. Frontend/server contracts use explicit DTOs. Database rows and domain entities are not exposed directly to the UI.
8. Phase 1 uses a versioned SQL migration directory from the beginning.
9. Game discovery and identity validation are separate from loader validation. A valid EFMI directory can be saved while `launch_ready` remains false, so stale third-party configuration is visible without being executed.
10. Process launch never accepts a frontend executable or argument list. The backend rebuilds a launch specification from saved settings, revalidates it, and requires the executable to be a direct child of its canonical working directory.
11. Game versions are reported only from a future authoritative manifest/version source. Engine/file versions and launcher application versions are retained as evidence only, not presented as the game version.

## EFMI observations (read-only, 2026-07-15)

The supplied folder appears to be an Endfield Model Importer (EFMI) / 3DMigoto layout:

- `d3dx.ini` targets `Endfield.exe` and lists `3DMigotoLoader.exe`/`d3d11.dll` components.
- Loader startup may be mediated by XXMI Launcher; the local `launch` setting points to an Endfield executable but is machine-specific.
- `include_recursive = Mods` recursively discovers mod INI files.
- `exclude_recursive = DISABLED*` gives EFMI a native folder-prefix disable convention.
- F10 reloads fixes/configuration after mod changes.
- The supplied `Mods` directory contains no sample mods, so archive root heuristics and real mod layouts still need fixtures.
- The supplied binaries are unsigned and were not executed.
- The current game installation is under `D:\Hypergryph Launcher\games\EndField Game`; the older EFMI `launch` value under `C:\Program Files\GRYPHLINK` is stale.

These observations justify an `EfmiGameAdapter` and an EFMI-specific deployment/conflict analyzer later. They must not leak into generic deployment interfaces.

## Known issues and open questions

- The CN Hypergryph Launcher registry/install layout is verified on one machine. International launcher manifests, paths, executable identity markers, and region detection remain unverified.
- No authoritative game-version file or launcher manifest has been identified. The launcher log/version and executable product version are not treated as the game version.
- The inspected EFMI package is intended to integrate with XXMI Launcher, while its local `3DMigotoLoader.exe` and stale `Loader.launch` also expose a direct loader path. XXMI protocol support still needs safe verification.
- 3DMigoto conflict semantics can involve INI section names, hashes, resources, and command lists—not only identical relative file paths. The first conflict engine must therefore support analyzer plugins.
- EFMI recursive include/load ordering is not yet verified. AEMM must not claim deterministic load priority until this is tested against real mods and the loader.
- Symlink/junction support, required privileges, anti-cheat implications, and loader compatibility need safe empirical validation.
- No representative mod archives were present in the supplied EFMI folder.

## Next plan

1. Phase 3: implement asynchronous repository scanning, author/local metadata separation, and incremental SQLite persistence.
2. Collect anonymized international game layouts and representative EFMI mod fixtures before Phase 5/6.
3. Identify an authoritative CN/global game-version source without parsing stale logs.
4. Decide the repository license before accepting external source redistribution or contributions.

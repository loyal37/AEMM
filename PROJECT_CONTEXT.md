# Project Context

## Project goal

Endfield Mod Manager (AEMM) is a maintainable Windows 10/11 desktop manager for *Arknights: Endfield* mods. It provides a safe local mod repository, pluggable deployment into a loader/game layout, profiles, metadata, conflict analysis, and eventually online discovery, updates, dependencies, version management, multi-game support, and cloud profile sync.

## Technology stack

- Desktop shell: Tauri 2
- Backend: Rust 2024, Tokio, SQLx/SQLite, Serde, thiserror, tracing
- Frontend: React 19, TypeScript, Vite, React Router, TanStack Query/Virtual, Lucide icons
- Target: Windows 10 and Windows 11 (`x86_64-pc-windows-msvc` initially)
- Package manager: pnpm (the npm-compatible scripts remain `npm run ...` compatible after `npm install`)

## Current implementation status

- Phase 1 foundation, Phase 2 game path management, Phase 3 mod scanning/database persistence, and Phase 4 Mods UI are implemented and validated locally on Windows 11.
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

### Phase 3 delivered

- An owned repository boundary with a versioned `.aemm-repository.json` marker, canonical root validation, and rejection of unowned non-empty custom directories, links, junctions, and other reparse points.
- An asynchronous filesystem scanner that treats direct repository children as installed mods, skips unsafe/non-regular entries, normalizes Windows-relative paths, inventories file roles/sizes/timestamps, and computes streaming BLAKE3 hashes off the Tauri thread.
- Incremental scanning that reuses persisted hashes when file size and modification time are unchanged, while deriving a deterministic content fingerprint for database synchronization.
- Tolerant `mod.json` parsing with validation, unknown-field preservation, safe relative preview resolution, and stable inferred internal metadata when an author manifest is absent or invalid. Author documents are never rewritten.
- Transactional SQLite synchronization for mods, author metadata, local overrides, file inventories, hashes, timestamps, missing/broken state, and migration-backed local tags.
- Thin scan/list/local-metadata commands plus matching TypeScript DTOs and invoke functions for the Phase 4 UI.
- Tests covering repository ownership, tampered markers, traversal rejection, author-file preservation, duplicate logical IDs, incremental hash reuse, local-override preservation, missing mods, migrations, and a 1,000-mod performance fixture.

### Phase 4 delivered

- A production Mods workspace with card/list modes, deferred full-text search, category/lifecycle/favorite filters, name/install/update/size sorting, result selection, and transactional batch favorite operations.
- TanStack Virtual row virtualization for both responsive card grids and compact lists, so only visible/overscan entries create DOM nodes for 1,000+ mod repositories.
- A complete mod detail route with preview, effective metadata, original author metadata, local-only override/notes/tags editing, lifecycle warnings, installation/file statistics, and a virtualized file inventory.
- Safe backend detail, preview, batch-favorite, and open-directory use cases. Commands accept only mod UUIDs; repository paths are resolved from SQLite through typed owned-root containment checks.
- Preview files are capped at 2 MiB and accepted only after PNG/JPEG/WebP/GIF signature validation. SVG/HTML and arbitrary frontend file paths are never served by the desktop backend.
- Dashboard installed/favorite counts and recent installs now use live mod database queries. Enabled/conflict statistics remain explicitly unavailable until their owning phases.
- Browser-only deterministic preview fixtures for UI development; desktop mode always reads real SQLite records.
- Visual and interaction checks at 1440×1000 and the 960×800 minimum window, including grid/list/detail rendering, search, selection, local editing, and horizontal-overflow checks.

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
12. A custom mod repository is accepted only when it is empty or already carries a valid AEMM ownership marker. The application default may be adopted during upgrade because it is resolved from AEMM's own app-data root.
13. Phase 3 uses BLAKE3 for content identity and persists file modification timestamps as an incremental cache hint. Content fingerprints remain based on normalized path, size, and content hash so timestamp-only changes do not masquerade as mod updates.
14. Every direct child directory of the repository is one installed mod. Archive/package root discovery remains an installer concern and is deliberately deferred to Phase 5.
15. Phase 4 performs search/filter/sort in the webview over the compact `ModListItem` projection, while TanStack Virtual bounds rendered DOM. If online catalogs or repositories grow far beyond local 1,000-mod targets, query/pagination moves behind a backend port without changing card/detail components.
16. Author-provided website values are displayed as untrusted text, not launched. Opening a folder and loading a preview are UUID-based backend operations with fresh repository containment validation.

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
- Phase 3 does not infer EFMI deployment targets or loader priority from scanned files. File roles are descriptive only until real mod fixtures establish adapter-specific semantics.
- Manual edits that create duplicate case-insensitive author IDs cause the scan transaction to fail with an actionable metadata error, preserving the previously consistent database state.
- Phase 4 does not expose an enable switch or claim conflict results because deployment/profile state and verified conflict semantics belong to Phases 6 and 7.
- Preview images larger than 2 MiB or with unsupported signatures fall back to a generated placeholder. A managed thumbnail cache can be added later if real fixtures require large-source downscaling.

## Next plan

1. Phase 5: implement archive/folder import, secure staged extraction, root analysis, immutable confirmation plans, progress, commit, and rollback.
2. Collect anonymized international game layouts and representative EFMI mod fixtures before Phase 5/6.
3. Identify an authoritative CN/global game-version source without parsing stale logs.
4. Decide the repository license before accepting external source redistribution or contributions.

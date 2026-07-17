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

- Phase 1 foundation through Phase 8 Profile management are implemented and validated locally on Windows 11.
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

### Phase 5 delivered

- ZIP, 7z, RAR, and folder import adapters selected against the Rust 1.85 MSRV, with third-party and RARLab UnRAR licensing recorded in `THIRD_PARTY_NOTICES.md`.
- An AEMM-owned staging boundary with root and per-operation markers. Only marker-verified operation children can be cleaned; user-selected roots and unowned non-empty custom staging directories are never deleted or adopted.
- Signature-based archive detection, strict Windows-relative path validation, case-insensitive collision/ancestor checks, link/reparse rejection, encrypted/multipart rejection, entry/file/total-size limits, compression-ratio limits, and ZIP overlapping-data rejection.
- Manual `create_new` extraction for ZIP and folder sources, a custom 7z extraction callback that ignores library-computed destination paths, and bounded RAR read-to-memory followed by AEMM-controlled writes. No archive adapter uses a native destination-path extraction shortcut.
- Unique `mod.json` root detection, safe single-wrapper removal, explicit rejection of ambiguous multi-mod packages, stable content-derived IDs for manifest-less imports, and immutable confirmation plans persisted in operation journals.
- Duplicate ID/content checks, no-overwrite destination allocation, same-volume atomic rename, verified cross-volume copy through a repository partial, database synchronization, progress events, cancellation, rollback, and startup recovery.
- A desktop import dialog with archive/folder pickers, Tauri drag-and-drop, staged progress, plan/warning/blocker review, confirmation, rollback-aware error handling, and live Mods cache refresh.
- Forty-eight passing Rust tests, including real ZIP/7z/RAR decoding, Zip Slip and 7z traversal rejection, ZIP symlink rejection, quotas, duplicate blocking, rollback, and interrupted-install recovery.

### Phase 6 delivered

- A concrete `efmi.copy.v1` implementation behind `ModDeploymentStrategy`. It validates the current EFMI root and requires the observed `include_recursive = Mods` plus `exclude_recursive = DISABLED*` policy before mutating anything.
- Copy deployment through a unique `DISABLED_AEMM_PENDING_*` directory, `create_new` writes, live BLAKE3/size verification, an ownership marker, exact manifest inventory verification, and one atomic rename to `AEMM_<mod UUID>` only after the copy is complete.
- Manifest-backed disable through an atomic rename to `DISABLED_AEMM_REVOKE_*`. SQLite is updated while the deployment is safely excluded; cleanup occurs only after commit, and a database failure renames it back.
- Startup reconciliation for interrupted pending copies and revokes. Database-committed deployments are restored/verified, while database-orphaned AEMM-owned directories are cleaned only after marker and inventory validation.
- Strict revoke behavior: files changed after deployment, extra files/directories, links, junctions, marker mismatches, or a different EFMI root stop deletion and preserve the directory for inspection.
- Active-profile state in migration `0003_deployment_state.sql`, transactional deployment records/profile flags, safe single and 256-item batch enable/disable, enabled filters/switches, detail actions, F10 guidance, and live Dashboard counts.
- Fifty-five default Rust tests plus an opt-in compatibility harness. A checksum-verified public [EFMI RabbitFX](https://gamebanana.com/mods/651557) v2.2 package (`GameBanana file 1721477`, MD5 `a13b3c546a8ffbc94e20c9b6b8c5c6fd`) passed real ZIP staging, root detection, EFMI deployment, verification, revoke, and cleanup without executing any content; the downloaded copy was removed afterward.

### Phase 7 delivered

- A plugin-style `ModConflictDetector` with independent `deployment.path.v1` and `efmi.ini.v1` analyzers. The detector consumes an immutable active-Profile deployment snapshot rather than UI state or guessed repository paths.
- Exact target-path collision detection based on destination root, deployment directory, and case-insensitive Windows-relative target path. The current isolated EFMI copy strategy correctly produces no file-path collision unless two manifests truly address the same deployed file.
- Bounded, read-only EFMI INI analysis for explicit namespace collisions and overlapping `TextureOverride`/`ShaderOverride` Hashes, including source file, section, match constraints, handling mode, and directly referenced resource filenames as evidence.
- False-positive avoidance based on the official [EFMI Tools generated template](https://github.com/SpectrumQT/EFMI-Tools/blob/main/efmi-tools/templates/per_component.ini.j2): common per-file sections such as `Constants`, `Present`, and `ResourceModName` are not treated as global conflicts.
- Canonical deployed-root containment, component-by-component link/reparse rejection, 4 MiB per-file, 256 INIs per-mod, and 64 MiB per-report bounds. Parsing runs on a blocking worker and supports UTF-8/UTF-16 BOMs with explicit lossy-decoding warnings.
- A transactional `ConflictStore` projection that rejects enabled Profile rows without matching deployment manifests, plus a shared deployment/conflict lock so analysis cannot race enable, disable, recovery, or Profile reconciliation.
- Live Dashboard counts, a Mods conflict report, affected-only filtering, card/list warnings, and per-mod details showing participating mods, exact evidence, and current AEMM Profile positions.
- Actual EFMI winner selection remains intentionally unset. The UI labels Profile order separately and states that recursive loader/Hash winner semantics are not verified rather than presenting a fabricated priority.
- Sixty default Rust tests pass, including official-template-shaped fixtures, deployment-marker tampering, resource evidence, path collisions, false-positive prevention, and ordered database snapshots. Browser preview checks at 1440×1000 and 960×800 reported no page errors or horizontal overflow.

### Phase 8 delivered

- A transactional `ProfileStore` and locked `ProfileService` for list/create/rename/copy/delete operations, with normalized 64-character names, case-insensitive uniqueness, active-Profile deletion protection, and preserved ordered memberships.
- Rollback-capable Profile switching through the existing EFMI deployment transaction boundary. Source deployments are marker-verified and atomically quarantined, target mods are fingerprint-verified and deployed in stored order, and SQLite replaces deployment records plus `app_state.active_profile_id` in one commit.
- Failure rollback removes newly created target deployments before restoring source revoke tombstones. If the process stops mid-switch, startup recovery now orders pending/orphan-active cleanup ahead of revoke restoration so shared mod directory names converge safely.
- Empty Profiles can switch without a configured loader. Non-empty switching requires a freshly validated EFMI root and never trusts a path from the frontend.
- A complete Profiles workspace with active status, enabled counts, saved order previews, create/copy/rename/delete controls, switch progress/results/warnings, a live top-bar selector, Dashboard active-Profile state, and deterministic browser-preview behavior.
- Sixty-five default Rust tests pass, including CRUD protections, full two-way Profile reconciliation, rollback after an invalid target mod, and loader-free empty switching. Browser interaction/visual checks at 1440×1000 and 960×800 reported no page errors or horizontal overflow.

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
17. Phase 5 commits accept only an operation UUID. The frontend cannot submit a destination path or modify a prepared plan; the backend reloads the owned journal, rescans the candidate, and rechecks duplicates immediately before commit.
18. Archive entry names are never trusted as output paths. ZIP and folder content is written with `create_new`, 7z's helper destination argument is ignored, and RAR content is read under a per-file bound before writing to a validated AEMM path.
19. Manifest-less imports use `local.<content-fingerprint-prefix>` as their internal logical ID so staging/wrapper directory names cannot make identity unstable.
20. Prepared plans are abandoned on restart because no UI owns their confirmation anymore. Interrupted commits are preserved only when SQLite proves the same repository path and fingerprint was committed; otherwise recovery verifies the fingerprint and rolls back.
21. Phase 6's first concrete strategy copies one repository mod into a stable, isolated `AEMM_<UUID>` child under the verified EFMI `Mods` root. Shared deployment interfaces remain strategy-neutral; no EFMI path convention leaks into the repository or scanner.
22. EFMI's verified `DISABLED*` exclusion is the transaction boundary: work-in-progress copies and revoke tombstones are never visible as active mods. The database is never allowed to authorize deletion by itself; the on-disk ownership marker and exact file inventory must also agree.
23. Deployment lists are immutable Hash/size manifests. AEMM refuses automatic revoke when deployed content has been changed or augmented, and removal enumerates only manifest paths so a raced-in extra path is never recursively deleted.
24. The singleton `app_state.active_profile_id` is the authoritative activity pointer; all deployment, conflict, mod-list, Dashboard, and Profile queries derive current state from it rather than assuming the seeded default Profile.
25. Conflict analysis reads the immutable manifests and currently deployed INI files for enabled mods. It does not infer runtime targets from repository filenames, and it shares the deployment mutation lock to avoid analyzing a half-completed filesystem transition.
26. Repeated ordinary section names are valid in the EFMI Tools output because each included INI has its own namespace. Only explicit namespace collisions, overlapping override Hashes, and actual destination paths are reported by Phase 7.
27. `profile_mods.load_order` is exposed as the current AEMM arrangement, not as a verified EFMI winner rule. Every Phase 7 `winning_mod_id` remains `None` until upstream source behavior and representative runtime fixtures prove deterministic precedence.
28. A non-active Profile stores desired membership only and must not own `deployment_records`. During a switch, source records remain authoritative until all target filesystem work succeeds; one transaction then removes source manifests, inserts target manifests, and advances `app_state`.
29. The current EFMI strategy uses the same isolated `AEMM_<mod UUID>` directory across Profiles. Phase 8 therefore quarantines the complete source set and redeploys the complete target set, including shared mods, instead of mutating ownership markers in place.
30. Profile CRUD and deployment/conflict mutations share one async operation lock. Database transactions still re-check active IDs, names, memberships, and manifests so the lock is coordination—not the sole consistency boundary.

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
- Phase 7 covers actual deployed paths, explicit namespaces, override Hashes, match constraints, handling, and direct resource-file references. Conditional command-list interactions, fuzzy resource matching, cross-file dependency semantics, and future EFMI syntax still need additional analyzer versions and fixtures.
- EFMI recursive include/load ordering is not yet verified. AEMM must not claim deterministic load priority until this is tested against real mods and the loader.
- Symlink/junction support, required privileges, anti-cheat implications, and loader compatibility need safe empirical validation.
- No representative mod archives were present in the supplied EFMI folder. A public RabbitFX v2.2 package has since validated the real archive/root/deploy/revoke path, but a representative EFMI Tools-generated character model package with Meshes/Textures is still needed before finalizing resource-level conflict semantics.
- Phase 3 does not infer EFMI deployment targets or loader priority from scanned files. File roles are descriptive only until real mod fixtures establish adapter-specific semantics.
- Manual edits that create duplicate case-insensitive author IDs cause the scan transaction to fail with an actionable metadata error, preserving the previously consistent database state.
- Preview images larger than 2 MiB or with unsupported signatures fall back to a generated placeholder. A managed thumbnail cache can be added later if real fixtures require large-source downscaling.
- Profile switching currently redeploys the full target set because EFMI deployment directories are keyed by mod UUID and ownership markers include the Profile ID. Safe shared-deployment transfer can be optimized later only with an explicit marker/database protocol and crash fixtures.

## Next plan

1. Phase 9: complete accessibility, localization/theme behavior, onboarding, load-order editing, settings validation feedback, and long-operation UX.
2. Phase 10: run the full security, performance, database-consistency, recovery, and public-API audit.
3. Collect anonymized international game layouts and a representative EFMI Tools-generated character model fixture before finalizing resource-level conflict priority.
4. Identify an authoritative CN/global game-version source without parsing stale logs.
5. Decide the repository license before accepting external source redistribution or contributions.

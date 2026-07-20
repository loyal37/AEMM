# AEMM Agent Guide

This repository contains **Endfield Mod Manager (AEMM)**, a Windows desktop application for managing *Arknights: Endfield* mods.

## Required context recovery

Before starting any new task:

1. Read this file.
2. Read `PROJECT_CONTEXT.md`.
3. Read `ARCHITECTURE.md`.
4. Read `TASKS.md`.
5. Inspect the existing code and current Git diff.
6. Only then make scoped changes.

After completing an important feature, update `PROJECT_CONTEXT.md`, `ARCHITECTURE.md` when architecture changed, `TASKS.md`, and `CHANGELOG.md`.

## Engineering rules

- Keep Tauri commands as transport adapters. Business logic belongs in core modules and services that can be tested without a webview.
- Preserve the boundaries between EFMI validation, direct mod storage/state, installation, metadata, conflicts, profiles, and persistence.
- AEMM directly manages a validated EFMI `Mods` directory and is not a game manager or launcher. Do not reintroduce game discovery, game-path, or process-launch features without an explicit product decision.
- Do not hard-code an unverified Endfield or loader layout into shared core logic.
- Prefer mature, actively maintained crates and packages over custom infrastructure.
- Production Rust code must propagate errors; avoid `unwrap()` and `expect()` outside narrowly justified tests or compile-time invariants.
- File operations must canonicalize and validate their roots. Archive extraction must reject absolute paths, parent traversal, link escapes, and writes outside the staging directory.
- Destructive operations must prove that the target is contained by an AEMM-owned root. Never delete a user-selected root itself.
- Long-running filesystem, hashing, archive, and database work must not block the Tauri/UI thread.
- Keep public APIs stable. Refactors must remain behavior-compatible and stay within task scope.
- Never execute third-party game loaders or mod binaries during analysis or tests.

## Required validation

Run the applicable checks before declaring a phase complete:

```text
pnpm build
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

For local development, `pnpm tauri dev` is the supported entry point. The desktop target requires the Windows MSVC build tools and WebView2 runtime.

For a standalone raw executable, use `pnpm tauri:release`. Do not use plain `cargo build --release`: the Tauri CLI must set the production build context so the web assets are embedded instead of pointing the webview at the Vite development URL.

## Git scope

- Keep each commit focused on one coherent feature.
- Do not stage unrelated user changes.
- Use Conventional Commit messages such as `feat: add mod scanner` or `fix: reject unsafe archive paths`.

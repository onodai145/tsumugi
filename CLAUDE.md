# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

tsumugi: a Misskey multi-column desktop client (Krile-like UX) built on Tauri v2. Rust core (`src-tauri/`) owns all Model-layer logic; the Svelte frontend (`frontend/`) is View/ViewModel only. Design docs live in `docs/design/` — `docs/design/misskey-multicolumn-client-design.md` is the authoritative design doc; if any other doc conflicts with it, the design doc wins. User-facing documentation lives in `docs/guide/user-guide.md`.

Frontend visual conventions (border-radius/font-size/icon-size/focus-visible scales) are codified in `docs/design/style-guide.md` — check it before adding new UI, don't invent one-off values (e.g. `rounded-[Npx]`).

## Commands

```sh
cargo tauri dev              # dev with hot reload — starts vite (127.0.0.1:5173) + the app together
cargo tauri build             # release build with frontend embedded (frontendDist), no dev server needed

cd src-tauri && cargo test    # Rust tests (real Misskey connectivity tests are #[ignore])
cd src-tauri && cargo test <test_name>   # run a single test
cd frontend  && pnpm check               # svelte-check + tsc (tsconfig.node.json)
cd frontend  && pnpm test                # Vitest unit tests

scripts/release.sh X.Y.Z      # bump version in package.json/Cargo.toml/tauri.conf.json/Cargo.lock, generate CHANGELOG.md, create release/vX.Y.Z branch + commit — never hand-edit these version fields with sed
```

Operation E2E tests (Playwright/tauri-driver against a throwaway Docker Compose Misskey instance) live in `e2e/` — see `e2e/README.md` for setup and run order, not covered here.

Never run `./target/debug/tsumugi` or `cargo run` directly — Tauri's debug build loads the frontend from the dev server (`devUrl` = `127.0.0.1:5173`); without vite running you get a connection-refused error. Always use `cargo tauri dev`.

`cargo tauri build --debug`'s bundler step can hang 40+ minutes on some dependency-tree changes (unrelated to compilation itself, which finishes in seconds). If you just need a runnable `target/debug/tsumugi` binary (e.g. for `tauri-driver`/E2E), use `cargo build` from `src-tauri/` directly, or `cargo tauri build --debug --no-bundle`.

E2E tests (`e2e/`) launch several background processes (`Xvfb`, `dbus-run-session`, `gnome-keyring-daemon`, the `tsumugi` binary under test) that can be orphaned if a run is killed abruptly. Clean these up by exact PID only (`ps aux` then `kill <pid>`) — never `pkill`/`killall` by name/pattern, since that can match unrelated real processes (e.g. a real browser) on the same machine.

On Linux/Wayland (Hyprland etc.), WebKitGTK's DMABUF renderer can conflict with wlroots compositors and crash rendering with `Gdk Error 71 (protocol error)`. `src-tauri/src/main.rs` sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` by default to work around this; if that doesn't help, fall back to `GDK_BACKEND=x11 cargo tauri dev`.

### Android
Android build support exists (`src-tauri/tauri.android.conf.json`, `src-tauri/gen/android`). CI (`android-build` job in `.github/workflows/test.yml`) only verifies it compiles and links — no signing or artifact distribution:
```sh
cd src-tauri && cargo tauri android build --debug --target aarch64
```
Requires NDK r27c. The symlinks inside the NDK toolchain that `setup-ndk` extracts are relative, so they break if the extraction path changes — CI re-points them to absolute paths as a workaround (see the comment above the `android-build` job in `test.yml`).

If the build fails during Gradle configuration with `A problem occurred configuring project ':buildSrc'` and a cause like `IllegalArgumentException: 26.0.2` (or any JDK version string), it's not an NDK issue — Gradle 8.14.3's bundled Kotlin DSL compiler can't parse newer JDK version strings when evaluating `buildSrc`'s `build.gradle.kts`. Build with an older JDK, e.g. `JAVA_HOME=/usr/lib/jvm/java-21-openjdk cargo tauri android build --debug --target aarch64`.

## Architecture

### Rust ↔ TS boundary
`src-tauri/src/lib.rs` is the single source of truth for the command/event surface: `specta_builder()` registers every `#[tauri::command]` and every event type via `tauri-specta`. In debug builds, `run()` re-exports TS bindings to `frontend/src/bindings/tauri.gen.ts` on every launch; `cargo test`'s `generates_frontend_bindings` test also regenerates it and asserts serde's `camelCase` rename made it through and that account tokens are never exposed in generated types. When adding a command or event, register it in `specta_builder()`, not just the `tauri::Builder`.

### src-tauri/src layout
- `domain/` — normalized domain types shared across the app (Note, User, Account, Column, Reaction, Mute, ...), `specta::Type`-annotated for TS export.
- `api/` — REST client. **Hand-written, not generated** (see below) — thin typed wrappers per resource (`notes.rs`, `meta.rs`, `drive.rs`, ...) plus `normalize.rs` to convert raw responses into `domain` types. All Misskey REST calls are POST with the token embedded in the JSON body; that's centralized in `client.rs`.
- `stream/` — Streaming (WebSocket), entirely hand-implemented since it's outside Misskey's OpenAPI spec: `connection.rs` (one WS per account, ping/pong + backoff reconnect), `protocol.rs` (message types), `inbox.rs` (dedupe received notes by note ID, since one shared WS can deliver the same note via multiple channel subscriptions).
- `filter/` — TQL (Tsumugi Query Language), the per-column filter DSL modeled on Krile's KQL. Designed as a two-stage pipeline: `token` → `parser` (parse + type-check) → `ast` → `eval` (in-memory, for live Streaming notes) / `sql` (SQL projection, intended for cached/backfill queries) — see `docs/design/filter-dsl-design.md` for the grammar. As of this writing the `sql` stage is not fully wired up yet (`filter/mod.rs` carries `#![allow(dead_code)]` for it) — don't assume cache/backfill filtering goes through SQL projection without checking current wiring. `CompiledFilter` (in `filter/mod.rs`) is compiled once per column at creation time to avoid re-parsing per note.
- `store/` — persistence. Settings and drafts are plain JSON files; only the note cache is SQLite via `rusqlite`.
- `session/` — account/token management; tokens go through the OS keyring (`keyring` crate), never through the frontend.
- `commands/` — the `#[tauri::command]` handlers, grouped by resource (`account`, `column`, `note`, `mute`, `draft`).
- `state.rs` — `AppState`, the single Tauri-managed state struct threading together accounts, secrets, connections, mute config, and settings; commands pull what they need from `State<AppState>`.
- `debug_bridge.rs` — debug-build-only (`#[cfg(all(debug_assertions, desktop))]`), not registered in `specta_builder()`. Opens a local-only listener — a Unix domain socket at `app_cache_dir()/debug-bridge.sock` on Linux/macOS, a named pipe at `\\.\pipe\tsumugi-debug-bridge-<USERNAME>` on Windows (cross-compile-checked only, not verified on real Windows) — so an agent without a display (e.g. Claude running headless) can execute JS against the user's already-running instance and read back console/DOM state. Neither uses a `127.0.0.1` TCP port, specifically to avoid the localhost-drive-by/DNS-rebinding attack surface a browser tab could otherwise reach. Talk to it with `curl --unix-socket <path> http://localhost/ --data-binary '<js>'` (Unix) or an equivalent named-pipe client (Windows); the body runs as a function body, so (like WebDriver's `execute_script`) a bare expression returns nothing — you must write `return ...`. See `docs/superpowers/specs/2026-08-22-claude-devtools-bridge-design.md` for the design rationale (Issue #232).

### frontend/src layout
- `ui/` — Svelte components (columns, compose bar, settings, account management).
- `render/` — note content rendering (MFM, emoji, media grid).
- `input/` — input widgets (e.g. reaction picker).
- `lib/` — cross-cutting utilities (IPC wrapper, theme, keymap, time formatting, MFM/nyaize helpers).
- `bindings/tauri.gen.ts` — **generated**, do not hand-edit; regenerate via `cargo test` or `cargo tauri dev`.

### progenitor is not used for REST codegen
Misskey's OpenAPI spec (`/api-doc.json`, snapshotted at `src-tauri/openapi/misskey-api-doc.json`) is **3.1.0**. `progenitor` depends on the `openapiv3` crate, which only supports OpenAPI 3.0.x, so it fails to parse Misskey's spec (nullable fields expressed as `type: ["string", "null"]`). This was tried and rejected during Phase 1 — see `docs/design/misskey-multicolumn-client-design.md` §6.1. The REST client is fully hand-written instead (`src-tauri/src/api/`); don't reintroduce a progenitor build step without re-validating against the current spec.

### specta/tauri-specta versions are pinned
`specta`, `specta-typescript`, and `tauri-specta` are pinned to exact versions (`=2.0.0-rc.25` / `=0.0.12`) in `Cargo.toml`. Don't loosen these without checking that TS binding generation still works — see `docs/design/phase0-scaffold.md` for context.

## Development workflow

- Never commit directly to `main`. Create a feature branch before touching any file (branch first, edit second — never the other way around).
- Fixes tied to a GitHub issue go through a PR, not a direct merge. Put `Fixes #N` or `Closes #N` in the PR body — referencing the issue number alone does not auto-close it on merge.
- After pushing, don't poll CI with Monitor/wait loops; the user checks CI results themselves.
- Commit messages: subject line only, no body/bullet points (the Co-Authored-By trailer is appended separately).
- `Agent` calls with `isolation: "worktree"` branch off `main`, not off the current feature branch — don't use that isolation mode for subtasks that depend on in-progress feature-branch changes.

## PR merging

`main` is a protected branch (no direct push, no force push — even for cleanup). When merging a PR, default to a normal merge commit (`gh pr merge --merge`), not squash, unless explicitly told otherwise. Squashing and then wanting a plain merge commit instead requires rewriting already-pushed history, which branch protection blocks outright — there's no clean way to fix it after the fact.

## Release process

1. `git checkout main && git pull origin main` — release branches off up-to-date main.
2. Decide the next version (semver: bump minor for feature additions since the last tag, patch for fix-only ranges). Check what's shipped since the last tag with `git log <last-tag>..main --oneline`.
3. `scripts/release.sh X.Y.Z` — creates `release/vX.Y.Z`, bumps the version in `frontend/package.json` / `src-tauri/Cargo.toml` / `src-tauri/tauri.conf.json` / `src-tauri/Cargo.lock`, generates `CHANGELOG.md` via `git-cliff`, and commits as `chore(release): vX.Y.Z`.
4. `git push -u origin release/vX.Y.Z` then `gh pr create` — the release branch goes through a PR like any other change, not a direct local merge to main.
5. Once CI passes, `gh pr merge <N> --merge` (normal merge commit, not squash — see PR merging above).
6. `git checkout main && git pull origin main`, then tag the merge commit and push it: `git tag -a vX.Y.Z -m "vX.Y.Z" && git push origin vX.Y.Z`.
7. Pushing the `vX.Y.Z` tag triggers `.github/workflows/release.yml`, which builds installers for Linux/macOS/Windows plus signed Android APKs and publishes them as a **draft** GitHub Release (`releaseDraft: true`). Actually publishing the release (editing notes, unmarking as draft) is a manual step on GitHub — not automated.

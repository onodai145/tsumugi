# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

tsumugi: a Misskey multi-column desktop client (Krile-like UX) built on Tauri v2. Rust core (`src-tauri/`) owns all Model-layer logic; the Svelte frontend (`frontend/`) is View/ViewModel only. Design docs live in `docs/` — `docs/misskey-multicolumn-client-design.md` is the authoritative design doc; if any other doc conflicts with it, the design doc wins.

## Commands

```sh
cargo tauri dev              # dev with hot reload — starts vite (127.0.0.1:5173) + the app together
cargo tauri build             # release build with frontend embedded (frontendDist), no dev server needed

cd src-tauri && cargo test    # Rust tests (real Misskey connectivity tests are #[ignore])
cd src-tauri && cargo test <test_name>   # run a single test
cd frontend  && pnpm check               # svelte-check + tsc (tsconfig.node.json)
```

Never run `./target/debug/tsumugi` or `cargo run` directly — Tauri's debug build loads the frontend from the dev server (`devUrl` = `127.0.0.1:5173`); without vite running you get a connection-refused error. Always use `cargo tauri dev`.

On Linux/Wayland (Hyprland etc.), WebKitGTK's DMABUF renderer can conflict with wlroots compositors and crash rendering with `Gdk Error 71 (protocol error)`. `src-tauri/src/main.rs` sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` by default to work around this; if that doesn't help, fall back to `GDK_BACKEND=x11 cargo tauri dev`.

## Architecture

### Rust ↔ TS boundary
`src-tauri/src/lib.rs` is the single source of truth for the command/event surface: `specta_builder()` registers every `#[tauri::command]` and every event type via `tauri-specta`. In debug builds, `run()` re-exports TS bindings to `frontend/src/bindings/tauri.gen.ts` on every launch; `cargo test`'s `generates_frontend_bindings` test also regenerates it and asserts serde's `camelCase` rename made it through and that account tokens are never exposed in generated types. When adding a command or event, register it in `specta_builder()`, not just the `tauri::Builder`.

### src-tauri/src layout
- `domain/` — normalized domain types shared across the app (Note, User, Account, Column, Reaction, Mute, ...), `specta::Type`-annotated for TS export.
- `api/` — REST client. **Hand-written, not generated** (see below) — thin typed wrappers per resource (`notes.rs`, `meta.rs`, `drive.rs`, ...) plus `normalize.rs` to convert raw responses into `domain` types. All Misskey REST calls are POST with the token embedded in the JSON body; that's centralized in `client.rs`.
- `stream/` — Streaming (WebSocket), entirely hand-implemented since it's outside Misskey's OpenAPI spec: `connection.rs` (one WS per account, ping/pong + backoff reconnect), `protocol.rs` (message types), `inbox.rs` (dedupe received notes by note ID, since one shared WS can deliver the same note via multiple channel subscriptions).
- `filter/` — TQL (Tsumugi Query Language), the per-column filter DSL modeled on Krile's KQL. Designed as a two-stage pipeline: `token` → `parser` (parse + type-check) → `ast` → `eval` (in-memory, for live Streaming notes) / `sql` (SQL projection, intended for cached/backfill queries) — see `docs/filter-dsl-design.md` for the grammar. As of this writing the `sql` stage is not fully wired up yet (`filter/mod.rs` carries `#![allow(dead_code)]` for it) — don't assume cache/backfill filtering goes through SQL projection without checking current wiring. `CompiledFilter` (in `filter/mod.rs`) is compiled once per column at creation time to avoid re-parsing per note.
- `store/` — SQLite persistence (settings, note cache) via `rusqlite`.
- `session/` — account/token management; tokens go through the OS keyring (`keyring` crate), never through the frontend.
- `commands/` — the `#[tauri::command]` handlers, grouped by resource (`account`, `column`, `note`, `mute`).
- `state.rs` — `AppState`, the single Tauri-managed state struct threading together accounts, secrets, connections, mute config, and settings; commands pull what they need from `State<AppState>`.

### frontend/src layout
- `ui/` — Svelte components (columns, compose bar, settings, account management).
- `render/` — note content rendering (MFM, emoji, media grid).
- `input/` — input widgets (e.g. reaction picker).
- `lib/` — cross-cutting utilities (IPC wrapper, theme, keymap, time formatting, MFM/nyaize helpers).
- `bindings/tauri.gen.ts` — **generated**, do not hand-edit; regenerate via `cargo test` or `cargo tauri dev`.

### progenitor is not used for REST codegen
Misskey's OpenAPI spec (`/api-doc.json`, snapshotted at `src-tauri/openapi/misskey-api-doc.json`) is **3.1.0**. `progenitor` depends on the `openapiv3` crate, which only supports OpenAPI 3.0.x, so it fails to parse Misskey's spec (nullable fields expressed as `type: ["string", "null"]`). This was tried and rejected during Phase 1 — see `docs/misskey-multicolumn-client-design.md` §6.1. The REST client is fully hand-written instead (`src-tauri/src/api/`); don't reintroduce a progenitor build step without re-validating against the current spec.

### specta/tauri-specta versions are pinned
`specta`, `specta-typescript`, and `tauri-specta` are pinned to exact versions (`=2.0.0-rc.25` / `=0.0.12`) in `Cargo.toml`. Don't loosen these without checking that TS binding generation still works — see `docs/phase0-scaffold.md` for context.

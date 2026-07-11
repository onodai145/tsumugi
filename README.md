# tsumugi

Misskey マルチカラムデスクトップクライアント（Krile 風 UX）。Tauri v2 + Rust コア + Svelte フロント。

設計は [`docs/`](docs/) を参照（設計書 / TQL フィルタDSL / ロードマップ）。

## 必要環境

- Rust（stable/nightly）, `cargo`
- Node.js + `pnpm`
- `cargo-tauri` CLI（`cargo install tauri-cli` もしくは同梱）
- Linux は `webkit2gtk-4.1` / `libsoup-3.0` / `gtk+-3.0` 等の系ライブラリ

## 起動方法

### 開発（ホットリロード）

```sh
cargo tauri dev
```

これ一発で vite dev サーバ（`127.0.0.1:5173`）とアプリの両方が起動する。

> **Linux/Wayland（Hyprland 等）:** WebKitGTK の DMABUF レンダラが wlroots 系
> コンポジタと衝突し `Gdk Error 71 (protocol error)` で描画が落ちることがある。
> 本アプリは Linux では `WEBKIT_DISABLE_DMABUF_RENDERER=1` を既定でセットして回避する
> （`src-tauri/src/main.rs`）。なお効かない場合は X11 フォールバックを試す:
> `GDK_BACKEND=x11 cargo tauri dev`

> **注意:** `./target/debug/tsumugi` や `cargo run` を**単体で直接起動しないこと**。
> Tauri の debug ビルドはフロントを dev サーバ（`devUrl` = `127.0.0.1:5173`）から読み込むため、
> vite が動いていないと `Could not connect to 127.0.0.1:5173: Connection refused` になる。
> 開発時は必ず `cargo tauri dev` を使う。

### スタンドアロン（vite 不要の単体アプリ）

```sh
cargo tauri build
```

release ビルドはフロントを埋め込む（`frontendDist`）ため、生成物は dev サーバ無しで単体起動できる。

## 構成

- `src-tauri/` … Rust コア（api / stream / store / filter / session / commands / domain）
- `frontend/` … Svelte + Vite（ui / render / input）
- Rust→TS 型・コマンド・イベントは `tauri-specta` で `frontend/src/bindings/tauri.gen.ts` に自動生成
  （`cargo test` の `generates_frontend_bindings` でも再生成される）

## テスト

```sh
cd src-tauri && cargo test           # Rust（実 Misskey 疎通テストは #[ignore]）
cd frontend  && pnpm exec svelte-check
```

## ライセンス

[MIT License](LICENSE)

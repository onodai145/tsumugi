# tsumugi

[![test](https://github.com/onodai145/tsumugi/actions/workflows/test.yml/badge.svg)](https://github.com/onodai145/tsumugi/actions/workflows/test.yml)
[![Release](https://img.shields.io/github/v/release/onodai145/tsumugi)](https://github.com/onodai145/tsumugi/releases/latest)
[![License: MIT](https://img.shields.io/github/license/onodai145/tsumugi)](LICENSE)
[![Vibe Coding](https://img.shields.io/badge/100%25-vibe%20coded-ff69b4)](https://github.com/onodai145/tsumugi)

Misskey マルチカラムデスクトップクライアント（Krile 風 UX）。Tauri v2 + Rust コア + Svelte フロント。

設計は [`docs/design/`](docs/design/) を参照（設計書 / TQL フィルタDSL / ロードマップ）。
使い方は [`docs/guide/user-guide.md`](docs/guide/user-guide.md) を参照。

## 特徴

- 複数のタイムライン・リスト・アンテナ・通知などを横に並べて同時に閲覧できるマルチカラムUI
- 1カラム内に複数タブを持たせ、タブ単位で TQL（Tsumugi Query Language）による柔軟なフィルタを設定可能
- 複数のMisskeyアカウントを同時にログインし、投稿やカラムごとに使い分け可能
- テーマ・フォント・背景画像などの詳細なカスタマイズに対応

## ダウンロード

<!-- release-download-links:start -->
最新版 v0.8.0（[Releases ページ](https://github.com/onodai145/tsumugi/releases/latest)）

| Windows | macOS | Linux | Android |
|---|---|---|---|
| [msi（標準）](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi_0.8.0_x64_en-US.msi) | [dmg（標準）](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi_0.8.0_universal.dmg) | [AppImage（インストール不要）](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi_0.8.0_amd64.AppImage) | [universal](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-v0.8.0-android-universal.apk)（不明な場合はこれ） |
| [exe（NSIS）](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi_0.8.0_x64-setup.exe) | [tar.gz](https://github.com/onodai145/tsumugi/releases/latest/download/tsumugi_universal.app.tar.gz) | [deb（Debian/Ubuntu）](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi_0.8.0_amd64.deb) | [arm64-v8a](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-v0.8.0-android-arm64-v8a.apk) |
| [ポータブル（インストール不要）](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-v0.8.0-portable-windows-x64.exe) | | [rpm（Fedora/RHEL）](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-0.8.0-1.x86_64.rpm) | [armeabi-v7a](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-v0.8.0-android-armeabi-v7a.apk) |
| | | | [x86_64](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-v0.8.0-android-x86_64.apk) |
| | | | [x86](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-v0.8.0-android-x86.apk) |
<!-- release-download-links:end -->

## 開発者向け情報

以下はソースからビルド・開発する人向けの情報。単に使いたいだけなら [ダウンロード](#ダウンロード) を参照。

### 必要環境

- Rust（stable/nightly）, `cargo`
- Node.js + `pnpm`
- `cargo-tauri` CLI（`cargo install tauri-cli` もしくは同梱）
- Linux は `webkit2gtk-4.1` / `libsoup-3.0` / `gtk+-3.0` 等の系ライブラリ

### 起動方法

#### 開発（ホットリロード）

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

#### スタンドアロン（vite 不要の単体アプリ）

```sh
cargo tauri build
```

release ビルドはフロントを埋め込む（`frontendDist`）ため、生成物は dev サーバ無しで単体起動できる。

### 構成

- `src-tauri/` … Rust コア（api / stream / store / filter / session / commands / domain）
- `frontend/` … Svelte + Vite（ui / render / input）
- Rust→TS 型・コマンド・イベントは `tauri-specta` で `frontend/src/bindings/tauri.gen.ts` に自動生成
  （`cargo test` の `generates_frontend_bindings` でも再生成される）

### テスト

```sh
cd src-tauri && cargo test           # Rust（実 Misskey 疎通テストは #[ignore]）
cd frontend  && pnpm exec svelte-check
```

### バージョニング / リリース

[`docs/design/release-process.md`](docs/design/release-process.md) を参照。

## ライセンス

[MIT License](LICENSE)

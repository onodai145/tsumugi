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

| OS | 形式 | リンク |
|---|---|---|
| Windows | インストーラ (msi) | [tsumugi_0.8.0_x64_en-US.msi](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi_0.8.0_x64_en-US.msi) |
| Windows | インストーラ (exe, NSIS) | [tsumugi_0.8.0_x64-setup.exe](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi_0.8.0_x64-setup.exe) |
| Windows | ポータブル (exe, インストール不要) | [tsumugi-v0.8.0-portable-windows-x64.exe](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-v0.8.0-portable-windows-x64.exe) |
| macOS | ディスクイメージ (dmg) | [tsumugi_0.8.0_universal.dmg](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi_0.8.0_universal.dmg) |
| macOS | アーカイブ (tar.gz) | [tsumugi_universal.app.tar.gz](https://github.com/onodai145/tsumugi/releases/latest/download/tsumugi_universal.app.tar.gz) |
| Linux | AppImage（インストール不要） | [tsumugi_0.8.0_amd64.AppImage](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi_0.8.0_amd64.AppImage) |
| Linux | Debian/Ubuntu (deb) | [tsumugi_0.8.0_amd64.deb](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi_0.8.0_amd64.deb) |
| Linux | Fedora/RHEL (rpm) | [tsumugi-0.8.0-1.x86_64.rpm](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-0.8.0-1.x86_64.rpm) |
| Android | APK (universal) | [tsumugi-v0.8.0-android-universal.apk](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-v0.8.0-android-universal.apk) |
| Android | APK (arm64-v8a) | [tsumugi-v0.8.0-android-arm64-v8a.apk](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-v0.8.0-android-arm64-v8a.apk) |
| Android | APK (armeabi-v7a) | [tsumugi-v0.8.0-android-armeabi-v7a.apk](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-v0.8.0-android-armeabi-v7a.apk) |
| Android | APK (x86_64) | [tsumugi-v0.8.0-android-x86_64.apk](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-v0.8.0-android-x86_64.apk) |
| Android | APK (x86) | [tsumugi-v0.8.0-android-x86.apk](https://github.com/onodai145/tsumugi/releases/download/v0.8.0/tsumugi-v0.8.0-android-x86.apk) |

Android版はお使いの端末のCPUアーキテクチャに合わせて選択してください（不明な場合は universal 版）。
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

# Issue #612 (1/2): WebView の CSP を設定する

## 目的

将来どこかに注入点(`{@html}` や URL / HTML の注入)ができても、スクリプトが実行されないようにする多層防御。現状、注入点は見つかっていない。既存の機能は壊さない。

Issue #612 は 2 つの PR に分ける。本書は 1 本目の CSP だけを扱う。2 本目(WebView から渡される任意のパスの制限)は、別の spec を書く。

## 現状

- `tauri.conf.json` の `app.security.csp` が `null` で、CSP が無い。
- WebView は Misskey と直接通信しない(REST と Streaming は Rust 側)。WebView が読み込む外部のものは、画像・動画・音声・URL プレビューの埋め込みプレイヤー・カスタム CSS が指定したもの。
- Vidstack 1.15.7 は、HLS / DASH のソースを渡されると、`hls.js@^1.5.0` / `dashjs@4.7.4` を jsdelivr から `<script>` で読み込む(ソースで確認。実行はしていない)。

## 方針

スクリプトは同梱のものだけに絞る。画像・動画などは今の表示を保つため、広めに許可する。

## 設定値

`tauri.conf.json` の `app.security` に、次を指定する。`tauri.android.conf.json` では上書きしない。

| ディレクティブ | 値 | 根拠 |
|---|---|---|
| `default-src` | `'self'` | 下に無い種類は同梱のものだけ |
| `script-src` | `'self'` | `eval`・インラインスクリプト・外部スクリプトを禁止する。Shiki は JavaScript エンジンを指定しているので、`'wasm-unsafe-eval'` は要らない |
| `style-src` | `'self' 'unsafe-inline' https:` | カスタム CSS の `<style>`、CodeMirror、`style` 属性。`https:` はカスタム CSS の `@import` |
| `font-src` | `'self' data: https:` | カスタム CSS の Web フォント |
| `img-src` | `'self' asset: http://asset.localhost blob: data: https: http:` | 外部の絵文字・アバター・添付・URL プレビューのサムネイル、背景画像 |
| `media-src` | `'self' asset: http://asset.localhost blob: data: https: http:` | 添付の動画・音声、背景動画 |
| `connect-src` | `'self' ipc: http://ipc.localhost asset: http://asset.localhost` | Tauri の IPC と、背景の `fetch(convertFileSrc(...))` |
| `worker-src` | `'self' blob:` | Tone.js の時計が `blob:` の Worker を作る |
| `frame-src` | `https: http:` | `UrlPreviewCard` の埋め込みプレイヤー |
| `object-src` | `'none'` | 使っていない |
| `base-uri` | `'none'` | 使っていない |
| `form-action` | `'none'` | フォーム送信を使っていない |
| `frame-ancestors` | `'none'` | アプリを埋め込む用途が無い |

あわせて `dangerousDisableAssetCspModification: ["style-src"]` を指定する。Tauri は、同梱の HTML に `<style>` があると `style-src` に nonce を足す。nonce があるとブラウザは `'unsafe-inline'` を無視するので、将来 `index.html` に `<style>` が入ったときに、スタイルが黙って壊れる。`script-src` の自動の変更(同梱スクリプトの nonce とハッシュ)は残す。

`devCsp` は指定しない。開発時(`custom-protocol` feature なし)は、デスクトップは Vite のページを直接読み込み、Android は Tauri が Vite へ中継して CSP ヘッダーを付けない(`protocol/tauri.rs` で確認)。デスクトップで効かないことは、検証で確かめる。

### 判断したこと

- **`img-src` / `media-src` の `http:`**: アカウントのホストは `https` 固定だが、URL プレビューのサムネイルや本文由来の画像には `http` がありうる。`https:` で外部送信はすでに可能なので、`http:` を足しても失うものは小さい。
- **カスタム CSS の外部読み込み**: `https:` は残す。`http:` の `@import` とフォントは読めなくなる。
- **画像の取得先をアカウントのホストに限定する案**: 採らない。外部の絵文字や URL プレビューが壊れる。

## HLS / DASH の添付はファイルとして扱う

HLS / DASH のプレイリストはプレイヤーに渡さず、通常のファイル(`FileList`、保存ボタンあり)として表示する。Vidstack の CDN ローダーに到達する経路をなくし、「再生できないプレイヤー」も出さないため。Misskey 公式 Web が直接表示を許可する MIME の一覧にこれらは無いので、通常の添付では起きない。

`frontend/src/lib/mediaViewer.svelte.ts` に判定を足し、該当するときは `isVideo` / `isAudio` が `false` を返す。次のどちらかに当たるものが該当する。

- MIME が `audio/mpegurl` / `audio/x-mpegurl` / `video/mpegurl` / `video/x-mpegurl`
- URL が `/\.(m3u8|mpd)($|\?)/i` に一致する(Vidstack は MIME に関係なく URL でも HLS / DASH と判定する)

`application/*` の MIME は、もともと動画・音声として扱っていないので、追加しない。画像の判定は変えない。

Vidstack の Google Cast ローダーは明示的な Cast の要求でだけ動き、tsumugi にはその UI が無い。CSP でも読み込めない。

## 違反の記録

`securitypolicyviolation` イベントを受けて、`console.warn` に出し、`window.__cspViolations` に積む(件数の上限を設ける)。`frontend/src/lib/cspViolations.ts` に置き、`main.ts` で最初に設置する。E2E はこの配列を読んで、違反があればテストを失敗にする。

起動前の読み込み(同梱の JS と CSS)が遮断された場合は画面が出ないので、E2E の既存のテストが失敗する。

## 効果と限界

- **防げる**: 注入されたインラインスクリプト、`onerror=` などのイベント属性、`javascript:` URL、`eval`、外部スクリプトの実行。
- **防げない**: 画像や CSS の読み込みを使った外部への送信。同梱のスクリプトの脆弱性。任意パスの読み書き(2 本目の PR で扱う)。
- アプリ用の ACL マニフェストは、全コマンドに影響するので、本 Issue では扱わない。

## テスト

- **Rust**: `tauri.conf.json` を読むテスト。`script-src` が `'self'` だけ、`default-src` が `'self'`、`'unsafe-eval'` がどこにも無い、`object-src` が `'none'`、`connect-src` に `https:` / `http:` のスキーム指定が無い、`dangerousDisableAssetCspModification` が `["style-src"]`。
- **Vitest**: HLS / DASH の判定と、`MediaGrid` が該当する添付をファイル一覧に出すこと。違反の記録の上限。
- **E2E**: 各テストの後に `window.__cspViolations` が空であること。

## 検証

最初に、CSP を入れた埋め込みビルドで、WebDriver のスクリプト実行とデバッグブリッジ(`eval_with_callback`)が動くことを確かめる。どちらかが動かない場合は、止めて相談する。テストを通すために `'unsafe-eval'` を足すことはしない。

- **ビルド**: `pnpm build` のあと `cargo build --no-default-features --features custom-protocol`(llama.cpp を省く)。
- **起動**: 隔離インスタンス(Xvfb、`WAYLAND_DISPLAY` を外す、`dbus-run-session`、短い XDG パス)。
- **画面**: コードブロックの強調表示、作成欄、動画・音声(波形を含む)、画像ビューワー、MML の通知音(`blob:` の Worker が動くか)、背景の画像・動画、URL プレビューの埋め込みプレイヤー、カスタム CSS(`https` の `@import` を含む)、日時ピッカー、HLS 扱いの添付がファイルになること。
- **Android**: エミュレーターで同じ画面を確認する(オリジンは `http://tauri.localhost`)。
- **`cargo tauri dev`**: CSP が適用されないことを確かめる。
- **Windows / macOS**: 確認しない(未検証と PR に書く)。

実測で許可が足りないと分かった場合、画像・接続先などは、理由を本書に書き足して追加する。**`script-src` を緩める必要が出た場合は、止めて相談する。**

## 文書

- `CLAUDE.md`: 新しい読み込み先(画像・接続・スクリプトなど)を足すときは `tauri.conf.json` の `csp` を確認すること。開発時は CSP が効かないので、違反は埋め込みビルドでしか分からないこと。
- `docs/guide/user-guide.md`: カスタム CSS は `http://` の `@import` とフォントを読めないこと。

## 参考

- Tauri: https://v2.tauri.app/security/csp/

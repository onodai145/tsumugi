# フロントエンド単体テスト基盤の導入（Vitest）（Issue #130）

## 背景

Issue #73「テストをちゃんとやる」より。現状、Rust側は `cargo test` によるユニットテストが整備されている一方、フロントエンド（`frontend/`）はCI上で `svelte-check`/`tsc` による型チェックのみが行われており、ロジック・コンポーネントの単体テストは1件も存在しない。

Issue #73は「見た目の確認や操作が必要なテスト」の自動化と、テストによりリリース環境・他インスタンスへ影響を与えないための環境分離を求めているが、範囲が広いため以下のサブIssueに分割した。

- **#130（本spec）**: フロントエンド単体テスト基盤の導入（Vitest） — まず着手する範囲
- #131: Svelteコンポーネント単体テストの導入（Testing Library）
- #132: 操作テストのE2E自動化（Playwright/tauri-driver検討）

## 方針

Vite製プロジェクトである `frontend/` に Vitest を導入し、`frontend/src/lib/` 配下の DOM / Tauri IPC / Svelte runes に依存しない純粋ロジックから単体テストを整備する。既存の `frontend/vite.config.ts` にVitestの `test` ブロックを追加する形で導入し、別ファイル（`vitest.config.ts`）は作らない。

## 依存関係

`frontend/package.json` の `devDependencies` に追加:

- `vitest`
- `jsdom`（初回対象は純粋関数のみでDOM操作は無いが、`lib/` 内には将来DOMに触れるファイルもあるため環境として設定しておく）

## 設定

`frontend/vite.config.ts` の `defineConfig` に `test` ブロックを追加する。

```ts
test: {
  environment: "jsdom",
},
```

`frontend/package.json` の `scripts` に追加:

```json
"test": "vitest run"
```

（watchモードでの実行はローカルで `pnpm exec vitest` を直接使う想定とし、`package.json` には常駐スクリプトを増やさない。）

## テストファイル配置

対象ファイルと同ディレクトリに `*.test.ts` 命名で配置する（例: `frontend/src/lib/time.test.ts`）。

## 初回対象ファイル

`frontend/src/lib/` 配下のうち、Tauri IPC / Svelte runes / DOM に依存しない小規模な純粋ロジック:

- `time.ts`
- `nyaize.ts`
- `backgroundFitMode.ts`
- `backgroundPosition.ts`
- `emojiKey.ts`
- `keymap.ts`
- `mfm.ts`（DOM非依存部分のみ）

各ファイルについて、代表的な正常系＋境界値・エッジケース（空文字、不正入力など）を最低限カバーする。100%カバレッジは目標にせず、カバレッジ計測ツール（`@vitest/coverage-v8` 等）の導入も今回は見送る。

見送る理由: 初回対象は数十行規模の小さな純粋関数群で、手薄な箇所は目視でも十分把握できる。このspecの主目的はVitestという基盤をプロジェクトに導入しCIに乗せることであり、カバレッジ閾値やレポート設定まで同時に決めるとスコープが広がり着手が遅れる。カバレッジ計測が有効になるのはテスト対象・テストケースが増えてから（#131以降）であり、そのタイミングで改めて検討する。

## スコープ外

- `store.svelte.ts`（Svelte runes + Tauri IPC依存、1696行）、`ipc.ts`（Tauri invokeラッパー）、`theme.ts`（DOM CSS変数操作あり）は、Tauri APIのモックが必要になり複雑度が上がるため対象外。必要になれば別途検討する。
- Svelteコンポーネントの描画・操作テスト（`render/`, `ui/`, `input/`）は #131 で扱う。
- 実際の操作を伴うE2Eテスト（Playwright/tauri-driver）は #132 で扱う。

## CI統合

`.github/workflows/test.yml` の `frontend-check` ジョブに、`svelte-check` の後段として `pnpm test` ステップを追加する。PR作成時・main pushの両方で自動実行される。

## テスト方針（このspec自体の検証）

- `cd frontend && pnpm test` がローカルで通ること。
- `.github/workflows/test.yml` の `frontend-check` ジョブでVitestが実行され、CIがgreenになること。
- `pnpm check` に既存の型エラーが生じていないこと。

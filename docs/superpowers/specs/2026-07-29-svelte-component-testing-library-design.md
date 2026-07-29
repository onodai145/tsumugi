# Svelteコンポーネント単体テストの導入（Testing Library）（Issue #131）

## 背景

Issue #73「テストをちゃんとやる」より。#130（Vitest基盤導入）に続くサブタスク。#130ではDOM非依存の`frontend/src/lib/`純粋ロジックをカバーしたが、「見た目の確認や操作が必要なテスト」を自動化するというIssue #73本来の目的を満たすには、Svelteコンポーネントの描画・操作テストが必要になる。

## 方針

`@testing-library/svelte`（Svelte 5対応、v5.4.2）を導入し、`frontend/src/render/` 配下の3コンポーネント（`Mfm.svelte`, `CustomEmoji.svelte`, `Sparkle.svelte`）を対象に、実際の描画結果を検証するテストを追加する。MFM描画（`Mfm.svelte`）はIssue本文が挙げる「見た目の確認が必要」の代表例であり、これをカバーすることを主眼とする。

`MfmNode.svelte` はノード種別ごとの分岐を持つ再帰コンポーネントだが、`Mfm.svelte`（`text` props を受け取り mfm-js でパースして `MfmNode` へ渡すだけの薄いラッパー）を通してテストすれば、実際にMFM文字列を書くだけで全ノード種別を経由させられる。ノードオブジェクトを手で組み立てて `MfmNode.svelte` を直接テストするより、実際の入力（投稿本文の文字列）に忠実で、内部実装（`MfmNode.svelte` の再帰構造）に依存しないテストになるため、`MfmNode.svelte` 単体のテストファイルは作らず `Mfm.svelte` 経由の1ファイルに集約する。

## 事前検証で判明した技術的制約

設計段階で以下3点を実機検証済み（詳細は本specの実装時に再現される）。

### 1. Vitest実行時、Svelteパッケージがサーバービルドに解決される問題

`@testing-library/svelte` の `render()` は内部で Svelte 5 の `mount()` を呼ぶが、Vitestの既定解決では `svelte` パッケージの `exports` 条件が `node`/`ssr` 側に倒れ、`mount(...) is not available on the server` で失敗する。`frontend/vite.config.ts` に以下を追加して回避する:

```ts
resolve: process.env.VITEST ? { conditions: ["browser"] } : undefined,
```

（`process.env.VITEST` はVitest実行時にのみ設定される環境変数のため、通常の `vite dev`/`vite build` の解決には影響しない。）

### 2. `MfmNode.svelte` は import するだけで `store.svelte.ts` に到達しクラッシュする

`MfmNode.svelte` は `UnicodeEmoji.svelte` と `CodeBlock.svelte` を静的importしている。ES Modulesの静的importは実際にレンダリングされるかどうかに関わらず評価されるため、`MfmNode.svelte`（および `MfmNode.svelte` をimportする `Mfm.svelte`）をimportした時点で `UnicodeEmoji.svelte` → `frontend/src/lib/store.svelte.ts` → `frontend/src/lib/platform.ts` の連鎖importが発生する。`platform.ts` はモジュール評価時に `@tauri-apps/plugin-os` の `platform()` を同期呼び出ししており、これはTauriランタイム外（jsdom環境）では `Cannot read properties of undefined (reading 'platform')` で例外を投げる。

回避策として、`Mfm.svelte` を対象とするテストファイルの先頭で以下のモックを行う:

```ts
vi.mock("@tauri-apps/plugin-os", () => ({
  platform: () => "linux",
}));
```

これにより `store.svelte.ts` 自体のimportは通る（`store.svelte.ts` のクラス初期化はTauriの他のAPI呼び出しを伴わないため、これ単体のモックで十分なことを確認済み）。`store.svelte.ts` のロジック自体（`app` の各メソッド）はこのspecのテスト対象ではない。

なお、この連鎖import経路により `Mfm.svelte` のテストファイルは初回importに約10秒前後かかる（Shiki等の重い依存を含む `CodeBlock.svelte` まで静的importの評価対象に入るため）。機能的には問題なく完了するため許容する。

### 3. `window.matchMedia` はjsdomで未実装

`Sparkle.svelte` は `prefers-reduced-motion` の判定に `window.matchMedia(...)` を直接呼んでおり、jsdomには実装がないため未モックだと即例外になる。#130の `mfm.test.ts` で使ったのと同じ `vi.stubGlobal("matchMedia", ...)` パターンをテストファイル側で個別に使う（共通セットアップファイルは導入しない。対象ファイルが少なく、重複コストより設定の透明性を優先する）。

## 依存関係

`frontend/package.json` の `devDependencies` に追加:

- `@testing-library/svelte`

（`@testing-library/jest-dom` は今回のテストケースでは素の `expect`（`toBe`/`toBeTruthy`/`toBeNull`等）で十分書けたため導入しない。YAGNI。必要になった時点で追加を検討する。）

## 初回対象コンポーネント

- `CustomEmoji.svelte` — url有無によるimg/フォールバックテキストの出し分け
- `Sparkle.svelte` — 子要素(snippet)の描画、`prefers-reduced-motion`時にパーティクルレイヤーを描画しないこと
- `Mfm.svelte` — 実際のMFM文字列（`**bold**`、`$[tada ...]`、`$[ruby ...]` 等）を渡し、代表的なノード種別（bold, italic, strike, small, center, quote, link, url, mention, hashtag, emojiCode, inlineCode, fn装飾（既知/未知/ruby/unixtime/sparkle/clickable/plain）、nyaize）の描画結果を検証する

`MfmNode.svelte` が分岐するノード種別のうち、`unicodeEmoji`（`UnicodeEmoji.svelte` 経由）と `blockCode`（`CodeBlock.svelte` 経由）は、それぞれ `store.svelte.ts` の `app.emojiImageUrl()` / `app.ui.codeHighlightTheme` に依存した実際のレンダリングになるため、このspecでは描画結果の検証対象から除外する（importの連鎖自体は上記のモックで解消済みなので、これらのノード種別を経由しないテストケースを書く）。

## スコープ外

- `UnicodeEmoji.svelte` / `CodeBlock.svelte` 自体の単体テスト（`store.svelte.ts` 全体のモック方針の検討が必要なため、後続issueで扱う）
- `frontend/src/ui/`, `frontend/src/input/` のコンポーネント（Tauri IPCやDrive等への依存が強く、モック方針が別途必要）
- 「操作が必要なテスト」（クリック等のインタラクション）は今回の描画確認が中心で、必要に応じて後続issue(#132のE2E、または本specの追加PR)で扱う

## テストファイル配置

`frontend/src/render/` の対象ファイルと同ディレクトリに `*.test.ts` 命名で配置する（#130の `lib/` と同じ規約）。

## CI統合

追加のCI変更は不要。#130で `frontend-check` ジョブに追加済みの `pnpm test`（`vitest run`）がそのまま新規テストファイルを拾う。

## テスト方針

各コンポーネントについて、代表的な描画パターン（正常系）と分岐条件（props違いによる出し分け）を最低限カバーする。カバレッジ計測は#130と同様に見送る。

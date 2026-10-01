# fillRemainingGap のキャッシュ優先バイパス 設計 (Issue #427)

Issue #228 / #238 で導入した `fetch_backfill` のキャッシュ優先読み出しが、`fillRemainingGap`（Issue #148）からの呼び出しにも効いてしまい、ギャップを埋めないままマーカーを消す問題を直す。

## 背景・原因

`fillRemainingGap` は gapMarker `{boundaryId, targetId}` の区間 `(targetId, boundaryId)` を埋めるため、`fetch_backfill(tabId, boundaryId)` をループ呼び出しする。`fetch_backfill` は通常の上スクロールと区別できず、`boundaryId` が有効境界 E より新しいと `load_cached_before(boundaryId)` でキャッシュから返す。

1. 再接続時のギャップ埋め / 再起動時の `fill_gap` が `gap_fill_limit` で打ち切られ、gapMarker が作られる。
2. ユーザーがマーカーを押すと `fetchBackfill(tabId, boundaryId)` が呼ばれる。
3. `targetId` 以下のキャッシュ済みノートが20件以上あれば Hit する。
4. フロントは `fetched.some(n => n.id <= targetId)` を満たしたとみなしてマーカーを消す。区間 `(targetId, boundaryId)` は一度も取得されない。

単一ソースでも発生する(#228 から)。#238 でキャッシュ優先が複数ソースへ広がり、影響カラムが増えた。

## スコープ

- 対象: `fetch_backfill` に、キャッシュ読み出しを使わず必ずAPIへ行く指定を足し、`fillRemainingGap` がそれを使う。
- 対象外:
  - `fetch_notifications_backfill`（キャッシュ優先経路を持たない）。
  - ギャップ埋めのロジック自体（`fill_gap` / `gap_fill_on_reconnect`）。
  - #428（複数ソース+強いフィルタの読み飛ばし）。

## 設計

### Rust (`src-tauri/src/commands/column.rs`)

- `fetch_backfill` に必須引数 `bypass_cache: bool` を追加する。
- キャッシュ読み出し(`load_cached_before` 以降のHit判定)に入る条件を `cache_eligible` から `cache_eligible && !bypass_cache` へ変える。この条件は純粋関数 `should_try_backfill_cache(cache_eligible, bypass_cache) -> bool` に切り出す。
- 境界の取得と、API取得後の `plan_boundary_extend` / `extend_fetch_boundaries` は従来どおり `cache_eligible` で行う。バイパス時もAPI取得結果が既存境界と連続していれば境界は延びる(連続性は `plan_boundary_extend` が `until_id >= b_i` で検証する)。
- キャッシュ Hit/Fallback のメトリクス(`record_backfill`)はバイパス時に記録しない。ユーザー操作の上スクロールのHit率を、ギャップ埋めが歪めないため。
- 引数名は `bypass_cache`。specta の `camelCase` 変換でTS側は `bypassCache`。

### フロント (`frontend/src/lib/store.svelte.ts`)

- `loadMore`: `commands.fetchBackfill(tab.id, oldest, false)`。
- `fillRemainingGap`: `commands.fetchBackfill(tabId, boundaryId, true)`。
- `frontend/src/bindings/tauri.gen.ts` は生成物。`cargo test`（`generates_frontend_bindings`）で再生成し、手で編集しない。

### ドキュメント

- `docs/design/phase0-scaffold.md` のコマンド表 `fetch_backfill` の引数に `bypass_cache` を追記する。

## 代替案（不採用）

- ギャップ埋め専用の別コマンド `fetch_gap_backfill`: コマンドが増え、中身はほぼ重複する。呼び出し側の意図は引数名で足りる。

## テスト

- Rust: `should_try_backfill_cache` の単体テスト。`bypass_cache=true` なら `cache_eligible` でも false、`false` なら `cache_eligible` に従う。
- Vitest (`store.svelte.test.ts`): 既存の `fetch_backfill` モックで、`fillRemainingGap` が第3引数 `true`、`loadMore` が `false` で呼ぶことを検証する。
- `cd src-tauri && cargo test` / `cd frontend && pnpm check && pnpm test` が通ること。
- 実機確認(推奨、Xvfb+`dbus-run-session` 越し): 再接続等でギャップマーカーが残る状況を作り、マーカー押下後に区間のノートが埋まり、キャッシュ Hit メトリクスが増えないことを確認する。

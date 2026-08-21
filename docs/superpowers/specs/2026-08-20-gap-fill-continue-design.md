# 起動時ギャップ埋めの打ち切り分を手動で続きから取得する（Issue #148）

## 背景

Issue #148「以前のノートを取得する機能」は、当初「Krileにあった、あるノートから遡ってノートを受信する機能」として起票された。ブレインストーミングの過程で、以下が判明した。

- カラム下スクロール時の追加読み込み（`fetch_backfill`, `src-tauri/src/commands/column.rs:363`）は既にサーバーAPIを直接叩いており、ギャップなく連続的に過去へ遡れる。
- WebSocket切断→再接続時のギャップ埋め（`gap_fill_on_reconnect`, `column.rs:872`）も既に実装済み。
- アプリ再起動時のギャップ埋め（`fill_gap` 経由、`resume_column`, `column.rs:246`）も実装済みだが、**`gap_fill_limit`（デフォルト200件）を超える量のノートが閉じている間に投稿されていた場合、そこで打ち切られる**（`fill_gap`, `column.rs:713`）。

この打ち切りにより、「起動時ギャップ埋めで取得できた新しいノート群」と「元々キャッシュにあった最新ノート（`newest_known_id`）」の間に、恒久的に埋まらない空白ができる。この空白はタイムライン中間に位置するため、下スクロールでは再訪されない（スクロールは常にリスト末尾＝より古い方向にしか進まない）。

これが実質的にissue #148が指す問題である、という結論に至った。今回のスコープはこの「起動時ギャップ埋めの打ち切りによる中間の空白」を、ユーザーが手動で埋められるようにすることに絞る。

## スコープ

- 対象: ノートカラムの起動時ギャップ埋め（`resume_column` 内の背景ギャップ埋め、および `gap_fill_on_reconnect`）が `gap_fill_limit` に達して打ち切られたケース
- 対象外:
  - 通知カラムの同様の打ち切り（別issueで検討）
  - `gapMarker` のアプリ再起動をまたいだ永続化。セッション中に埋めなかった空白は、次回起動時に新たな `newest_known_id` を基準とした別のギャップ埋めが走るだけで、今回の仕組みでは再表面化しない。将来的にDBへ永続化する拡張は別issueとする
  - カラムに一度も表示されたことのない任意ノート（URL/ID）を起点にした新規取得（別issue候補）

## アーキテクチャ

### バックエンド

`fill_gap`（`src-tauri/src/commands/column.rs:713`）の戻り値を、単なる `Vec<Note>` から打ち切り情報を含む構造体に変更する。

```rust
struct GapFillResult {
    notes: Vec<Note>,
    /// newest_known_id に追いつく前に limit/ページ数上限で打ち切られた場合 true。
    truncated: bool,
    /// truncated=true のとき、取得できた中で一番古いノートのid。
    /// 「続きを取得」時の fetch_backfill の until_id に使う。
    boundary_id: Option<String>,
}
```

呼び出し元（`resume_column` の背景タスク、`gap_fill_on_reconnect`）は `truncated` と `boundary_id` を `ColumnGapFill` イベントに載せて送る。

`ColumnGapFill` イベント（`src-tauri/src/events.rs:20`）を拡張:

```rust
pub struct ColumnGapFill {
    pub column_id: String,
    pub notes: Vec<Note>,
    pub truncated: bool,
    pub boundary_id: Option<String>,
    /// truncated=true のときの到達目標（元のキャッシュ最新ノートid）。
    pub target_id: Option<String>,
}
```

「続きを取得」の実処理には新規コマンドを追加しない。既存の `fetch_backfill(column_id, until_id)` をフロントエンドから複数回ループ呼び出しすることで実現する。

### フロントエンド

`TabView`（`frontend/src/lib/store.svelte.ts`）に以下を追加:

```ts
gapMarker: { boundaryId: string; targetId: string } | null;
```

`ColumnGapFill` イベントハンドラで `truncated` なら `tab.gapMarker` をセットする（`false` なら `null` のまま、または既存マーカーがあれば消す）。

`Column.svelte` の描画: `tab.notes` を走査する際、要素の id が `gapMarker.boundaryId` と一致したら、その直後（より古い側）に区切り線＋「省略された投稿を表示」ボタンを描画する。

新規メソッド `app.fillRemainingGap(tabId: string)`（`store.svelte.ts`）:

1. `tab.gapMarker` が無ければ何もしない
2. 二重実行防止フラグ（例: `tab.fillingGap`）を立ててボタンをローディング表示に
3. 最大10ページを上限に、以下をループ:
   - `commands.fetchBackfill(tabId, boundaryId)` を呼ぶ
   - 結果を、`boundaryId` に該当するノートの直後（配列上の位置）に挿入。既存ノートとの重複はid照合で除外
   - 挿入したノートは `#captureInitial` でsubNote購読（Issue #3対策、既存 `loadMore` と同様）
   - 取得ページの中に `targetId` が含まれていれば、そのノードでギャップは完全に解消 → `tab.gapMarker = null` にしてループ終了
   - 含まれていなければ、`boundaryId` をそのページの最古ノートidに更新して次ページへ
   - ページが空、またはAPI呼び出し失敗の場合はループを打ち切り、`gapMarker` はそのまま残す（再度ボタンでリトライ可能）
4. 10ページ上限に達してもまだ `targetId` に到達しない場合、`gapMarker.boundaryId` を最新の境界に更新したまま残す（ボタン再表示、再クリックでさらに続きを取得できる）
5. 例外発生時は `#logFailure` でログし、`gapMarker` は変更しない

`MAX_NOTES`（300件）の切り詰めは既存の `loadMore` と同じ規約に従う（挿入後に超過したら末尾＝より古い側を切る）。

## エラーハンドリング

- `fetchBackfill` の失敗はループを止めてマーカーを保持するのみ。ユーザーは再度ボタンを押してリトライできる
- 打ち切り後にさらに新しいギャップ埋め（別の再接続など）が発生した場合の `gapMarker` の扱い: 新しい `ColumnGapFill(truncated=true)` が来たら、既存の `boundaryId`/`targetId` を新しい値で上書きする（複数マーカーの同時表示はサポートしない、シンプルさ優先）

## テスト

- Rust: `fill_gap` が `gap_fill_limit` 超過時に `truncated: true` と正しい `boundary_id` を返すことの単体テスト（既存の `fill_gap` テストを拡張）
- Frontend (Vitest): `fillRemainingGap` のループ終了条件（`targetId` 到達で解消、ページ上限到達で `gapMarker` 更新、失敗時は現状維持）

# タイムラインの返信可視性修正 設計書

Issue #297「TLに他の人への返信が表示されない」、#298「TLに自分への返信が表示されない」への対応設計。

## 1. 背景・根本原因

Misskey本家ソース（`misskey-dev/misskey`）とtsumugiのコードを突き合わせた調査結果（詳細は会話ログ参照）：

- `src-tauri/src/domain/column.rs` の `ColumnKind::Local`/`ColumnKind::Hybrid` の `stream_request()`/`rest_request()` が `withReplies` パラメータを一切送っていない（`json!({})` のまま）。
- Misskey本家の `notes/local-timeline` / `notes/hybrid-timeline`（REST）と `localTimeline` / `hybridTimeline`（Streamingチャンネル）は `withReplies` のデフォルトが `false`。指定しないと「返信ではない」か「投稿者自身への返信（自己スレッド継続）」以外の返信は全部弾かれる。
  - これが #297（フォローしていないユーザーへの返信が出ない）の直接原因。
- さらに本家ソースを読むと、**REST版とStreaming版で「返信先が自分」の扱いが非対称**になっている：
  - Streamingチャンネル（`local-timeline.ts`/`hybrid-timeline.ts`/`home-timeline.ts`）は `withReplies=false` でも「返信先が自分」なら例外的に通す実装。
  - REST（`notes/timeline`・`notes/local-timeline`・`notes/hybrid-timeline`）にはその例外が**存在しない**。自己スレッド継続以外は問答無用で除外。
  - この非対称のせいで「自分への返信」はライブ受信（Streaming）時は表示されるのに、初期ロードや遡り取得（REST）では消える。#298の「条件は謎」という報告内容と符合する。
- **Homeタイムラインは対象外**：`notes/timeline`（REST）にも `homeTimeline`（Streaming）にも `withReplies` 相当のパラメータがそもそも存在せず、上記の返信フィルタが常時無条件で有効。クライアント側からは制御不可能な、Misskey本家APIの制約。

## 2. 方針

トグルUIは追加しない。Local/Socialカラムは常に `withReplies: true` を送って返信を無条件で全部受け取り、絞り込みたい場合はTQLの `where` 句（`!reply`、`reply_to_me` 等）で対応する。

これにより：
- #297: 他人宛返信も含めて全部届くようになり解消。
- #298: REST/Streamingどちらの経路でも「全部届く」という同じ状態になるため、経路による非対称も解消。

TQLでの選別を実用的にするため、設計時から計画されていたが未配線だった `reply_to_me` 述語（`filter/eval.rs` で常時 `false` 固定）もあわせて配線する。

Homeタイムラインについては本家API側に制御パラメータが存在しないため、今回の変更では対応しない（既知の制約として明記）。

## 3. 変更内容

### 3.1 `domain/note.rs`
`Note` 構造体に `reply_user_id: Option<String>` を追加。`specta::Type` 付き、`camelCase` でTS側にも出力される。

### 3.2 `api/normalize.rs`
`RawNote` に `reply: Option<Box<RawNote>>` を追加（既存の `renote: Option<Box<RawNote>>` と同じパターン）。正規化処理で以下のように `reply_user_id` を埋める：

```rust
reply_user_id: r.reply.as_ref().map(|reply| reply.user.id.clone()),
```

### 3.3 `filter/eval.rs`
```rust
// before
// reply_user_id は domain::Note に無いため未対応（常に false）
ReplyToMe => false,

// after
ReplyToMe => n.reply_user_id.as_ref().map_or(false, |u| ctx.my_user_ids.contains(u)),
```

### 3.4 `store/note_cache.rs`
`reply_user_id` 列への書き込みをハードコードされた `None` から実値（`domain::Note.reply_user_id`）に変更。SQLiteスキーマ（`store/db.rs`）の `reply_user_id TEXT` 列は既存のため、マイグレーション不要。

### 3.5 `domain/column.rs`
`ColumnKind::Local` / `ColumnKind::Hybrid` の `stream_request()` と `rest_request()` の両方に `withReplies: true` を追加：

```rust
// stream_request()
ColumnKind::Local => ("localTimeline", json!({ "withReplies": true })),
ColumnKind::Hybrid => ("hybridTimeline", json!({ "withReplies": true })),

// rest_request() の body へも同様に withReplies: true を追加
```

`ColumnKind::Home` / `ColumnKind::Global` / その他は変更しない。

### 3.6 既存テストへの波及
- `Note` リテラルを組み立てているテストコード（`filter/mod.rs`, `filter/eval.rs`, `commands/column.rs`, `commands/note.rs` 等）に `reply_user_id: None` を機械的に追加。
- `ColumnKind::stream_request`/`rest_request` の既存テスト（`domain/column.rs` 内）のアサーションを `withReplies` を含む形に更新。
- `reply_to_me` 述語の新規ユニットテスト（`filter/eval.rs`）：自分宛の返信で `true`、他人宛で `false` になることを確認。

### 3.7 フロントエンド
`frontend/src/bindings/tauri.gen.ts` は `cargo test`（`generates_frontend_bindings`）または `cargo tauri dev` で自動再生成されるため、手動編集しない。`Note.replyUserId` フィールドが型に追加される想定だが、フロントエンド側で明示的に参照する箇所は今回追加しない（TQL経由の絞り込みは既存の `where` パーサ/評価器がそのまま使う）。

## 4. ドキュメント更新

- `docs/design/filter-dsl-design.md`: 変更不要（もともと `reply_to_me` は実装済み前提で書かれている）。
- 本設計書自体が今回の変更の記録。

## 5. スコープ外・既知の制約

- Homeタイムラインの返信可視性はMisskey本家API側の制約でクライアントから制御不可能。ユーザーには「Home以外（Local/Social）で確認してほしい」旨をIssueにコメントする想定。
- TQL以外での「デフォルトで返信を隠す」設定（トグルUI）は作らない。

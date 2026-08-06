# チャンネル投稿機能 設計 (Issue #95)

## 背景・目的

tsumugi はフォロー中チャンネルのタイムライン閲覧(`ColumnKind::Channel` カラム)には既に対応しているが、チャンネルへの投稿手段がない。Misskey の `notes/create` API は `channelId` パラメータを既にサポートしているが、tsumugi の `NoteDraft` にはこのフィールドがなく、コンポーズバーにもチャンネル選択 UI がない。本設計はチャンネルへの投稿を可能にする。

## スコープ

- コンポーズバーからフォロー中チャンネルを選択して投稿できるようにする。
- チャンネル内ノートへの返信時、返信元の `channel_id` を自動検出してチャンネルを事前選択する。
- チャンネル一覧はフォロー中チャンネルのみ(検索は対象外)。
- チャンネルタイムラインカラムからの新規投稿導線(FABからの自動選択など)は対象外(将来の別issue)。

## バックエンド (Rust)

- `src-tauri/src/api/notes.rs` の `NoteDraft` に `channel_id: Option<String>` を追加する。既存の `visibility`/`cw`/`reply_id`/`renote_id`/`local_only` と同列のフィールドとして扱う。
- `create_note` は `NoteDraft` をそのまま `notes/create` に POST しているため、フィールド追加のみで `channelId` がサーバーに渡る。`post_note` コマンド(`src-tauri/src/commands/note.rs`)のシグネチャ変更は不要。
- チャンネル一覧取得は既存の `list_channels` コマンド(`fetch_followed_channels` をラップ、`AddColumnModal.svelte` で使用中のもの)をそのまま再利用する。新規 API 呼び出しの追加は不要。
- `specta_builder()` の登録変更は不要(既存コマンドの型変更のみ)。`cargo test` の `generates_frontend_bindings` テストが `channelId` の camelCase 変換とトークン非露出を検証する。

## フロントエンド (Svelte)

### `ComposeBar.svelte`
- `channelId: string | undefined` の状態を追加する。
- フォロー中チャンネル一覧を取得し、チャンネルセレクタ(ドロップダウン)を追加する。
- `channelId` が選択されている間は `visibility` ピッカーを非表示または無効化する(Misskey サーバー側で `channelId` 指定時は `visibility` が `public` に強制されるため)。CW・ローカルオンリー等の他オプションは選択可能なまま維持する。
- 投稿時は `NoteDraft` に `channelId` を積んで `post_note` を呼ぶ。

### チャンネル自動選択(返信・引用)
- `store.svelte.ts` の `openCompose(accountId, opts)` にて、`opts.replyTo` / `opts.quoteOf` として渡されたノートの `channelId` を確認し、値があれば `channelId` state を事前選択する。ユーザーは選択後に手動で変更・解除できる。
- `openCompose` の `opts` 型に `channelId?: string` を追加する。

### エラーハンドリング
- チャンネル一覧取得失敗時は `AddColumnModal.svelte` に既存のエラー表示パターンを踏襲する。

## テスト

- Rust: `cd src-tauri && cargo test` — 既存の bindings 再生成テスト(`generates_frontend_bindings`)が `channelId` の camelCase 変換とアカウントトークン非露出を検証することを確認する。
- Frontend: `cd frontend && pnpm check`(svelte-check + tsc)。
- 手動確認: `cargo tauri dev` を用いて、フォロー中チャンネルへの実投稿、およびチャンネル内ノートへの返信時の自動選択を確認する。

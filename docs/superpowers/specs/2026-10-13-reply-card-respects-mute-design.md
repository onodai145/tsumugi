# 返信先カードがミュート設定を考慮する設計

Issue #486(関連: #287 / PR #483、`2026-10-10-reply-target-preview-design.md`)

## 目的

返信先カード(#287)は、返信先ノートの表示名・アバター・本文を出す。しかしミュート判定(ローカルNG・サーバーミュート・ワードミュート)は本体と `renote` 先しか見ておらず、`reply`(返信先)を見ない。そのため、ミュートしたユーザーや単語を含むノートへの返信でも、カラムのタイムラインと通知カードに返信先の内容が出る。

返信先がミュートに該当するときは、返信先カードを出さず、従来の「↩ 返信」だけにする。返信ノート自体の表示・非表示は、これまでのミュート判定のまま変えない。

成功条件:

- 返信先が NG ユーザー・NG インスタンス・NG ワード・サーバーミュート/ブロック・サーバーワードミュートのいずれかに該当するとき、カラムのタイムラインと通知カードは「↩ 返信」だけを出す(返信先の表示名・アバター・本文を出さない)。
- 返信先が該当しないときの表示は変わらない。
- 純粋な Renote の Renote 先に付いた返信先(`note.renote.reply`)も同じ扱いにする。
- ミュート設定は後から変わるため、キャッシュには元のノートを保存し、配信時・読み出し時に返信先を外す。
- ローカルNGを保存した直後は、表示中のノートと通知の返信先カードも即座に「↩ 返信」へ戻す。サーバーミュート・ワードミュートの変更は、次の受信・読み出しから反映する。
- 展開した引用など、もともと返信先カードを出さない場所に返信先の内容が出ないことを、テストで固定する。

## 前提(コードの現状)

- 返信先カードを描画するのは `Column.svelte` と `NotificationCard.svelte` だけ(`showReplyCard`)。`NoteCard.svelte` は `inner.replyId && replyTarget`(`inner.reply ?? null`)の両方があるときだけカードを出し、`reply` が `None` なら従来の「↩ 返信」になる。UI の変更は要らない。
- 純粋な Renote は `inner = note.renote` を使うため、カードの元になるのは `note.renote.reply`。
- ミュート判定は `filter/mute.rs`(`is_muted` / `is_word_note_muted`)、`AppState::is_server_muted` / `is_word_muted`、`connection.rs` の `is_server_muted_note`、`column.rs` の `server_muted_note` に分かれ、いずれも本体と `renote` 先だけを見る。
- 通知は `filter_notifications`(`column.rs`、3 か所から呼ばれる)とストリーム側(`connection.rs` の `notification_muted`)を通るが、発生元ユーザーしか見ない。`Notification.note`(mention / reply / quote 等の対象ノート)の返信先は何も判定されない。
- ストリーム受信では、ミュート判定を通ったノートを `cache_note` で保存してから `emit` している(`connection.rs`)。
- フロントの `setMute`(`store.svelte.ts`)は、保存したローカルNG設定を `isMuted` で表示中のノートに再適用して取り除く。

## 設計

### 1. 判定(`filter/mute.rs`)

`is_reply_target_muted(reply: &Note, cfg: &MuteConfig, server_muted: bool, word_rules: &[WordMuteRule]) -> bool` を追加する。次のいずれかで真:

- ローカルNG: 既存の `is_muted_one(reply, cfg)`(ユーザー・インスタンス・NG ワード)に該当する。
- サーバーミュート/ブロック: 呼び出し側が `reply.user.id` で判定した `server_muted` が真。
- サーバーワードミュート: `is_word_muted(reply.text, reply.cw, word_rules)`。

`is_muted_one` は非公開のままにし、同じファイル内の新関数から使う。

### 2. 除去(`state.rs`)

`AppState::strip_muted_reply(&self, account_id: &str, note: &mut Note)` を追加する。

- `note.reply` が `Some` で `is_reply_target_muted` が真なら `None` にする。
- `note.renote` の中の `reply` にも同じことをする。
- `reply_id` / `reply_user_id` は残す(`reply_to_me` 述語と「↩ 返信」の表示に必要)。
- ミュート設定(`self.mute`、`server_mutes`、`server_word_mutes`)は呼び出しのたびに読む。複数ノート用に `strip_muted_replies(&self, account_id, &mut [Note])` と、通知用に `strip_muted_reply_in_notifications` を同じ場所に置く。

### 3. Rust の適用地点

既存のミュート判定(`is_muted` / `server_muted_note` / `is_word_muted`)を通ったノートに対し、配信または返却の直前に `strip_muted_reply` を呼ぶ。除去は判定を通った後なので、本体の表示・非表示は変わらない。

- ストリーム受信(`connection.rs`): `cache_note` には元の `normalized` を渡し、`emit` 用のコピーにだけ除去を適用する。
- バックフィルとキャッシュ読み出し(`column.rs`): 初期ロード・追加ロード・差分補完の各 `retain` / `collect` の後。
- キャッシュ検索(`search_cache_core`)とサーバー検索(`apply_search_mutes`)の結果。
- 通知: `filter_notifications` の戻り値と、ストリーム側の通知配信で、`Notification.note` の返信先を同様に除去する。

### 4. フロント

- `store.svelte.ts` に `stripMutedReply(n: Note, cfg: MuteConfig): Note` を追加する。`noteMutedOne(n.reply, cfg)` が真なら `reply: null` のノートを返し、`n.renote.reply` も同様に扱う。該当しなければ同じオブジェクトを返す。
- `setMute` で、`isMuted` による除外のあとに、表示中のノート(`tab.notes`)と通知(`tab.notifications` の `note`)へ適用する。サーバーミュート・ワードミュートはフロントから見えないので、ここでは扱わない。

### 5. 既知の挙動

- 一度外した返信先カードは、ミュートを解除しても再読み込みまで戻らない(本体の非表示と同じ扱い)。
- サーバーミュート・ワードミュートの変更直後は、表示中のカードに反映されない。次の受信・読み出しから反映する。

### 6. ドキュメント

`docs/guide/user-guide.md` の返信先の表示の項から、ミュート設定が適用されないという既知の制限の記述を、実際の挙動(ミュート対象の返信先は「↩ 返信」だけになる)の説明に書き換える。

## テスト

Rust:

- `is_reply_target_muted`: NG ユーザー・NG インスタンス・NG ワード・サーバーミュート・サーバーワードミュートのそれぞれで真、非該当で偽。
- `strip_muted_reply`: `reply` と `renote.reply` が外れる。`reply_id` は残る。非該当なら `reply` が残る。
- ストリーム経路: `cache_note` に渡るノートの返信先は除去されず、`emit` されるノートでは除去される。
- `filter_notifications`: 通知ノートの返信先が除去され、通知自体は残る。
- バックフィル・キャッシュ検索・サーバー検索: 返却ノートの返信先が除去され、本体は残る。

フロント(Vitest):

- `stripMutedReply`: ローカルNGの各種別と `renote.reply`。非該当では同一オブジェクトを返す。
- `setMute`: 表示中のノートと通知の返信先が外れ、本体は残る。
- 展開した引用など、返信先カードを出さない場所では、`reply` の内容が描画されないこと。

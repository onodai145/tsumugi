# 返信先を分かりやすくする(返信先プレビュー)設計

Issue #287

## 目的

タイムライン上で返信ノートを見たとき、どのノートへの返信かが分からない。現状は返信ノートの上に「↩ 返信」という固定文言が出るだけで、返信先の投稿者も本文も見えない。

成功条件:

- カラムのタイムラインと通知カードでは、返信ノートの上に、返信先のアバター・表示名・`@acct`・相対時刻と本文(改行を残して最大3行)を載せたカードが表示される(Misskey 本家に近い見た目)。
- それ以外の場所(検索モーダル・プロフィールモーダル)では、従来どおり投稿者名と本文冒頭の1行プレビューが表示され、クリックすると返信先ノートがその場で展開され、もう一度クリックすると折りたたまれる。
- 返信先が取れない場合(削除済み・閲覧不可・旧キャッシュ行)は、従来どおり「↩ 返信」だけを表示する(通知カードでは何も出さない)。
- 追加のネットワーク通信は発生しない。
- 既存のキャッシュ(旧行)がそのまま読め、マイグレーションは不要。

## 現状

- `domain::Note` は `reply_id` と `reply_user_id` しか持たない。
- Misskey は `reply` にネストした返信先ノートを返しており、`RawNote.reply` は受けているが、`normalize.rs` は `reply.user.id` を `reply_user_id` に抜き出した後で捨てている。
- `renote: Option<Box<Note>>` が「浅く保持」する前例としてある。
- キャッシュは `note.payload` (JSON) にノート全体を保存し、ユーザーは `user` テーブルへ正規化する。保存時に `user` を `{"id": ...}` のスタブへ差し替え、復元時に `user` テーブルから埋め戻す。この処理は `renote` だけを再帰する作りになっている。

## 採用しない案

- **フロントが `notes/show` で返信先を遅延取得する**: `Note` 型は変わらないが、新規コマンド、フロント側キャッシュ、取得中・失敗の状態管理が要る。さらにスクロールのたびにリクエストが飛ぶ。Misskey が既に返している情報を捨てて取り直すことになるため割に合わない。
- **`reply` が無いときだけ `notes/show` で補う併用案**: 欠落ケースを救えるが、実際に問題になってから足せば足りる(YAGNI)。

## 設計

### 1. ドメイン型

`src-tauri/src/domain/note.rs` の `Note` に次を追加する。

```rust
/// 返信先ノート(浅く1階層のみ保持。`reply.reply` は常に None)。表示用で、フィルタ評価には使わない
pub reply: Option<Box<Note>>,
```

- `reply_id` と `reply_user_id` は TQL (`reply` / `reply_to_me` 述語、`filter/` と各キャッシュ SQL 列)が使っているため変更しない。
- `Option` なので、`reply` キーを持たない旧 payload は `None` として読める。
- TS バインディング (`frontend/src/bindings/tauri.gen.ts`) は `cargo test` の `generates_frontend_bindings` で再生成する。手編集しない。

### 2. 正規化

`src-tauri/src/api/normalize.rs` の `From<RawNote> for Note` で、`r.reply` を `Note` に変換して `reply` に詰める。`reply_user_id` は従来どおり `r.reply` から取る。

浅く保持するため、変換した `reply` の `reply` は `None` に落とす。`renote` は変換したままだが、`reply.renote` はそのまま保持してよい(返信先が引用ノートの場合に引用を表示できる)。

### 3. キャッシュ (user 参照の正規化)

`renote` だけを再帰している次の処理に `reply` を加える。保存時と復元時の処理が非対称になると、保存時に `reply.user` だけスタブ化されて復元時に戻らず、`User` として読めずにノート行がスキップされるため、全て同じ変更で揃える。

| 関数 | 場所 | 変更 |
|---|---|---|
| `collect_users` | `store/user_ref.rs` | `reply` の投稿者も返す(保存時に `user` テーブルへ upsert される) |
| `stub_user_refs` | `store/user_ref.rs` | `reply.user` もスタブ化する |
| `collect_user_id_refs` | `store/user_ref.rs` | `reply.user.id` も集める(`fetch_users_by_ids` が1クエリで引く対象に含める) |
| `has_legacy_full_user` | `store/user_ref.rs` | `reply` 側の旧形式も検出する |
| `hydrate_user_refs` | `store/user_ref.rs` | `reply.user` を埋め戻す(下記の縮退あり) |
| `self_heal_node` | `store/note_cache.rs` / `postgres_backend.rs` / `mysql_backend.rs` | `reply` へ再帰する |

**縮退**: 返信先は補助表示のため、`reply.user` が `user` テーブルに無くても、ノート行を捨てずに `reply` だけを `null` にして復元する。本体と renote の `user` が欠けた場合は従来どおり復元不可(スキップ)。

DB スキーマは変更しない。`reply` は payload 内の追加フィールドであり、カラム追加もマイグレーションも要らない。

### 4. フロントエンド (`frontend/src/ui/NoteCard.svelte`, `ReplyTargetCard.svelte`)

`inner.replyId` のときに出している「↩ 返信」バナー(`NoteCard.svelte` 358行目付近)を、表示場所によって2通りに出し分ける。`NoteCard` の新 prop `showReplyCard`(既定 `false`)で切り替える。

#### 4.1 返信先カード(`showReplyCard` 真: カラムのタイムライン・通知カード)

`frontend/src/ui/Column.svelte` と `frontend/src/ui/NotificationCard.svelte` の `<NoteCard>` だけが `showReplyCard` を渡す。返信の上に、新規コンポーネント `ReplyTargetCard.svelte` を出す。

- 左に小さいアバター(`h-6 w-6`)、右の1行目に表示名・`@acct`・右端に相対時刻(`app.now` を読んで再計算する。`NoteCard` の `displayTime` と同じ作り)。
- 2行目以降は本文。優先順位は CW → 本文 → ファイル件数(「(1つのファイル)」「(N個のファイル)」) → 本文なしの Renote(「(Renote)」)。何も無ければ本文行を出さない。本文決定は `frontend/src/lib/replyPreview.ts` の `replyCardBody` が担う(空白は畳まず trim のみ)。
- 本文は改行を残して `line-clamp-3` で最大3行に切る。`Mfm simple` で描画し、絵文字は閲覧インスタンスのローカル絵文字をフォールバックに重ねた解決(`NoteCard` の `replyEmojiMap`)を使う。
- 色は `text-muted-foreground` で、返信本体より目立たせない。
- カードは常に表示し、展開トグルは持たない。アバターと表示名のクリックは `openProfile({ userId }, accountId)`(`NoteCard` 既存のアバター操作と同じ)。カード本体のクリックは親の選択(bubbling)のまま。
- `showReplyCard` が真で返信先(`inner.replyId` かつ `inner.reply`)があるときは、`hideActionBanner` が立っていてもカードを出す。通知カードでは通知の種類を問わず、通知に紐づくノートが返信ならカードが出る。
- `showReplyCard` が真で返信先が取れないとき: `hideActionBanner` なら何も出さず、そうでなければ従来どおり「↩ 返信」。

#### 4.2 1行プレビュー(`showReplyCard` 偽: 検索モーダル・プロフィールモーダル・展開した引用)

`inner.reply` がある場合: `↩ 表示名: 本文冒頭…` を1行(`truncate`)で表示する。

- 表示名とプレビューの本文は MFM を `simple` で描画する(Renote バナーと同様)。
- 返信先に CW があれば、本文の代わりに CW 文言を表示する。
- 本文が空でファイルがあれば「(画像)」「(ファイル)」のように表示する。
- 全て空なら表示名だけにする。
- 本文の改行・連続した空白は半角スペース1つに畳む(`replyPreviewBody`)。
- `inner.reply` が無い場合: 従来どおり「↩ 返信」。
- 1行プレビューは `button` とし、クリックで `quoted` 表示の `NoteCard` として返信先をその場に展開し、もう一度押すと折りたたむ。展開状態はカードごとのローカル state (`$state`) で持ち、永続化しない。
  - 展開した返信先は `hideReactions` を付け、`showActions` は偽にする(`quoted` の既定)。
  - 展開した返信先のカードは、さらに自身のバナーを出さない(`hideActionBanner`)。返信の連鎖を辿る機能はこの Issue の範囲外とする。
- `hideActionBanner` のときは従来どおりバナーごと非表示。

色・角丸・フォントサイズ・アイコンサイズは `docs/design/style-guide.md` のスケールに従い、新しい任意値を作らない。

### 5. ドキュメント

- `docs/guide/user-guide.md` に、返信先カード(カラム・通知カード)と、それ以外での1行プレビューのクリック展開・折りたたみを追記する。
- `docs/design/misskey-multicolumn-client-design.md` の `Note` 定義に `reply` を追記する(設計書が正)。

## テスト

Rust:

- `normalize.rs`: ネストした `reply` が `Note` として入ること。`reply.reply` が `None` になること。`reply` が無い応答で `None` になること。
- `user_ref.rs`: `collect_users` / `stub_user_refs` / `collect_user_id_refs` / `has_legacy_full_user` / `hydrate_user_refs` が `reply` を扱うこと。`reply.user` が欠けたとき `reply` だけ `null` になり、行は復元できること。
- `note_cache.rs`: 保存して読み戻すと `reply` と `reply.user` が元に戻ること。`reply` を持たない旧 payload が読めること。
- Postgres / MySQL の実 DB テスト(`postgres_` / `mysql_` の `#[ignore]` テスト)にも同じ往復を追加する。

フロント (Vitest):

- `replyPreview.test.ts`: `replyCardBody` の優先順位、改行の保持、ファイル1件/複数件のラベル、Renote、空白のみ→null。既存の `replyPreviewBody` が変わらないこと。
- `ReplyTargetCard.test.ts`: アバター・表示名・`@acct`・相対時刻、CW、ファイルラベル、本文の `line-clamp-3`、展開トグルが無いこと、アバター/名前クリックでの `openProfile`、ローカル絵文字の解決。
- `NoteCard.test.ts`: `showReplyCard` なしは従来の1行表示+展開・折りたたみ、CW、「(画像)」、「↩ 返信」へのフォールバック、`hideActionBanner` で1行プレビューが出ないこと。`showReplyCard` ありでカードが出て1行プレビューが出ないこと、`hideActionBanner` 併用でもカードが出ること、返信先なしでは `hideActionBanner` なら何も出ず、そうでなければ「↩ 返信」。
- `NotificationCard.test.ts`: 返信ノートに紐づく通知でカードが出ること、返信先が無ければ出ないこと。

実画面確認: Xvfb 越しの隔離インスタンスで、実際の返信ノートの表示(カラムのカードと、モーダルの1行プレビューの展開)を確認する。

## 範囲外

- 返信の連鎖(祖先ノートまでの辿り)の表示。
- `reply` が取れなかったノートを `notes/show` で補う処理。
- 返信先へのスクロールジャンプ。
- TQL からの `reply` の中身の参照。
- 返信先カードの展開(カラム・通知カードでは常にカード表示で、展開トグルを持たない)。

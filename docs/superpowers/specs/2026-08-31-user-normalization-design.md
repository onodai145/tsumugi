# ユーザー情報(instance含む)のpayload非正規化解消 設計

Issue #263

## 背景

`Note.user`（`InstanceInfo` を含む）は、Misskey APIのレスポンスをそのまま `note.payload`（JSON丸ごと保存）に埋め込んでキャッシュしている。同一ユーザーの投稿を100件キャッシュすれば、同じ `instance` 情報が100回重複して保存される。

`user` テーブル自体は既に存在するが `id/username/host/name/is_bot/is_cat/*_count` のみを持つ、検索フィルタ用のJOIN対象でしかなく、表示復元は常に `payload` から行われる。そのため「ユーザー情報を1箇所で最新化する」仕組みが無く、Instance Ticker実装（Issue #103）より前にキャッシュされた古いノート（手元の `cache.db` 48,470件中43,993件、91%）には `user.instance` が無いままになる。

暫定対応として `NoteCard.svelte` にホスト名だけのフォールバック表示を追加済み（Issue #262作業中）。本Issueは根本原因（ユーザー情報の非正規化）を解消する。

## 方針

`user` テーブルを実質的な正規化テーブルに格上げし、`note.payload` からはユーザーのフルオブジェクトを取り除いて `{"id": "..."}` スタブのみを持たせる。読み込み時に `user` テーブルとJOIN/引き当てて `Note` を再構成する。

既存の48,470件のキャッシュ移行は、起動時の一括バッチ処理ではなく、**読み込み時に旧形式を検知したらその場で自己修復する遅延方式**を採る。`store/db.rs` 冒頭のコメント「キャッシュDBは破棄しても再取得で復元できるため重いマイグレーションを持たない」という既存方針に沿う。

## スキーマ変更

`src-tauri/src/store/db.rs` の `migrate_cache`（`column_exists` チェック＋`ALTER TABLE` の冪等パターン、既存の `column_note.created_at` 追加と同じ形）に以下を追加する。

```sql
ALTER TABLE user ADD COLUMN avatar_url TEXT;
ALTER TABLE user ADD COLUMN bio TEXT;
ALTER TABLE user ADD COLUMN banner_url TEXT;
ALTER TABLE user ADD COLUMN emojis TEXT NOT NULL DEFAULT '{}';        -- JSON: {name: url}
ALTER TABLE user ADD COLUMN instance_name TEXT;
ALTER TABLE user ADD COLUMN instance_icon_url TEXT;
ALTER TABLE user ADD COLUMN instance_theme_color TEXT;
```

3列に分割するのは、将来TQLで `instance.name` 等を単体カラムとしてSQL射影・検索したくなった場合に備えるため。`InstanceInfo` が Some のときは3列とも埋まり、None のときは3列とも NULL という不変量を書き込み側で常に保つ（部分的な欠損は発生させない）。

## 書き込みパス

### payloadのスタブ化（`upsert_note`）

`serde_json::to_string(n)` する前に、`Note` を `serde_json::Value` へ変換し、`user` フィールドを再帰的に `{"id": <元のid>}` へ差し替えてから保存する。対象は:

- ノート本体の `user`
- `renote`（入れ子）が存在する場合、その `renote.user`（Misskeyの仕様上renoteのrenoteは展開されないため実運用では深さ1だが、コードは深さに依存しない再帰関数として書く）

### `user` テーブルへのupsert

ノート本体・renote本体それぞれのフル `User` オブジェクトに対して `upsert_user` を呼ぶ。

```sql
INSERT INTO user (
    id, username, host, name, avatar_url, is_bot, is_cat,
    followers_count, following_count, notes_count, emojis,
    bio, banner_url, instance_name, instance_icon_url, instance_theme_color
) VALUES (...)
ON CONFLICT(id) DO UPDATE SET
    username = excluded.username,
    host = excluded.host,
    name = excluded.name,
    avatar_url = excluded.avatar_url,
    is_bot = excluded.is_bot,
    is_cat = excluded.is_cat,
    followers_count = excluded.followers_count,
    following_count = excluded.following_count,
    notes_count = excluded.notes_count,
    emojis = excluded.emojis,
    bio = COALESCE(excluded.bio, user.bio),
    banner_url = COALESCE(excluded.banner_url, user.banner_url),
    instance_name = COALESCE(excluded.instance_name, user.instance_name),
    instance_icon_url = COALESCE(excluded.instance_icon_url, user.instance_icon_url),
    instance_theme_color = COALESCE(excluded.instance_theme_color, user.instance_theme_color)
```

`username/host/name/avatar_url/is_bot/is_cat/*_count/emojis` は Misskeyの `UserLite`（ノートに常に付随する最小限のユーザー情報）で毎回必ず埋まるため常に上書きする。`bio/banner_url/instance_*` は `UserLite` コンテキストでは省略されうる（ローカルユーザーの `instance`、フェッチ失敗時の `instance: null`、ノート由来では取得されない `bio`/`banner_url`）ため、`COALESCE` で「新しい値が無ければ既存値を残す」。これにより:

- `commands/user.rs` がフルユーザー取得で先に書いた `bio`/`banner_url` を、後続のノート受信（`UserLite`のみ）の `NULL` で踏み潰さない。
- `instance` フェッチが一時的に失敗した投稿（`"instance":null`、実データで298件）が、既に分かっている `instance` を消さない。

## 読み込みパス

`load_cached` / `load_cached_before` / `get_note` / `search_cache` は、`payload` 文字列を取得した後、以下の共通処理を通す。

### 1. 旧形式の自己修復

`payload["user"]` が `"username"` キーを持つフルオブジェクトであれば旧形式と判定する。

1. 該当する `user`（ノート本体・renote分すべて）を `upsert_user` で `user` テーブルへ書き込む。
2. `payload` 中のそのユーザーを `{"id": ...}` スタブへ差し替え、`UPDATE note SET payload = ?1 WHERE id = ?2` で書き戻す。
3. 抽出（JSONパースやフィールド読み取り）に失敗した場合は、その行の `payload` には一切触れず元のまま次に進む（既存ユーザーデータを失う書き換えを絶対に行わない）。

一度修復された行は次回以降スタブ形式として扱われ、二度とこの分岐に入らない（行単位で生涯1回のコスト）。

### 2. ハイドレーション

その呼び出しで取得した全行ぶんの `user.id`（renote分含む）を重複排除して集め、1回の `SELECT ... FROM user WHERE id IN (...)` でまとめて引く（N+1にしない）。結果を `HashMap<id, ユーザーJSON>` にし、各行のスタブへ埋め戻してから `Note` へデシリアライズする。

### 3. 欠落時のポリシー

ハイドレーション対象の `id` が `user` テーブルに存在しない場合、そのノートは既存の `deserialize_note_or_warn`（JSONパース失敗時）と同じ扱いにする: ログ警告を出し、そのノートは結果から除外する（呼び出し元をエラーにしない）。

## 影響範囲外

- `domain::Note` / `domain::User` のRust型・TSバインディング（`bindings/tauri.gen.ts`）は変更しない。インメモリ表現・Tauri IPC表現は今まで通りフル埋め込みのまま。変わるのは SQLite 保存表現（`note.payload` の中身）だけ。
- `prune` / `shrink_to_size` 等の既存ノート削除ロジックは変更しない。`user` テーブル行のGC（どのノートからも参照されなくなったユーザー行の削除）は今回スコープ外とする。各ユーザーにつき最新情報1行を持つだけなので肥大化は緩やかであり、必要になれば別Issueで対応する。

## パフォーマンスへの影響

- 読み込み1件あたり、`serde_json::Value` 経由の往復が挟まる分JSON処理コストは増える（概ね1.5〜2倍程度）が、`load_cached` 系は常に `limit`（数十〜百件程度）で頭打ちのため、48,470件全体にはスケールしない。
- user解決は呼び出し1回につき1クエリ（`IN (...)`、`id` はPRIMARY KEY）。
- 書き込みは、renoteがある場合に `user` upsertが+1回増えるのみ。既存の `cache_notes` は元々ノートごとのループを1トランザクションに包んでいるため、トランザクション数は増えない。
- 自己修復のUPDATEは行ごとに生涯1回のみ発生する一時的なコスト。

## テスト方針

`src-tauri/src/store/note_cache.rs` に以下を追加する（既存のテストパターンに倣う）。

- `upsert_note` がpayloadの `user` をスタブ化して保存すること、`renote.user` も同様にスタブ化すること。
- `upsert_user` の `ON CONFLICT` で、`bio`/`banner_url`/`instance_*` は新しい値が `NULL` のとき既存値を保持し、`username` 等は常に上書きされること。
- 新形式（スタブ）payloadが `load_cached`/`get_note`/`search_cache` で正しくハイドレーションされ、`Note.user` が復元されること。
- 旧形式（フル `user` 埋め込み）payloadを直接INSERTしたテスト行が、読み込み1回で `user` テーブルへ抽出され、`payload` がスタブ形式へ書き戻されること（自己修復）。
- `user` テーブルに対応行が無い場合、そのノートが結果から除外され、ログ警告のみで例外にならないこと。
- renoteを持つノートで、renote元ユーザーの `instance` も正しくハイドレーションされること。

## 実データ検証

`docs/design/misskey-multicolumn-client-design.md` 等の既存検証と同様、実装後に手元の `cache.db`（48,470件）で以下を確認する。

- 移行前に `instance` が欠けていた古いノートで、Instance Tickerがhost名フォールバックではなく本来のTicker表示に戻ること。
- `load_cached` のレイテンシが移行前後で体感に出るほど劣化していないこと（簡易ベンチマーク）。

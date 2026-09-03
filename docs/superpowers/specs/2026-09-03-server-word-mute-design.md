# サーバ側ワードミュートの反映設計 (Issue #11)

## 背景

Issue #11「サーバー側のミュート設定が反映されていない」を調査した結果、以下が判明した:

- **サーバ側のユーザー/ブロックミュート**(`mute/list`・`blocking/list`)は既に実装済みで、起動時・アカウント追加時に同期され、ノート表示・通知フィルタに反映されている(`api/mutes.rs::fetch_muted_and_blocked` / `state.rs::server_mutes` / `commands/mute.rs::sync_server_mutes`)。
- 一方、Misskey本体の `/i`(`MeDetailed`)が持つ **`mutedWords`(ソフトワードミュート)** は、リポジトリ内のどのレイヤーにも一切実装されていない。ユーザーがMisskeyのWeb UI(設定 > ミュートとブロック > ワードミュート)で設定した内容が、tsumugi上では全く反映されない。
- `hardMutedWords` と `mutedInstances` はサーバー側の配信自体を絞る設定であり、既に実質的に効果が出ている可能性が高いため、今回のスコープからは除外する。

このドキュメントは `mutedWords`(ソフトワードミュート)をtsumugiのノート表示フィルタに反映する設計を記す。

## スコープ

- **対象**: `mutedWords` のみ。
- **非対象**: `hardMutedWords`、`mutedInstances`、通知へのワードミュート適用(既存のローカルNGワード `ng_words` も通知には適用されておらず、それと平仄を合わせる)。

## データモデル

### `WordMuteRule`(`filter/mute.rs` に追加)

```rust
pub enum WordMuteRule {
    /// 複数語のAND(1語のみのケースも含む)。大小無視の部分一致。
    Words(Vec<String>),
    /// /pattern/flags 形式の正規表現ルール。
    Regex(regex::Regex),
}
```

Misskeyの `mutedWords: (string | string[])[]` の各要素を以下のルールで変換する:

- 配列要素(`["foo","bar"]`) → `Words(vec!["foo","bar"])`。空文字列要素は除外し、全要素が空になったグループは無視する。
- 文字列要素が `/pattern/flags` 形式(先頭`/`、末尾に`/`+0個以上のflag文字、中身が空でない)にマッチ → `Regex`。`RegexBuilder` に `flags` のうち `i`(大小無視)だけを反映する。それ以外のflag(`g`/`m`等)はマッチ判定に影響しないため無視してよい。
- それ以外の文字列要素 → 単語1個のANDグループとして `Words(vec![s])`。
- 正規表現のコンパイルに失敗したルールは、そのルールだけスキップし `log::warn!` で警告する(壊れた1設定でミュート全体を無効化しない)。
- `mutedWords` フィールドが欠落/null/空配列なら `Vec::new()`(ミュートなし)。

### マッチ判定: `is_word_muted(text: Option<&str>, cw: Option<&str>, rules: &[WordMuteRule]) -> bool`

- `Words(ws)`: `hay = format!("{text} {cw}").to_lowercase()`(既存 `is_muted_one` の hay 生成と同じ)に対し、`ws` の全語(空語除く)が部分一致すれば true(AND)。
- `Regex(re)`: 小文字化しない生の `format!("{text} {cw}")` に対し `re.is_match(&hay)`(大小無視は `i` フラグで表現済みのため)。
- ルール同士はOR(いずれか1ルールが true ならミュート対象)。

### ノート単位の判定: `is_word_note_muted(note: &Note, rules: &[WordMuteRule]) -> bool`

既存 `is_muted` と同様に、ノート本体または renote 先のどちらかが該当すれば true。

## 取得・同期

- **`api/mutes.rs::fetch_muted_words(client: &MisskeyClient) -> Result<Vec<WordMuteRule>>`** を追加。
  - `client.post("i", &json!({}))` で生JSON(`serde_json::Value`)を取得し、`mutedWords` フィールドのみを取り出してパースする(`RawUser`/`MeDetailed` 型は作らない。`fetch_muted_and_blocked` が `mute/list` の生JSONから必要フィールドだけ拾っているのと同じスタイル)。
- **`state.rs`**:
  - `server_word_mutes: Mutex<HashMap<String, Vec<WordMuteRule>>>` を `server_mutes` と並べて追加。
  - `is_word_muted(&self, account_id: &str, note: &Note) -> bool` / `set_server_word_mutes(&self, account_id: &str, rules: Vec<WordMuteRule>)` を追加。
- **`commands/mute.rs::sync_server_mutes`** を拡張し、同じ呼び出しの中で `fetch_muted_words` も叩いて `state.set_server_word_mutes` に反映する。
  - 戻り値を `u32` から `SyncMuteResult { blocked_users: u32, word_rules: u32 }`(仮称、`specta::Type` 付与)に変更する。
  - フロント側 `store.svelte.ts::#syncServerMutes` のログ文言を件数内訳が分かるように更新する(例: `サーバのミュート/ブロックを同期: ユーザN件・ワードM件`)。
  - `/i` 取得に失敗した場合は関数全体がエラーになる(既存の `mute/list`/`blocking/list` 失敗時と同じ扱い。部分成功は扱わない)。

## 組み込み箇所

ローカルNGワード (`crate::filter::mute::is_muted`) と全く同じ5箇所に、`server_word_muted_note` 相当のチェックを追加する。通知フィルタ(`filter_notifications` 等)には適用しない。

| ファイル | 箇所 |
|---|---|
| `commands/column.rs` (~424行) | `fetch_backfill`(上スクロールによる過去ノート補完) |
| `commands/column.rs` (~899行) | `fill_gap`(再接続ギャップ埋め) |
| `commands/column.rs` (~1122行) | `search_cache_core`(検索モーダル。`is_server_muted: impl Fn(&Note) -> bool` と同様に `is_word_muted: impl Fn(&Note) -> bool` をクロージャ引数として追加) |
| `commands/column.rs` (~1209行) | `fetch_and_filter_multi`(REST初期/過去ページ取得) |
| `stream/connection.rs` (~753行) | WebSocketストリーミング受信 |

`stream/connection.rs` は `server_muted_note` 相当のロジック(`is_server_muted_note`)を `column.rs` とは別に重複定義しているが、既存の重複構造自体は本Issueのスコープ外として触らず、同じパターンで両ファイルにそれぞれ追加する。

各箇所での判定は「ノート本体 or renote先のいずれかが該当すれば非表示」で、既存の `server_muted_note`/`is_muted` と同じ意味論。

## テスト

- `filter/mute.rs`: `WordMuteRule` のパース(配列AND、単語、正規表現、不正正規表現のスキップ)と `is_word_muted`/`is_word_note_muted` のマッチング(AND内一部欠落で不一致、OR、renote先一致、大小無視、regexの `i` フラグ)を既存 `is_muted` 系テストと同スタイルで追加する。
- `commands/column.rs::search_cache_core`: 既存の `search_cache_core_excludes_locally_muted_notes` / `search_cache_core_excludes_notes_the_closure_marks_server_muted` に倣い、word-mute版のテストを追加する。
- `commands/mute.rs::sync_server_mutes`: 可能なら `fetch_muted_words` のパース部分は純粋関数として単体テスト可能にし、モックHTTPなしでカバーする。

## 非対象・既知の限界

- `hardMutedWords` / `mutedInstances` は未対応(サーバー側で既に配信が絞られている前提)。将来的にIssueが立てば別スペックとして扱う。
- 正規表現の `g`/`m` などのflagはマッチ判定に反映しない(`i` のみ対応)。
- ワードミュートは通知には適用しない(ローカルNGワードと挙動を揃える)。
- `resume_column`(初期ロード時のキャッシュ読み込み)はミュート設定を再検証しない — ローカルNGワード・サーバ側ユーザー/ブロックミュートと共通の既存動作で、このブランチが新規に導入したものではない。起動後に新しいノートを受信する経路(ストリーミング/バックフィル)から順次キャッシュが入れ替わるため実害は限定的だが、起動直後に古いキャッシュ内容が一瞬表示される可能性がある。将来別Issueとして対応を検討。

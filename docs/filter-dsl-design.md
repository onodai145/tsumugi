# Misskey Note フィルタDSL 設計書（TQL: Tsumugi Query Language）

KrileのKQL（Krile Query Language）を Misskey Note 向けに翻訳した、カラム定義用フィルタ言語の設計。
Rustバックエンドの Model層で「パーサ→AST→評価器（インメモリ / SQL射影の二段）」として実装する。

## 0. 設計原則（KQLからの継承と差分）
- **形は踏襲**：`from <sources> where <expression>`。1カラム＝1クエリ。
- **二段評価を維持**：各フィールドは「インメモリ評価関数」と「SQL射影文字列」の両方を持つ
  （Krile `GetEvaluator()` + `GetSqlQuery()`）。Streaming受信Noteはインメモリ評価、
  キャッシュ/過去検索はSQLで評価。
- **重要な差分**：
  1. **Note ID は文字列**（aid/aidx）。数値比較に使わない。`id` は String。
     時間での絞り込みは `created_at`(epoch秒, Numeric) を使う。
  2. **favs → reactions**：リアクションは絵文字。`reactions` は 合計数(Numeric) と
     絵文字名の集合(Set) の二面を持つ。
  3. 連合の概念：`host`（発信元インスタンス）、`local`/`remote` 判定を追加。
  4. 可視性・CW・投票・チャンネル・Renote/引用 を述語化。

## 1. 文法（EBNF）
KQLとほぼ同型。演算子もKQL互換（`->` = contains, `<-` = in）。

```
query    := "from" sources "where" expr
sources  := source ("," source)*
source   := IDENT | IDENT "(" STRING ("," STRING)* ")"
expr     := orExpr
orExpr   := andExpr ("||" andExpr)*
andExpr  := notExpr ("&&" notExpr)*
notExpr  := "!" notExpr | compare
compare  := add (compOp add)?
compOp   := "==" | "!=" | "<" | ">" | "<=" | ">="
          | "->" | "contains"        (* 左が集合/文字列に右を含む *)
          | "<-" | "in"              (* 左が右(集合)に含まれる *)
          | "startswith" | "endswith" | "match"   (* match = 正規表現 *)
add      := mul (("+"|"-") mul)*
mul      := unary (("*"|"/") unary)*
unary    := "(" expr ")" | value
value    := field | STRING | NUMBER | set | account
field    := IDENT ("." IDENT)*      (* 例: user.followers *)
set      := "[" (value ("," value)*)? "]"
account  := "@" IDENT               (* 自分のアカウント参照。KrileのUserSpecified相当 *)
```

- 型：`Boolean` / `Numeric` / `String` / `Set`。フィールドは文脈で複数型を取りうる
  （KQLのValueBaseと同じ多態）。
- Boolean述語は単独で `where renote` のように書ける（`== true` 省略可）。

## 2. ソース（from 節）
| DSL | 受信方法 | Krile対応 |
|---|---|---|
| `home` | homeTimeline channel | FilterHome |
| `local` | localTimeline channel | （新規） |
| `hybrid` / `social` | hybridTimeline channel | （新規） |
| `global` | globalTimeline channel | （新規） |
| `mentions` | main channel の mention / notes/mentions | FilterMentions |
| `list("<id>")` | userList channel | FilterList |
| `antenna("<id>")` | antenna channel | （新規） |
| `channel("<id>")` | channel タイムライン | （新規） |
| `user("@acct")` | users/notes | FilterUser |
| `tag("<hashtag>")` | hashtag タイムライン | FilterTrack近い |
| `search("<query>")` | notes/search（インスタンスで無効な場合あり） | FilterSearch |
| `cache` / `local_cache` | 受信せずローカルDBのみ検索 | FilterLocal |

複数ソースは KQL同様 OR 合成（いずれかで受信したNoteが対象）。

## 3. フィールド語彙（where 節）

### 3.1 Boolean 述語
| DSL | 意味 | Krile対応 |
|---|---|---|
| `renote` | 純粋なRenote（本文なし） | retweet |
| `quote` | 引用Renote（本文あり + renote先あり） | （新規） |
| `reply` | 返信（replyId あり） | （in_reply_to で代替されていた） |
| `has_files` / `has_media` | 添付ファイルあり | has_media |
| `has_poll` | 投票あり | （新規） |
| `cw` / `has_cw` | CW（内容警告）あり | （新規） |
| `sensitive` | センシティブ指定あり | （新規） |
| `local` | ローカルユーザ発（user.host なし） | （新規） |
| `remote` | リモートユーザ発 | （新規） |
| `bot` | 投稿者がbot | （新規） |
| `cat` | 投稿者がcat | （新規） |
| `direct` | 可視性 = direct(specified) | direct_message |
| `to_me` / `mentions_me` | 自分宛メンション | （user系で代替） |
| `reacted` | 自分がリアクション済み(myReaction あり) | favorited近い |
| `renoted` | 自分がRenote済み | retweeted近い |
| `favorited` | 自分がお気に入り済み（Misskeyのfavは別概念） | favorited |
| `mine` | 自分のアカウントいずれかの投稿 | @account / local(user) |
| `following` | 投稿者を自分がフォロー中（要context） | user.is_following相当 |
| `reply_to_me` | 自分の投稿への返信 | （新規） |
| `has_mention` | 誰かへのメンションを含む | （新規） |
| `has_link` / `url` | 本文にURLを含む | （新規） |
| `pinned` | ピン留めノート | （新規） |

### 3.2 Numeric 値
| DSL | 意味 | 備考 |
|---|---|---|
| `reactions` | リアクション合計数 | Set面も持つ（3.4） |
| `renotes` | Renote数 | favs/retweets相当 |
| `replies` | 返信数 | |
| `files` | 添付数 | |
| `length` | 本文長 | |
| `created_at` | 投稿時刻(epoch秒) | **時間比較はこれ**。id は使わない |
| `user.followers` | 投稿者フォロワー数 | |
| `user.following` | 投稿者フォロー数 | |
| `user.notes` | 投稿者ノート数 | |

### 3.3 String 値
| DSL | 意味 | Krile対応 |
|---|---|---|
| `text` | 本文（MFM原文 or 平文化） | text |
| `cw_text` | CW文言 | （新規） |
| `via` | クライアント名(あれば) | via |
| `host` | 投稿者インスタンスhost（ローカルは""） | （新規） |
| `visibility` | public/home/followers/specified | （新規） |
| `channel` | チャンネル名/ID | （新規） |
| `lang` | 言語（あれば） | （新規） |
| `reply_id` | 返信先NoteID | （新規、会話追跡用） |
| `renote_id` | Renote先NoteID | （新規） |
| `user.username` | @なしユーザ名 | user.screen_name |
| `user.acct` | @user@host 形式 | （新規） |
| `user.name` | 表示名 | user.name |
| `user.id` | ユーザID（文字列） | user.id |

### 3.4 Set 値（`->` / `<-` / `in` / `contains` 用）
| DSL | 意味 | Krile対応 |
|---|---|---|
| `reactions` | 付いている絵文字キーの集合 | favs(Set面) |
| `tags` / `hashtags` | ハッシュタグ集合 | （新規） |
| `mentions` / `to` | メンション先ユーザ集合 | to |
| `emojis` | 使用カスタム絵文字集合 | （新規） |
| `file_types` | 添付のMIMEカテゴリ集合（image/video/audio/...） | （新規） |

> **利用不可（Misskey側が公開しない語彙）**：`reacted_by` / `renoted_by`（誰がリアクション/Renoteしたか）。
> Misskeyのnoteオブジェクトはリアクションを**集計数のみ**で返し、ユーザ一覧は持たない（Krileのfavs/retweetsの
> ユーザSetに相当するものは無い）。よってこれらは語彙に**含めない**。

### 3.5 紛らわしい3概念の整理（重要）
名前が似ているが**別物**なので明確に分ける:
- `local`（**where節のBoolean述語**）… 投稿者がそのインスタンスの**ローカルユーザ**（`user.host` が無い）。⇔ `remote`。
- `local`（**from節のソース**）… localTimeline channel。上の述語とは**節が違うので別物**（位置で判別）。
- `cache`（from ソース）… 受信せず**ローカルSQLiteキャッシュ**を検索する（KrileのFilterLocal）。
- `mine`（Boolean述語）… **自分のアカウント**いずれかの投稿（要 EvalContext）。

#### リアクション絵文字キーの表記と正規化（確定）
Misskeyの `note.reactions` はキーが**混在**する:
- **Unicode絵文字**：生のUnicode文字そのまま（例 `"👍"`, `"❤️"`）。`:thumbsup:` にはならない。
- **カスタム絵文字**：`:name@host:` 形式。ローカルは `:name@.:`、リモートは `:blobcat@misskey.io:`。

DSLでの比較ルール:
- Unicode絵文字はそのまま文字リテラルで書く：`"👍" <- reactions`
- カスタム絵文字は `:name:` で書け、既定では**ホスト部を吸収してマッチ**（ローカル/リモート差を無視）：
  `":blobcat:" <- reactions`。ホストを厳密指定したい場合は `":blobcat@misskey.io:"` と書く。
- 実装は評価前に両辺をMisskeyキー形式へ正規化してから集合包含を判定する。

## 4. 例
```
# ホームで画像付き、CWなし、bot除外
from home where has_files && !cw && !bot

# ローカル＋ハイブリッドで、10リアクション以上ついた人気ノート
from local, hybrid where reactions >= 10

# 特定ユーザの、引用Renoteを除いた本文ノート
from user("@alice@example.com") where !renote && !quote

# 本文に "Rust" を含み、リモート発、直近のもの
from home, local where text -> "Rust" && remote

# 👍 か ❤ が付いたノート（リアクション絵文字集合に含まれる）
from home where "👍" <- reactions || "❤" <- reactions

# メンションで自分宛、未リアクション
from mentions where to_me && !reacted

# キャッシュ検索：過去ログから正規表現一致
from cache where text match "(?i)misskey|fediverse"
```

## 5. Rust実装の分割（Krileの構成に対応）
- `filter/token.rs` … Tokenizer（KQL Tokenizer.cs 相当。演算子 `-> <- && || match` 等）
- `filter/parser.rs` … Pratt/再帰下降で AST 構築（QueryCompiler.cs 相当）
- `filter/ast.rs` … Expr ノード定義（Boolean/Numeric/String/Set の型付き評価）
- `filter/eval.rs` … `fn evaluate(&Ast, &Note, ctx) -> bool`（インメモリ。GetEvaluator相当）
- `filter/sql.rs` … AST → SQLite WHERE句（GetSqlQuery相当。キャッシュ検索用）
- `filter/source.rs` … from節ソース → Receiver 起動（FilterSourceBase相当）
- `filter/context.rs` … 自分のアカウント/リアクション状態など評価に要る文脈（@account解決）

### 型付き評価の指針
- 各フィールドは「サポートする型集合」を宣言（KQLの SupportedTypes）。
- 比較演算子は左右の型を突き合わせて評価関数を選ぶ（数値比較/文字列比較/集合包含）。
- Boolean述語は単独出現で真偽を返す。
- 未対応の型組合せはパース時にエラー（KQLの型検査を踏襲）。

## 6. 決定事項 / 要検討

### 決定済み
- **リアクション絵文字の表記**：Unicodeは生文字、カスタムは `:name:`（既定でホスト吸収）。
  §3.4 の正規化ルールで統一。
- **`search` / `tag` がサーバで無効な場合**：既定で**ローカルSQLiteキャッシュ（`cache`ソース）へ
  フォールバック**し、UIに「サーバ検索非対応のためキャッシュ検索」と表示。
  （Misskeyは `notes/search` を無効化したインスタンスが多い。外部検索エンジン未設定・
  ロール制限・ローカルのみ制限など。Krileの `FilterLocal` と同じ発想。）
- **動的更新（再評価）の方針**：**「値は更新、出入りはしない」**。
  - `noteUpdated` イベント（`reacted`/`unreacted`/`pollVoted`/`deleted`）を受けて
    Noteの表示値（リアクション数等）は更新する。
  - フィルタ条件の合否が後から変わっても、**一度表示したNoteはカラムから消さない/追加もしない**
    （`deleted` のみ除去）。Krile寄りで予測しやすい挙動。
  - 購読は**カラムに載っている（見えている）Noteのみ** `sn`（subNote）購読しコストを抑える。

### 要検討（実装時に確定）
- `via`（クライアント名）はNoteに常に載るわけではない → 取得可否を確認。
- `cache` フォールバック時、キャッシュ未取得分は引けない旨のUI表現。
- （DBスキーマとSQL射影の対応表は §9・§10 で確定済み。テーブルのインデックス設計は実装時に。）

---

## 7. Rustデータ構造（ドメインモデル）
評価対象となる正規化済みNote。misskey-jsの生JSONではなく、Rust側でこの形に落としてから
Inbox→評価器へ流す。`serde` + `specta`(tauri-specta) で TS 型も自動生成する。

```rust
use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct Note {
    pub id: String,                 // aid/aidx。数値比較しない
    pub created_at: i64,            // epoch秒。時間比較はこれ
    pub text: Option<String>,      // MFM原文。純Renoteは None
    pub cw: Option<String>,        // 内容警告。None=CWなし
    pub visibility: Visibility,
    pub local_only: bool,          // 連合なし投稿
    pub user: User,
    pub reply_id: Option<String>,
    pub renote_id: Option<String>,
    pub renote: Option<Box<Note>>, // 引用/Renote先（浅く保持）
    pub files: Vec<DriveFile>,
    pub poll: Option<Poll>,
    pub tags: Vec<String>,          // ハッシュタグ
    pub mentions: Vec<String>,      // メンション先 userId
    pub emojis: Vec<String>,        // 本文で使うカスタム絵文字名
    pub channel_id: Option<String>,
    pub via: Option<String>,        // クライアント名（取得可否は要確認）
    pub lang: Option<String>,

    // 集計・自分の状態（noteUpdatedで更新される可変部）
    pub reactions: HashMap<String, u32>, // キー=Misskey形式（Unicode生 or :name@host:）
    pub reaction_count: u32,
    pub renote_count: u32,
    pub reply_count: u32,
    pub my_reaction: Option<String>,     // 自分が付けた絵文字。None=未リアクション
    pub is_renoted_by_me: bool,
    pub is_favorited_by_me: bool,
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Visibility { Public, Home, Followers, Specified } // Specified = direct

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct User {
    pub id: String,
    pub username: String,          // @なし
    pub host: Option<String>,      // None=ローカル
    pub name: Option<String>,      // 表示名
    pub is_bot: bool,
    pub is_cat: bool,
    pub followers_count: u32,
    pub following_count: u32,
    pub notes_count: u32,
}

impl User {
    pub fn acct(&self) -> String {   // "@user" or "@user@host"
        match &self.host {
            Some(h) => format!("@{}@{}", self.username, h),
            None => format!("@{}", self.username),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct DriveFile {
    pub id: String,
    pub mime_type: String,         // "image/png" 等。file_types はここから category 化
    pub is_sensitive: bool,
    pub url: String,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct Poll {
    pub choices: Vec<PollChoice>,
    pub multiple: bool,
    pub expires_at: Option<i64>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct PollChoice { pub text: String, pub votes: u32, pub is_voted: bool }
```

### 評価コンテキスト（`mine`/`following`/`to_me` 等の解決に必要）
```rust
pub struct EvalContext {
    pub my_user_ids: std::collections::HashSet<String>, // 全ログインアカウントのuserId
    pub following_ids: Option<std::collections::HashSet<String>>, // フォロー中(取得済みなら)
    pub local_host: Option<String>, // 受信アカウントのインスタンスhost（host比較用）
}
```

## 8. AST / ソース / フィールドの型定義
KQLの ValueBase(多態) を Rust の enum で表現。パース時に型検査する。

```rust
#[derive(Debug, Clone, PartialEq, specta::Type, serde::Serialize, serde::Deserialize)]
pub enum FilterType { Boolean, Numeric, String, Set }

/// where 節の式木（Bool を返す木）
pub enum Expr {
    Or(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Compare { lhs: Value, op: CompareOp, rhs: Value },
    Bare(Value),                 // Boolean述語の単独出現 (例: `renote`)
}

pub enum CompareOp {
    Eq, Ne, Lt, Gt, Le, Ge,
    Contains,   // -> : 左(集合/文字列)が右を含む
    In,         // <- : 左が右(集合)に含まれる
    StartsWith, EndsWith, Match, // Match=正規表現
}
pub enum ArithOp { Add, Sub, Mul, Div }

/// 値。算術は Value 側に持たせる（Compare の左右辺に算術結果を置けるようにするため。
/// 例: `reactions + renotes > 10` の左辺は Value::Arith）。
pub enum Value {
    Field(Field),
    Str(String),
    Num(f64),
    Set(Vec<Value>),
    Account(String),   // @name : 自分のアカウント参照
    Arith { lhs: Box<Value>, op: ArithOp, rhs: Box<Value> }, // Numeric のみ（型検査で担保）
}

/// フィールド。各バリアントが「対応する型集合」と評価/SQL射影を持つ。
pub enum Field {
    // Boolean
    Renote, Quote, Reply, HasFiles, HasPoll, Cw, Sensitive, Local, Remote,
    Bot, Cat, Direct, ToMe, ReplyToMe, HasMention, HasLink, Pinned,
    Reacted, Renoted, Favorited, Mine, Following,
    // Numeric
    Reactions, Renotes, Replies, Files, Length, CreatedAt,
    UserFollowers, UserFollowing, UserNotes,
    // String
    Text, CwText, Via, Host, VisibilityStr, Channel, Lang, ReplyId, RenoteId,
    UserUsername, UserAcct, UserName, UserId,
    // Set  (Reactions は Numeric と Set の両面を持つ点に注意)
    Tags, Mentions, Emojis, FileTypes,
}

/// from 節のソース
pub enum Source {
    Home, Local, Hybrid, Global, Mentions,
    List(String), Antenna(String), Channel(String),
    User(String),      // @acct
    Tag(String),
    Search(String),
    Cache,
}

pub struct Query { pub sources: Vec<Source>, pub predicate: Option<Expr> }
```

### 型検査の指針（Krile踏襲）
- 各 `Field` は `supported_types() -> &[FilterType]` を返す（例：`Reactions` は `[Numeric, Set]`）。
- `Compare` は演算子と両辺の型から評価関数を決定：
  数値比較 / 文字列比較（`==`は完全一致, `Contains`は部分一致）/ 集合包含（`In`/`Contains`）/ 正規表現(`Match`)。
- `Bare(Value)` は Value が Boolean を返せる場合のみ許可。
- 型不一致・未対応の組合せは**パース時にエラー**（実行時に落とさない）。
- `reactions` の Set 面は §3.4 の正規化を通してから包含判定。

---

## 9. ローカルDBスキーマ（SQL射影の前提）
Krile Casket の二段構え（DB永続化＋インメモリ評価）を踏襲。SQL射影はこのスキーマを前提にする。
Set系フィールドは正規化テーブルへ分割し、相関サブクエリで参照（KrileのFavorites/StatusEntity方式）。

```sql
CREATE TABLE note (
  id            TEXT PRIMARY KEY,
  created_at    INTEGER NOT NULL,     -- epoch秒
  text          TEXT,
  text_length   INTEGER NOT NULL DEFAULT 0,  -- 挿入時に算出
  cw            TEXT,
  visibility    TEXT NOT NULL,        -- 'public'|'home'|'followers'|'specified'
  local_only    INTEGER NOT NULL DEFAULT 0,
  user_id       TEXT NOT NULL,
  reply_id      TEXT,
  reply_user_id TEXT,                 -- reply_to_me 用に投稿時展開して保存
  renote_id     TEXT,
  channel_id    TEXT,
  via           TEXT,
  lang          TEXT,
  files_count   INTEGER NOT NULL DEFAULT 0,
  has_poll      INTEGER NOT NULL DEFAULT 0,
  has_link      INTEGER NOT NULL DEFAULT 0,  -- 挿入時にURL検出して算出
  is_pinned     INTEGER NOT NULL DEFAULT 0,
  -- 可変集計部（noteUpdatedで更新）
  reaction_count    INTEGER NOT NULL DEFAULT 0,
  renote_count      INTEGER NOT NULL DEFAULT 0,
  reply_count       INTEGER NOT NULL DEFAULT 0,
  my_reaction       TEXT,
  is_renoted_by_me  INTEGER NOT NULL DEFAULT 0,
  is_favorited_by_me INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE user (
  id TEXT PRIMARY KEY, username TEXT NOT NULL, host TEXT, name TEXT,
  is_bot INTEGER NOT NULL DEFAULT 0, is_cat INTEGER NOT NULL DEFAULT 0,
  followers_count INTEGER NOT NULL DEFAULT 0,
  following_count INTEGER NOT NULL DEFAULT 0,
  notes_count     INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE note_reaction (note_id TEXT, emoji_key TEXT, count INTEGER); -- キー=Misskey形式
CREATE TABLE note_tag      (note_id TEXT, tag TEXT);
CREATE TABLE note_mention  (note_id TEXT, user_id TEXT);
CREATE TABLE note_emoji    (note_id TEXT, emoji TEXT);
CREATE TABLE note_file     (note_id TEXT, mime_type TEXT, mime_category TEXT, is_sensitive INTEGER);
```
- クエリは `note n JOIN user u ON n.user_id = u.id` を基本とする。
- `mine` / `following` / `to_me` など EvalContext 依存の値は**バインド変数**（`:my_ids` 等）で注入。
- `match`（正規表現）は rusqlite に **REGEXP 関数を登録**して `col REGEXP :pat` で使う。

## 10. Field × 評価/SQL射影 対応表
凡例：`n`=&Note, `ctx`=&EvalContext（インメモリ）／`n`,`u`=note,user テーブル別名（SQL）。

### 10.1 Boolean
| Field | インメモリ評価 | SQL射影 |
|---|---|---|
| `renote` | `n.renote_id.is_some() && n.text.is_none()` | `(n.renote_id IS NOT NULL AND n.text IS NULL)` |
| `quote` | `n.renote_id.is_some() && n.text.is_some()` | `(n.renote_id IS NOT NULL AND n.text IS NOT NULL)` |
| `reply` | `n.reply_id.is_some()` | `n.reply_id IS NOT NULL` |
| `has_files` | `!n.files.is_empty()` | `n.files_count > 0` |
| `has_poll` | `n.poll.is_some()` | `n.has_poll = 1` |
| `cw` | `n.cw.is_some()` | `n.cw IS NOT NULL` |
| `sensitive` | `n.files.iter().any(\|f\| f.is_sensitive)` | `EXISTS(SELECT 1 FROM note_file f WHERE f.note_id=n.id AND f.is_sensitive=1)` |
| `local` | `n.user.host.is_none()` | `u.host IS NULL` |
| `remote` | `n.user.host.is_some()` | `u.host IS NOT NULL` |
| `bot` | `n.user.is_bot` | `u.is_bot = 1` |
| `cat` | `n.user.is_cat` | `u.is_cat = 1` |
| `direct` | `n.visibility == Visibility::Specified` | `n.visibility = 'specified'` |
| `to_me` | `n.mentions.iter().any(\|m\| ctx.my_user_ids.contains(m))` | `EXISTS(SELECT 1 FROM note_mention m WHERE m.note_id=n.id AND m.user_id IN (:my_ids))` |
| `reply_to_me` | `n.reply_user_id.as_ref().map_or(false,\|u\| ctx.my_user_ids.contains(u))` | `n.reply_user_id IN (:my_ids)` |
| `has_mention` | `!n.mentions.is_empty()` | `EXISTS(SELECT 1 FROM note_mention m WHERE m.note_id=n.id)` |
| `has_link` | `n.text.as_deref().map_or(false, has_url)` | `n.has_link = 1` |
| `pinned` | `n.is_pinned` | `n.is_pinned = 1` |
| `reacted` | `n.my_reaction.is_some()` | `n.my_reaction IS NOT NULL` |
| `renoted` | `n.is_renoted_by_me` | `n.is_renoted_by_me = 1` |
| `favorited` | `n.is_favorited_by_me` | `n.is_favorited_by_me = 1` |
| `mine` | `ctx.my_user_ids.contains(&n.user.id)` | `n.user_id IN (:my_ids)` |
| `following` | `ctx.following_ids.as_ref().map_or(false,\|s\| s.contains(&n.user.id))` | `n.user_id IN (:following_ids)` ※未取得ならContradiction |

### 10.2 Numeric
※ インメモリ列は最終的に `Value::Num(f64)` へ変換する（各式に `as f64` を補う）。
| Field | インメモリ | SQL |
|---|---|---|
| `reactions` | `n.reaction_count as f64` | `n.reaction_count` |
| `renotes` | `n.renote_count` | `n.renote_count` |
| `replies` | `n.reply_count` | `n.reply_count` |
| `files` | `n.files.len()` | `n.files_count` |
| `length` | `n.text.as_deref().unwrap_or("").chars().count()` | `n.text_length` |
| `created_at` | `n.created_at` | `n.created_at` |
| `user.followers` | `n.user.followers_count` | `u.followers_count` |
| `user.following` | `n.user.following_count` | `u.following_count` |
| `user.notes` | `n.user.notes_count` | `u.notes_count` |

### 10.3 String
| Field | インメモリ | SQL |
|---|---|---|
| `text` | `n.text.clone().unwrap_or_default()` | `COALESCE(n.text,'')` |
| `cw_text` | `n.cw.clone().unwrap_or_default()` | `COALESCE(n.cw,'')` |
| `via` | `n.via.clone().unwrap_or_default()` | `COALESCE(n.via,'')` |
| `host` | `n.user.host.clone().unwrap_or_default()` | `COALESCE(u.host,'')` |
| `visibility` | `n.visibility.as_str()` | `n.visibility` |
| `channel` | `n.channel_id.clone().unwrap_or_default()` | `COALESCE(n.channel_id,'')` |
| `lang` | `n.lang.clone().unwrap_or_default()` | `COALESCE(n.lang,'')` |
| `reply_id` | `n.reply_id.clone().unwrap_or_default()` | `COALESCE(n.reply_id,'')` |
| `renote_id` | `n.renote_id.clone().unwrap_or_default()` | `COALESCE(n.renote_id,'')` |
| `user.username` | `n.user.username.clone()` | `u.username` |
| `user.acct` | `n.user.acct()` | `('@'\|\|u.username\|\|CASE WHEN u.host IS NULL THEN '' ELSE '@'\|\|u.host END)` |
| `user.name` | `n.user.name.clone().unwrap_or_default()` | `COALESCE(u.name,'')` |
| `user.id` | `n.user.id.clone()` | `u.id` |

### 10.4 Set（`->` / `<-` / `in` / `contains` の対象）
| Field | インメモリ（→ 正規化済み集合） | SQL（相関サブクエリ） |
|---|---|---|
| `reactions` | `n.reactions.keys()` を正規化 | `(SELECT emoji_key FROM note_reaction r WHERE r.note_id=n.id)` |
| `tags` | `n.tags` | `(SELECT tag FROM note_tag t WHERE t.note_id=n.id)` |
| `mentions` / `to` | `n.mentions` | `(SELECT user_id FROM note_mention m WHERE m.note_id=n.id)` |
| `emojis` | `n.emojis` | `(SELECT emoji FROM note_emoji e WHERE e.note_id=n.id)` |
| `file_types` | `n.files.iter().map(mime_category)` | `(SELECT mime_category FROM note_file f WHERE f.note_id=n.id)` |

## 11. 演算子 → SQL 対応
| DSL演算子 | 左辺型 | SQL |
|---|---|---|
| `==` / `!=` | 数値/文字列 | `lhs = rhs` / `lhs <> rhs` |
| `<` `>` `<=` `>=` | 数値 | 同記号 |
| `startswith` | 文字列 | `lhs LIKE rhs \|\| '%'` |
| `endswith` | 文字列 | `lhs LIKE '%' \|\| rhs` |
| `contains` / `->` | 文字列 | `lhs LIKE '%' \|\| rhs \|\| '%'` |
| `contains` / `->` | 集合 | `rhs IN (集合サブクエリ)` |
| `in` / `<-` | 値∈集合 | `lhs IN (集合サブクエリ)` |
| `match` | 文字列 | `lhs REGEXP rhs`（REGEXP関数を登録） |

- LIKE の右辺はエスケープしてワイルドカード誤爆を防ぐ（`%` `_` を実文字扱いにしたい場合）。
- 集合同士の比較（両辺Set）は初期は非対応でよい（KQLも実質スカラ×集合が中心）。
- `reactions`/`emojis` の集合比較は §3.4 正規化を通した値でバインドする。

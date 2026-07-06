# Misskey マルチカラムクライアント 設計書

- 作成日: 2026-07-05
- 想定名称: （仮）`Kohitsuji` ※後で決定
- 参照元: Twitterクライアント「Krile / TweenParty」のUX

---

## 1. 目的・コンセプト

Krileが提供していた「カラム単位で複数のタイムライン・複数アカウントを同時監視する」体験を、Misskeyのストリーミング/REST APIを使って再現する。単一MisskeyフォークのWeb UI（CherryPickのマルチカラム機能等）ではなく、**独立したネイティブデスクトップアプリ**として実装する。

## 2. スコープ

### 対象OS
Windows / macOS / Linux のクロスプラットフォーム対応。開発機はArch Linux + Hyprlandだが、配布物は3OSで動作させる。Tauriを採用する時点でWebView依存OS差異（Linuxは`webkit2gtk`、Windowsは`WebView2`、macOSは`WKWebView`）が生じるため、後述の「未確定事項」でOS別の検証範囲を明記する。

### ドキュメント構成
本書は以下の二段構成。

1. **第I部: 全体アーキテクチャ** — 恒久的に変わらない骨格（レイヤー構成、通信方式、データモデルの考え方）
2. **第II部: MVP詳細設計** — 最初にビルドする範囲の具体仕様

Krileのフル機能（テーマ配布、プラグイン、詳細なキーボードショートカット体系など）は本書ではPhase2以降の「将来拡張」として一覧化のみ行う。

---

# 第I部: 全体アーキテクチャ

## 3. 技術スタック

| 領域 | 選定 | 理由 |
|---|---|---|
| アプリフレームワーク | Tauri v2 | Rust/Tauriは既存スタックと一致。クロスプラットフォーム、軽量 |
| Misskey APIバインディング | **OpenAPI定義から自動生成したRust型 + 接続ロジックは自前実装** | MisskeyはOpenAPI仕様（`/api-doc.json`）を公開しているため、リクエスト/レスポンスの型定義はこれをもとに`progenitor`や`openapi-generator`等でRustコードとして生成する。一方、WebSocketの接続確立・チャンネル多重化・再接続・認証といった**接続ロジックは自前実装**とし、外部crate（`misskey-rs`等）にもmisskey-js（JS/TS）にも依存しない。これによりトークンをRustプロセス内に閉じ込めたまま実装できる（misskey-js案で懸念したWebViewへのトークン露出を回避） |
| バックエンドロジック | Rust | ストリーム接続管理・再接続・REST呼び出し・フィルタ処理を担い、トークンはRustプロセス内から出さない |
| フロントエンド | **Svelte**（+ Vite） | 下記§3.1で理由を詳述 |
| Rust→TS型・コマンド生成 | `tauri-specta` + `specta` | RustのドメインStruct（Account/Note/Column等）にderiveを付けてTS型を自動生成し、Tauriのcommand/eventシグネチャも同時に生成する。フロントとバックエンドで型を二重管理しない |
| MFM描画 | `mfm-js`（フロント側） | Misskeyの投稿記法（MFM）をパースしてASTからDOMへ描画する。パースはmfm-js、描画（メンション/ハッシュタグ/カスタム絵文字/リンク/CW折り畳み）はSvelteコンポーネント側で行う |
| ローカルDB | SQLite（`rusqlite` or `sqlx`） | カラム定義・アカウントメタデータ・既読位置に加えて、**受信ノートの永続キャッシュ**も保持する（§5で詳述） |
| 認証情報保管 | `keyring` crate | OS標準のSecret機構（Windows Credential Manager / macOS Keychain / Linux Secret Service）にアクセストークンを保存。**トークンはRust Core内でのみ扱い、フロントには一切渡さない** |
| 通知 | Tauri notification plugin + カラム別サウンド再生（`rodio` crateなど） | デスクトップ通知とKrile的なサウンド通知の両方に対応。ストリーム側（Rust）で新着を検知してフロントへemitし、フロント側で表示と合わせて発火する |

### 3.1 フロントエンドフレームワークの選定理由（Svelteに更新）

Krileクローンの負荷特性は「多数の独立したカラムが、それぞれ高頻度・非同期にWebSocketイベントを受け取り、末尾に1行追加する」という形になる。これはReactのVDOM diffだと更新のたびに関係ないカラムまで再評価コストが乗りやすい一方、Svelteはコンパイル時に「どのDOMノードをどう更新するか」を確定させ、ランタイムでの差分計算そのものを行わないため、この「多カラム・高頻度・局所更新」というワークロードに構造的に向いている。

- 当初はSolid.js（fine-grained reactivity）を候補に挙げていたが、Krileの設計思想を踏まえたロードマップ側の技術選定に合わせてSvelteに変更した。両者の負荷特性上の優位性はほぼ同等（VDOM diffを避けるという点で共通）で、Svelteの方がテンプレート構文がシンプルでエコシステムも広い
- Next.jsの経験（JSX）とは書き味が変わるが、テンプレート構文自体はVueに近く学習コストは大きくない
- `tauri-specta`によるRust→TS型生成、`mfm-js`によるMFM描画など周辺ツールはフレームワーク非依存なので、Svelteへの変更によるアーキテクチャ全体への影響は§4以降の構成には及ばない

## 4. レイヤー構成

```
┌─────────────────────────────────────────┐
│ Frontend (Svelte, Vite, WebView)         │
│  - カラムUI / 設定UI / 通知UI            │
│  - Tauri invoke() でコマンド呼び出し     │
│  - Tauri event listen() でストリーム受信 │
│  - MFM描画（mfm-js）                     │
│  - トークンには一切触れない              │
└───────────────▲──────────────┬───────────┘
                 │ invoke       │ event（tauri-specta生成型）
┌───────────────┴──────────────▼───────────┐
│ Tauri Core (Rust)                         │
│ ┌─────────────────────────────────────┐ │
│ │ Command Handlers (invoke対象)        │ │
│ │  - add_column / remove_column        │ │
│ │  - add_account / remove_account      │ │
│ │  - post_note / react / renote 等     │ │
│ └─────────────────────────────────────┘ │
│ ┌─────────────────────────────────────┐ │
│ │ Connection Manager（自前実装）        │ │
│ │  - Account毎に1 WebSocket接続を保持   │ │
│ │  - チャンネル購読(id)のライフサイクル │ │
│ │  - 再接続(exponential backoff)       │ │
│ └─────────────────────────────────────┘ │
│ ┌─────────────────────────────────────┐ │
│ │ Inbox → Broadcaster                  │ │
│ │  - 受信メッセージをNote型へ正規化      │ │
│ │  - NoteIDで重複排除                   │ │
│ │  - 各カラムの評価器へファンアウト      │ │
│ └─────────────────────────────────────┘ │
│ ┌─────────────────────────────────────┐ │
│ │ Generated API Types                  │ │
│ │  - OpenAPI定義からコード生成         │ │
│ │  - REST/Streamingのペイロード型      │ │
│ └─────────────────────────────────────┘ │
│ ┌─────────────────────────────────────┐ │
│ │ Filter Evaluator                     │ │
│ │  - MVP: 部分一致キーワードフィルタ    │ │
│ │  - 将来: TQL（§5.1参照）             │ │
│ │  - 通知トリガー判定                  │ │
│ └─────────────────────────────────────┘ │
│ ┌─────────────────────────────────────┐ │
│ │ Persistence (SQLite / keyring)       │ │
│ │  - 設定（Account/Column）             │ │
│ │  - ノート永続キャッシュ（§5参照）      │ │
│ │  - トークンはこの層から外に出さない  │ │
│ └─────────────────────────────────────┘ │
└────────────────────────────────────────┘
                 │
                 ▼
      複数Misskeyインスタンス
      （WebSocket + REST）
```

設計原則: **WebSocket接続・チャンネル多重化・REST呼び出し・トークンの保持は全てRust Core側に閉じ込め、フロントは「カラムID→イベント」の単純な購読者に留める。** misskey-jsのようなJS製SDKをフロントで使う案も検討したが、その場合トークンをWebViewのJS実行コンテキストに展開する必要が生じるため、セキュリティ上のトレードオフとして採用しないことにした（前回検討時の記録）。REST/エンティティの型定義自体はMisskeyのOpenAPI仕様から自動生成し、手書きの型メンテナンスコストを下げつつ、接続ロジックは自前実装することで外部crateのメンテナンス状況に依存しない構成にする。Rust→フロント間の型は`tauri-specta`で自動生成し、二重管理を避ける。

## 5. データモデル（概念設計）

`Account`/`Column`はRust側でSQLiteに永続化する設定データ。`token`本体はkeyringに保管し、**Connection Manager（Rust内）のみがkeyringから読み出して使用する。フロントへtokenを渡すinvokeコマンドは設けない。**

```rust
struct Account {
    id: Uuid,
    host: String,           // e.g. "misskey.io"
    display_name: String,
    avatar_url: Option<String>,
    // token本体はkeyringに保管し、DBにはkeyring参照キーのみ
}

struct Column {
    id: Uuid,
    account_id: Uuid,
    kind: ColumnKind,
    order: i32,
    width: i32,
    filter_keywords: Vec<String>, // MVP: 部分一致のみ。将来はTQLのQuery ASTに置き換える（§5.1）
    notify_sound: bool,
    notify_desktop: bool,
}

enum ColumnKind {
    Home,
    Local,
    Global,
    Hybrid,
    Notifications,
    List { list_id: String },
    Antenna { antenna_id: String },
    Search { query: String },
}
```

### 5.1 ノートのドメインモデルと永続キャッシュ（方針転換）

**当初はノートをSQLiteに永続キャッシュせず、メモリ内リングバッファのみとする方針だったが、Krileの`Casket`設計とTQLフィルタDSL（`cache`ソース、過去ログ検索）を採用するため、方針を転換し、受信ノートをSQLiteに永続キャッシュする。** インメモリ評価（Streaming受信時）とSQL評価（キャッシュ検索時）の二段構えとする。

misskey-jsの生JSON（またはOpenAPI生成型の生レスポンス）をそのまま使わず、Rust側で以下の正規化済みドメイン型に変換してからInbox→評価器へ流す。`specta`アノテーションを付け、`tauri-specta`でTS型も同時生成する。

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct Note {
    pub id: String,                 // aid/aidx。数値比較しない
    pub created_at: i64,            // epoch秒
    pub text: Option<String>,       // MFM原文。純RenoteはNone
    pub cw: Option<String>,
    pub visibility: Visibility,
    pub local_only: bool,
    pub user: User,
    pub reply_id: Option<String>,
    pub renote_id: Option<String>,
    pub renote: Option<Box<Note>>,
    pub files: Vec<DriveFile>,
    pub poll: Option<Poll>,
    pub tags: Vec<String>,
    pub mentions: Vec<String>,
    pub emojis: Vec<String>,
    pub channel_id: Option<String>,
    pub via: Option<String>,
    pub lang: Option<String>,
    // 集計・自分の状態（noteUpdatedイベントで更新される可変部）
    pub reactions: std::collections::HashMap<String, u32>, // キー=Misskey形式
    pub reaction_count: u32,
    pub renote_count: u32,
    pub reply_count: u32,
    pub my_reaction: Option<String>,
    pub is_renoted_by_me: bool,
    pub is_favorited_by_me: bool,
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Visibility { Public, Home, Followers, Specified }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct User {
    pub id: String,
    pub username: String,
    pub host: Option<String>,       // None=ローカル
    pub name: Option<String>,
    pub is_bot: bool,
    pub is_cat: bool,
    pub followers_count: u32,
    pub following_count: u32,
    pub notes_count: u32,
}
```

`DriveFile`/`Poll`等は省略（詳細は別紙`filter-dsl-design.md`§7参照）。この型定義は、フィルタDSL（TQL、§5.2）の評価対象そのものでもあるため、フィルタ側の要求語彙（リアクション集合、CW有無、visibility等）を過不足なく含む形に揃えている。

**注意（データの制約）**: Misskeyの`note.reactions`は絵文字キーごとの**集計数のみ**を返し、「誰がリアクションしたか」のユーザー一覧は持たない。Twitter/Krileの`favs`/`retweets`のようなユーザーSetに相当するものは存在しないため、「誰が」を条件にしたフィルタや表示は実現できない。

### 5.2 フィルタDSL（TQL）の位置づけ

Krileの「Krile Query（KQL）」をMisskey向けに翻訳したフィルタDSL「TQL（Tsumugi Query Language）」を、`filter-dsl-design.md`として別途設計済み。`from home where has_files && !cw && !bot`のような構文で、カラムの受信ソースと絞り込み条件を1つのクエリとして表現する。

- 各フィールドは「インメモリ評価関数」と「SQL射影文字列」の両方を持ち、Streaming受信時はインメモリで、キャッシュ検索時はSQLiteに対するクエリで評価する（二段評価）
- Rust側は`filter/token.rs`（字句解析）→`filter/parser.rs`（構文解析・型検査）→`filter/ast.rs`（AST）→`filter/eval.rs`（インメモリ評価）→`filter/sql.rs`（SQL射影）の分割構成
- **MVPのスコープには含めない。** MVPでは§8.2の通り部分一致キーワードフィルタのみとし、TQLは§13の将来拡張（Phase4相当）で本格導入する。ただしノートの永続キャッシュ（本節）はTQL導入の前提になるため、MVPの時点でSQLiteスキーマ自体は用意しておく

### 5.3 ローカルDBスキーマ（ノートキャッシュ）

`filter-dsl-design.md`§9で確定済みのスキーマをそのまま採用する。`note`テーブルを中心に、`user`テーブルと結合し、Set系フィールド（リアクション・タグ・メンション・絵文字・添付ファイル種別）は正規化した別テーブル（`note_reaction`/`note_tag`/`note_mention`/`note_emoji`/`note_file`）に分割する。既読位置（`lastNoteId`相当）もこのDBに保持し、再起動時に「前回表示位置」を復元する。

キャッシュの保持上限（何件/何日分残すか）とGC方針は§12の未確定事項として別途検討する。

## 6. ストリーミング接続設計（Rust自前実装）

Misskeyのストリーミング接続は「1 WebSocket接続の中で複数チャンネルをidで多重化する」設計になっている。この設計自体はマルチカラムクライアントに都合が良いが、以下の点は明示的に自前実装で担保する必要がある。

- **チャンネル接続idは「チャンネルの接続ごと」に発行する。** 同一チャンネルに異なるパラメータで複数接続するケースがあるため、カラムを追加するたびに新規UUIDを発行し、`type: connect` で送信する。接続断→再接続時もUUIDを再発行する。
- **リアクション等の反映にはノート単位のキャプチャ購読が別途必要。** タイムライン購読だけでは、表示済みノートへの後からのリアクション付与を検知できない。表示領域に入ったノートを動的に「キャプチャ登録」し、スクロールで表示領域外に出たら解除する仕組みをConnection Manager側に持つ。
- **切断は前提として発生する。** 放置すると確実に切断されるため、ping/pongまたは定期的な`api`呼び出しでの生存確認＋バックオフ付き再接続をAccount単位で実装する。再接続時は該当Accountに紐づく全カラムのチャンネル購読を再送する。
- **1本のWebSocket接続を複数カラムで共有する。** 同一アカウントの複数カラムが同じチャンネル種別（例: 複数のHomeカラム）を購読するケースもあるため、受信したメッセージはInboxで一旦Note型に正規化し、NoteIDで重複排除した上でBroadcasterが該当する各カラムの評価器へファンアウトする（§4のInbox→Broadcasterに対応）。

これらはMisskey公式ドキュメントおよびコミュニティの実装記（Misskeyクライアント制作記、Telnetクライアント開発記）から確認した挙動であり、公式APIリファレンスに明記されていない部分（キャプチャの詳細な解除タイミング等）もあるため、実装時に実機で挙動確認が必要（§12参照）。

### 6.1 OpenAPI型生成の位置づけ

MisskeyはOpenAPI仕様（`/api-doc.json`）を公開している。これをもとに以下のようなビルドフローを想定する。

1. 対象Misskeyバージョンの`api-doc.json`をリポジトリ内にスナップショットとして保存（サーバー起動時に毎回取得はしない。バージョン差異を意図せず取り込まないため）
2. **`progenitor`（Oxide Computer製）を採用し、`build.rs`経由でコード生成する。** Rust純正で継続的にメンテナンスされており、OpenAPI 3.0.x準拠のドキュメントを主要ターゲットとしている点、型だけでなく非同期reqwestベースのクライアントも同時に生成される点が決め手。これにより単純なREST呼び出し（投稿・リアクション・リノート等）は生成されたクライアントをそのまま利用し、自前実装が必要な範囲をStreaming部分に絞り込める。ただしprogenitorは元々Dropshot（Oxide自身のフレームワーク）が出力するOpenAPIドキュメントを主要ターゲットとして設計されているため、Misskey側の生成系に起因する非対応パターンが出る可能性はある。これは実際に`api-doc.json`を投入して検証する（§12）
3. 生成された型・クライアントをConnection Manager・REST呼び出し部分の両方から利用する

**ストリーミングAPI（WebSocketメッセージのやり取り）はOpenAPI仕様の対象外**のため、こちらは公式ドキュメント・実装記をもとに型を手書きする必要がある。この手書き部分の量が実装コストの中心になる見込み。

---

# 第II部: MVP詳細設計

## 7. MVPのゴール

「複数アカウント・複数カラムでタイムラインをリアルタイム監視し、投稿・リノート・リアクションができる」最小構成。テーマ、プラグイン、詳細なキーボードショートカットは含めない。

## 8. MVP機能一覧

### 8.1 アカウント管理
- アカウント追加（MiAuthフローでのトークン取得、`keyring`への保存）
- アカウント削除
- 複数アカウント同時ログイン状態の一覧表示

### 8.2 カラム管理
- カラム種別: Home / Local / Global / Hybrid / Notifications / List / Search（Antennaは将来拡張に回す候補）
- カラムの追加・削除・並べ替え（ドラッグ&ドロップ）・幅調整
- カラムごとのキーワードフィルタ（正規表現は将来拡張、MVPは部分一致のみ）

### 8.3 タイムライン表示・操作
- ノート表示（MFM本文を`mfm-js`でパースしSvelteコンポーネントで描画。メンション/ハッシュタグ/カスタム絵文字/リンク/CW折り畳みに対応。添付メディアのサムネイル表示）
- リアクション（絵文字ピッカーは簡易版：よく使う数個+検索）
- リノート / 引用RN / 返信 / 投稿
- 投稿フォーム（可視性選択: public/home/followers/specified）

### 8.4 通知
- カラムごとの通知音ON/OFF
- OSネイティブのデスクトップ通知（Notificationsカラムに新着があった場合）

## 9. MVPのIPCコマンド一覧（案）

| コマンド | 用途 |
|---|---|
| `add_account(host, token)` | アカウント登録 |
| `list_accounts()` | 登録済みアカウント一覧 |
| `remove_account(account_id)` | アカウント削除（関連カラムも削除） |
| `add_column(account_id, kind, order)` | カラム追加 |
| `update_column(column_id, patch)` | カラム設定変更（幅、フィルタ等） |
| `remove_column(column_id)` | カラム削除 |
| `post_note(account_id, text, visibility, reply_id?)` | 投稿 |
| `react(account_id, note_id, reaction)` | リアクション |
| `renote(account_id, note_id)` | リノート |

イベント（Rust→フロント、`emit`）:

| イベント名 | ペイロード | 用途 |
|---|---|---|
| `column:note` | `{ column_id, note }` | 新規ノート受信 |
| `column:note_updated` | `{ column_id, note_id, patch }` | リアクション等の更新 |
| `column:connection_state` | `{ column_id, state: connecting\|connected\|reconnecting\|error }` | 接続状態のUI表示用 |
| `notification:new` | `{ account_id, notification }` | Notificationsカラム/デスクトップ通知トリガー |

いずれのコマンド・イベントもペイロードは§6.1のOpenAPI生成型（および手書きのStreamingメッセージ型）をserdeでシリアライズしたものを使う。トークンはこれらのペイロードに一切含めない。

## 10. MVPの画面構成（概要）

- メイン画面: 横スクロールのカラムリスト（Krile同様）
- カラム追加モーダル: アカウント選択→カラム種別選択→（List/Search選択時は追加パラメータ入力）
- アカウント管理画面: 別ウィンドウ or 設定タブ
- 投稿モーダル: カラム上部の「投稿」ボタンから開く。デフォルトアカウントはそのカラムに紐づくアカウント

## 11. MVPの完了条件（Definition of Done）

- [ ] 2アカウント×3カラム程度を同時に開いた状態で30分以上安定動作（切断・再接続が正しく行われる）
- [ ] リアクション付与がリアルタイムでタイムライン上に反映される
- [ ] アプリ再起動後、カラム構成が復元される（アクセストークンはkeyringから再取得）
- [ ] アプリ再起動後、SQLiteキャッシュから直近のノートが即座に表示され、その後Streamingで追記される
- [ ] Windows/macOS/Linuxそれぞれでビルド・起動確認済み

---

## 12. 未確定事項・要検討リスト

設計を進める上で、現時点では断定できない/要検証の点を列挙しておく。

1. **`progenitor`がMisskeyの`api-doc.json`を問題なく処理できるかの実機検証が未実施。** 生成に失敗するスキーマパターンがあれば、該当箇所のみ手書き型で補うか、生成後のコードにパッチを当てる方針を検討する。
2. **ノートキャッシュ（SQLite）の保持上限とGC方針が未確定。** 何件/何日分を保持するか、カラムごとに異なる保持期間を許容するか、ディスク使用量の上限をどう設けるかを実装前に決める必要がある。
3. **TQL（フィルタDSL）のRust実装コストの見積もりが未実施。** `filter-dsl-design.md`の設計をそのまま実装した場合の工数感（特にSQL射影部分とインメモリ評価部分の二重実装コスト）を、MVP完了後の着手前に見積もる。
4. **ノートキャプチャの解除タイミングの詳細仕様が公式リファレンスに明記されていない。** コミュニティの実装記からの推測部分があるため、実機検証で確認する。
5. **Windows WebView2 / Linux webkit2gtk間でのCSS・パフォーマンス差異。** 特に多カラム時のスクロール性能はOS依存の可能性があり、早期にプロトタイプで確認したい。
6. **絵文字ピッカー（カスタム絵文字含む）の取得方法。** インスタンスごとにカスタム絵文字セットが異なるため、キャッシュ戦略（インスタンス単位でのローカルキャッシュ有効期限など）を別途検討。
7. **リスト/アンテナのMVPでの扱い。** リストはMVPに含めたが、アンテナは複雑なパラメータ（キーワード、フォロー限定等）を持つため、MVP後のPhase2に回すか、簡易版としてMVPに含めるかは要相談。
8. **アプリ名・配布形態（インストーラ形式、自動更新の有無）は未決定。**

---

## 13. 将来拡張（Phase2以降・一覧のみ）

MVP（本書第II部、`misskey-client-prompts.md`のPhase 0〜3相当）完了後のロードマップ。フェーズ番号は`misskey-client-prompts.md`に対応。

- **Phase 4: TQL（フィルタDSL）とマルチカラムの本格対応** — `filter-dsl-design.md`で設計済み。カラム定義を単純なキーワードフィルタからTQLクエリへ置き換え、アンテナ/チャンネル/ユーザー/タグ/キャッシュ検索をカラムソースとして追加
- **Phase 5: Mute/Block/NG・通知・キーアサイン** — ローカルNGワード/NGユーザー/NGインスタンスとサーバー側mute/blockの反映、通知タブ、キーボードショートカットのカスタマイズ
- **Phase 6: 永続化・設定の仕上げ** — ライト/ダークテーマ、設定画面の仕上げ、WS再接続の堅牢化
- テーマ/スキン機能
- プラグイン機構
- 複数ウィンドウ（カラムを別ウィンドウに切り出す）
- 画像/動画のインラインビューア強化
- モバイル版（Tauri Mobile検討）

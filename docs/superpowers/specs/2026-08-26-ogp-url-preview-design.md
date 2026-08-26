# OGP/リンクプレビュー対応 設計 (Issue #9)

## 背景・目的

投稿本文中に貼られたURLに対して、Misskeyインスタンス組み込みのsummalyプロキシからOGP情報（タイトル・説明・サムネイル・サイト名）を取得し、ノート本文の下にプレビューカードとして表示する。動画/音声を持つURL（YouTube等）はクリックでプレイヤーを再生できるようにする。

## スコープ

- 対象: `NoteCard.svelte`が描画するノート本文中の全URL（MFMの`url`ノード、重複は同一URLとしてデデュープ）
- プレイヤー埋め込み（iframe、クリックで遅延ロード）を含む
- 表示ON/OFFの設定トグル（デフォルトON）を`UiPrefs`に追加
- カスタムsummalyプロキシURLをアプリ全体で1つ設定可能にする（任意項目、既定は空＝自分のインスタンスの`/url`を使う）
- センシティブ判定（`sensitive: true`）はセンシティブメディアと同様に「ぼかし→クリックで開示」
- キャッシュはセッション内メモリのみ（アプリ再起動で消える）。永続化しない
- 取得失敗時はカード自体を描画しない（エラーUIは出さない）

### スコープ外 (YAGNI)

- SQLiteへのプレビュー結果の永続化・TTL管理
- 取得失敗時のリトライUIやエラー表示
- CW（内容の警告）非表示時の特別扱い — 既存のCW開閉に自然に追従させる（本文が展開されて初めてプレビューも描画される）

## アーキテクチャ

### Rust (`src-tauri`)

- `domain/url_preview.rs`（新設）: `UrlPreview`, `UrlPlayer`を`specta::Type`付きで定義。フィールドはすべて`Option`（summalyの応答はいずれのフィールドも欠落しうる）。
  ```rust
  pub struct UrlPreview {
      pub url: String, // 正規化後のURL(summalyの返す "url"。リダイレクト先を指すことがある)
      pub title: Option<String>,
      pub description: Option<String>,
      pub thumbnail: Option<String>,
      pub icon: Option<String>,
      pub sitename: Option<String>,
      pub sensitive: bool,
      pub player: Option<UrlPlayer>,
  }
  pub struct UrlPlayer {
      pub url: String,
      pub width: Option<i32>,
      pub height: Option<i32>,
  }
  ```
- `api/url_preview.rs`（新設）: `fetch_url_preview(client: &MisskeyClient, url: &str, custom_proxy: Option<&str>) -> Result<UrlPreview>`。
  - **未確定事項（実装時に実インスタンス疎通で確定させる）**: Misskeyのリンクプレビューは`/api/*`配下のREST APIコマンドではなく、認証不要のプレーンなWebルート `GET /url?url=...` （summalyプロキシの公開口）と理解している。現行スナップショット`openapi/misskey-api-doc.json`には掲載がない（非APIルートのため）。実装時に`#[ignore]`テストで実際のパス・レスポンス形・エラー時のステータスコードを検証し、想定と異なればここを実態に合わせて修正する。
  - `custom_proxy`が`Some`の場合はそのベースURLに対し、`None`の場合は接続先インスタンスの`/url`に対し、いずれも`?url=<encoded>`を付与してGETする。この規約（ベースURL + `?url=`）はMisskey本家の管理者設定`summalyProxy`と同じもの。
  - `client.rs`に、既存の`post`と対称な認証なしGETヘルパー（例: `get_public<R>(&self, path_and_query: &str) -> Result<R>`）を追加。トークンは付与しない。カスタムプロキシ使用時はホスト自体が対象インスタンスと異なるため、`MisskeyClient`経由ではなく指定URLへ直接GETする別経路になる点に注意。
- `commands/note.rs`: `#[tauri::command] async fn fetch_url_preview(account_id: String, url: String, state: State<AppState>) -> Result<UrlPreview, TsumugiError>`を追加し、`specta_builder()`に登録。`state`から`UiPrefs.summalyProxyUrl`（空なら`None`）を読んでRust層に渡す。
- `domain/ui.rs`の`UiPrefs`に以下を追加:
  - `url_preview_enabled: bool`（`#[serde(default = "default_true")]`）
  - `summaly_proxy_url: String`（`#[serde(default)]`、空文字＝未設定＝自分のインスタンスの`/url`を使う）

**セキュリティ上の注意**: カスタムプロキシを設定すると、本文中のURL（プレビュー対象の実URL）が接続先インスタンスではなく第三者のプロキシへ直接送られる。設定UIにその旨の説明を添える。

### フロントエンド (`frontend/src`)

- `lib/urlPreview.ts`（新設、`lib/mentionAvatar.ts`と同構造）:
  - `Map<url, UrlPreview | null>`のセッションキャッシュ + `inflight`によるフェッチ集約
  - `null`は「確定的に取得不可（データなし等）」を意味し永続キャッシュ、ネットワークエラー等の一時的失敗はキャッシュせず次回再試行可能とする
  - キャッシュキーはURLのみ（プレビュー内容自体はアカウントに依存しない公開情報のため、`mentionAvatar`と異なりaccountIdをキーに含めない）
  - フェッチには`app.defaultAccountId()`のアカウントを使う（`mentionAvatar`と同じ慣例）
- `render/UrlPreviewCard.svelte`（新設）:
  - `{ preview: UrlPreview }`を受け取り、サムネイル・タイトル・説明・サイト名を表示するカード
  - `sensitive === true`の場合、`MediaGrid`の閲覧注意と同じ「ぼかし表示→クリックで開示」パターンに揃える
  - `player`がある場合、サムネイル上に再生ボタンを重ね、クリックで`<iframe sandbox="allow-scripts allow-same-origin" src={player.url}>`に差し替える（初期描画では埋め込まない＝自動再生しない）
- `render/Mfm.svelte` / `MfmNode.svelte`: 本文パース時に収集した`url`ノードのURL一覧（重複排除）を呼び出し元に返せるようにする（既存のパース結果から抽出する形で、MFM側の描画ロジック自体は変更しない）
- `ui/NoteCard.svelte`: `MediaGrid`の直後に、`app.ui.urlPreviewEnabled`が真のときのみ、収集したURLごとに`UrlPreviewCard`（`urlPreview.ts`経由で非同期取得、未取得/失敗時は何も描画しない）を並べる
- 設定UI: `ui/settings/`内の適切なセクション（既存の似た性質のトグルの並びを踏襲）に「リンクプレビュー」ON/OFFと、任意項目としてのカスタムsummalyプロキシURL入力欄（プレースホルダーで既定動作を説明、第三者へURLが送られる旨の注記を添える）を追加

## データフロー

1. `NoteCard`が本文をレンダリング → `MfmNode`のパース結果からurlノードのURL一覧を取得
2. `urlPreviewEnabled`が真なら、各URLについて`urlPreview.ts`の関数を呼ぶ（キャッシュ済みなら同期的に返る）
3. 未取得なら`fetch_url_preview`コマンドを呼び、結果をキャッシュしてから`UrlPreviewCard`を描画
4. Rust側は`summalyProxyUrl`が設定されていればそのプロキシへ、未設定なら接続先インスタンスの`/url`へ`?url=...`を付与してGETし、レスポンスを`UrlPreview`に正規化して返す

キャッシュキーはURLのみでプロキシ設定を含めないため、セッション中に設定を変更しても既にキャッシュ済みのプレビューは古いプロキシ経由の結果のまま残る（アプリ再起動で解消）。頻繁に切り替える想定ではないため許容する。

## エラーハンドリング

- ネットワークエラー・タイムアウト・非2xx・パース失敗: フロントは`null`扱いだが**キャッシュしない**（一時的失敗として次回再試行可能）。カードは描画しない
- 明確に「プレビュー対象外」と判定できるレスポンス（例: OGPフィールドが全て欠落）は`null`として永続キャッシュしてよい
- IPC層自体の失敗（コマンド未登録等の開発時エラー）も一時的失敗としてキャッシュしない（`mentionAvatar`の`resolve()`と同じ方針）

## テスト

- Rust: `fetch_url_preview`のレスポンス正規化ロジックの単体テスト（モックJSON→`UrlPreview`変換）
- Rust: `#[ignore]`付きの実連携テストを追加し、実インスタンスに対して`/url`エンドポイントの実際のパス・レスポンス形を検証する（既存の実連携テスト群と同じ並び）
- Frontend (Vitest): `urlPreview.ts`のキャッシュ/inflight dedupe挙動、`UrlPreviewCard.svelte`のsensitive開閉・player遅延ロード挙動
- `cargo test`の`generates_frontend_bindings`で新コマンド・型のTSバインディング生成を確認

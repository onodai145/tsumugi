# Instance Ticker 設計 (Issue #103)

## 背景・目的

Misskey本家の「インスタンスチッカー」相当の機能。NoteCardの投稿者名・acct（`@user@host`）の下に、投稿元インスタンスのアイコン・名前を、そのインスタンスのテーマカラー背景で表示する。マルチインスタンス環境で「これはどこのサーバーの投稿か」を一目で分かるようにする。

## スコープ

- 表示場所は **NoteCardのみ**。設定画面のアカウント一覧（AccountsSection）やAccountSelectには出さない。
- Renote（引用含む）の内側ノートは、既存の `Self`（NoteCard自身の再帰コンポーネント）描画に相乗りするため、個別対応は不要。純粋Renoteの「◯◯ がRenote」バナー行（renoter側）にはチッカーを出さない。

## 設定

`UiPrefs` に文字列フィールドを1つ追加する。

```rust
/// Instance Ticker の表示モード（Issue #103）。
/// "off"    = 表示しない
/// "remote" = リモートユーザーの投稿にのみ表示（既定）
/// "always" = ローカルユーザー（自分と同一インスタンス）の投稿にも表示
#[serde(default = "default_instance_ticker")]
pub instance_ticker: String,
```

既定値は `"remote"`（本家Misskeyのデフォルト挙動に合わせる）。

設定UIは `AppearanceSection.svelte` に、既存の「テーマ」セグメントコントロールと同じパターンで追加する（3ボタン: 表示しない/リモートのみ/常に表示）。

## データソース

### リモートユーザー

MisskeyのノートAPI・Streamingが返す `UserLite` オブジェクトには、`host != null` のユーザーに対してのみ `instance: { name, softwareName, softwareVersion, iconUrl, faviconUrl, themeColor }` が埋め込まれている（OpenAPIスキーマの `UserLite.required` に `instance` は含まれず、ローカルユーザーには存在しないフィールド）。これは既存のノート取得経路（REST正規化・Streaming）にそのまま乗っているので、**追加のAPI呼び出しは不要**。

`domain::User` に以下を追加する:

```rust
/// 投稿元インスタンス情報（リモートユーザーのみ Some）。ノートキャッシュのJSONに
/// 保存済みの既存レコード（このフィールド追加前）を壊さないよう #[serde(default)]。
#[serde(default)]
pub instance: Option<InstanceInfo>,
```

`api/normalize.rs` の `RawUser` に `instance: Option<RawInstanceInfo>` を追加し、`From<RawUser> for User` で変換する。`store/note_cache.rs:534` 付近の `User` リテラル構築箇所にも `instance: None` を追加する。

### ローカルユーザー（"always" モード時のみ使用）

ローカルユーザーの投稿には `instance` が付与されないため、接続先インスタンス自身の情報を別途保持する必要がある。

- Misskeyの `/api/meta`（認証不要、`{"detail": false}` で軽量レスポンス）から `name` / `iconUrl` / `themeColor` を取得する。
- `api/meta.rs` に `pub async fn fetch_meta(client: &MisskeyClient) -> Result<InstanceInfo>` を追加。
- `domain::Account` に `instance: Option<InstanceInfo>` を追加。
- 新規コマンド `refresh_instance_meta(account_id: String) -> Result<Account>`（`commands/account.rs`）:
  1. `state.client_for(&account_id)` でクライアント取得
  2. `fetch_meta` を呼ぶ
  3. `state.accounts` 内の該当Accountを更新し、`state.settings.upsert_account()` で永続化
  4. 更新後の `Account` を返す
- フロント `store.svelte.ts` の `boot()` 内で、`listAccounts()` 後に全アカウント分 `refresh_instance_meta` を **fire-and-forget**（`Promise.allSettled`、await しない）で呼び、解決ごとに `app.accounts` の該当要素を差し替える。起動シーケンスをブロックしない。ネットワークエラー時は無視（既存の `instance` を維持、次回起動で再試行）。

### 共有型 `InstanceInfo`

`domain/user.rs` に定義し、`User` と `Account` の両方から使う:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InstanceInfo {
    pub name: Option<String>,
    pub icon_url: Option<String>,
    pub theme_color: Option<String>,
}
```

`faviconUrl` は取得対象に含めない（`/api/meta` の `MetaLite` スキーマに存在しないため、リモート側の値も使わない）。ただし `iconUrl`（リモートは`UserLite.instance.iconUrl`、ローカルは`/api/meta`の`iconUrl`）が未設定の場合、`InstanceInfo::with_favicon_fallback` がホストの `https://{host}/favicon.ico` にフォールバックする（本家Misskeyの `MkInstanceTicker.vue` と同じ挙動）。管理者がアイコンを設定していないインスタンスでも favicon が実在すればアイコン欠落を避けられる。

`themeColor` が未設定の場合も、本家Misskeyの `MkInstanceTicker.vue` と同じ既定色 `#777777`（グレー）に `InstanceInfo::with_theme_color_fallback` でフォールバックする（本家はホストから動的に取得する手段が無く、固定値にフォールバックしている）。これにより未設定インスタンスの投稿もグラデーション付きで表示される。

## 表示ロジック（フロント）

`NoteCard.svelte` で `inner.user` に対して評価する:

```
mode = app.ui.instanceTicker ?? "remote"
if mode === "off": 非表示
else if inner.user.instance: inner.user.instance を表示（リモート）
else if inner.user.host === null && mode === "always":
  account = app.accounts.find(a => a.id === accountId)
  account?.instance があればそれを表示
else: 非表示
```

`name` が null の場合は host（`inner.user.host` またはローカルなら接続先 `account.host`）をフォールバック表示に使う。

## 見た目

- ヘッダー行（名前・acct・時刻・可視性アイコン）の直後、CW欄より前に新しい行として挿入。
- `themeColor` 背景 + アイコン（`iconUrl` があれば）+ インスタンス名のピル。角丸はスタイルガイドのxsバッジ相当（`rounded-sm`）。
- `themeColor` は他インスタンス由来の任意hexであり可読性が保証されないため、相対輝度から文字色（黒/白）を自動選択するヘルパー `lib/color.ts: readableTextColor(hex: string): "#000000" | "#ffffff"` を新設し、単体テストを書く。Rust側で`with_theme_color_fallback`により通常は必ず値が入るため、フロント側の `muted` フォールバックは「不正なhex文字列（CSSインジェクション対策で弾いた値）」のときのみ発生する防御的な扱いになる。

## テスト

- Rust: `normalize.rs` に、`instance` ありのリモートユーザーJSON / なしのローカルユーザーJSONそれぞれのデシリアライズテストを追加。`fetch_meta` はユニットテスト（レスポンスJSONのパース）程度に留める（実サーバー接続系は `#[ignore]`）。
- Frontend: `lib/color.ts` の `readableTextColor` に単体テスト（黒背景→白文字、白背景→黒文字、境界値）。`NoteCard.test.ts` に、`off`/`remote`/`always` 各モードでのチッカー表示有無のケースを追加。

## 非対応（YAGNI）

- チッカーのクリック挙動（インスタンス情報ページを開く等）は本家にもあるが今回はスコープ外。単なる表示のみ。
- AccountsSection/AccountSelect等、NoteCard以外の箇所への表示は対象外。

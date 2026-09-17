# デバッグログ・可観測性の改善 設計

Issue #241。Issue #228(backfillキャッシュ優先化)の動作確認中に浮上した、デバッグログ・observability周りの改善。

## 背景

- 「実際どれだけAPI呼び出しを削減できているか」を測る手段が今は皆無。`resume_column`の境界未設定(Issue #228のPR #237で残課題として記載)のような「実は機能が働いていない」ケースを本番で検知できない。
- `enable_file_logging`設定(既定`false`)がONの時しか`tauri-plugin-log`が登録されず、`log::info!`等は無音でどこにも出力されない(stdoutにも出ない)。`cargo tauri dev`での開発中に一時的な`log::info!`が出力されず調査に手間取った実例がある。

### 現状のログ呼び出し棚卸し

Rust側の`log::{info,warn,error}!`呼び出しは34箇所(`log::debug!`/`trace!`の直接呼び出しは無し)。内訳:

- `stream/connection.rs`: WS接続失敗・idle timeout・再接続系
- `store/{note_cache,postgres_backend,mysql_backend}.rs`: キャッシュ行のパース失敗スキップ
- `sound.rs`: 通知音の出力デバイス/デコード失敗
- `debug_bridge.rs`: bridge起動・接続エラー
- `lib.rs`: 起動時の設定移行・キャッシュバックエンド接続エラー
- `api/mutes.rs`: ミュート正規表現の不正パターン
- `commands/app.rs::log_frontend_event`: フロント発火イベントを`target: "frontend"`で転送(JS側からerror/warn/info/debugレベル指定)

一方、Misskey REST API呼び出し(`api/client.rs::MisskeyClient::post`)には**ログが一切無い**。エンドポイント名・呼び出し回数・レイテンシが分からず、「どれだけAPI呼び出しを削減できたか」を直接見る手段がない。

## スコープ

1. `client.rs::post`にMisskey APIアクセスログ(エンドポイント名・ステータス・所要時間)を追加
2. `fetch_backfill`/`resume_column`のキャッシュhit/fallback回数をカウントし、Backstageの新設「メトリクス」タブで可視化
3. devビルドでは`enable_file_logging`設定に関係なく常にロガーを登録する
4. Streaming受信時のフィルタ/ミュートによるノートdropをログ出力する

## 1. Misskey APIアクセスログ

`src-tauri/src/api/client.rs::post`で、レスポンスのステータスが確定した時点(成功・エラー両方の分岐に入る前)に1行出す。

```rust
pub async fn post<B, R>(&self, endpoint: &str, body: &B) -> Result<R>
where
    B: Serialize,
    R: DeserializeOwned,
{
    let value = self.build_body(body)?;
    let url = format!("{}/{}", self.api_base, endpoint.trim_start_matches('/'));
    let started = std::time::Instant::now();
    let resp = self.http.post(&url).json(&value).send().await?;
    let status = resp.status();
    log::debug!(target: "api", "{endpoint} -> {status} ({}ms)", started.elapsed().as_millis());

    if status.is_success() {
        ...
```

- `target: "api"`にすることで、将来的にレベルを個別調整できるようにする(`frontend`ターゲットと同じ扱い)
- ログにはエンドポイント名・ステータス・所要時間のみを含め、リクエストボディ(トークンを含む`value`)やレスポンス本文は一切出力しない
- レベルは`debug`。3節の変更でdevビルドでは既定表示、releaseビルドでは`enable_file_logging`時のみ表示(既存のdebugレベル運用と同じ)

## 2. キャッシュhit/fallbackカウンタとBackstage「メトリクス」タブ

### カウンタ設計

`AppState`に集計用の構造体を追加する(`AtomicU64`、プロセス内メモリのみでDB永続化はしない)。

```rust
// state.rs
#[derive(Default)]
pub struct CacheMetrics {
    pub backfill_hit: AtomicU64,
    pub backfill_fallback_boundary: AtomicU64, // cache_eligibleだがboundary未確定でAPIへ(Issue #228 PR#237の残課題)
    pub backfill_fallback_other: AtomicU64,    // cache_eligibleだが範囲外/件数不足でAPIへ
    pub resume_hit: AtomicU64,
    pub resume_fallback: AtomicU64,
}
```

- `resume`(`resume_column`)と`backfill`(`fetch_backfill`)は分けてカウントする。`resume`のhitはバックグラウンドでgap fill用のAPI呼び出しが走りうるため(`column.rs:308-335`)、「API削減」の指標として`backfill`と混ぜると誤解を招く。
- カウント対象は**キャッシュ適合パスのみ**。通知カラム(`resume_column`で`is_notif`)、複数ソースカラム(`fetch_backfill`で`cache_eligible = false`)は、そもそもキャッシュを試みないためカウント対象外(分母に含めない)。
- `backfill`のfallbackは理由別に2種類に分ける。境界未確定(`boundary: None`)によるfallbackを`other`と混ぜると、「hit率が低い」ことは分かっても「境界設定機能が働いていない」ことが見えなくなるため。

### 計装箇所

- `commands/column.rs::resume_column`(266行目〜): `notes.is_empty()`で分岐する箇所(294行目)で、`is_notif`でなければ`resume_hit`/`resume_fallback`をインクリメント
- `commands/column.rs::fetch_backfill`(392行目〜): `cache_eligible`が真の場合のみ、`cache_backfill_page`が`Some`を返した(431行目でreturn)なら`backfill_hit`、`None`だった場合は`boundary`が`None`なら`backfill_fallback_boundary`、それ以外(範囲外/件数不足)なら`backfill_fallback_other`をインクリメント

### コマンド

```rust
#[derive(Serialize, specta::Type)]
pub struct DebugMetrics {
    pub backfill_cache_hit: i32,
    pub backfill_cache_fallback_boundary: i32,
    pub backfill_cache_fallback_other: i32,
    pub resume_cache_hit: i32,
    pub resume_cache_fallback: i32,
}

#[tauri::command]
#[specta::specta]
pub async fn get_debug_metrics(state: State<'_, AppState>) -> Result<DebugMetrics>
```

- 全フィールド`i32`(`note_count`/`prune_note_cache`と同じ変換方針。`specta-typescript`が`u64`を`bigint`にマップし、フロント側の数値演算・`toLocaleString()`と相性が悪いため)
- `specta_builder()`(`lib.rs`)に登録し、`cargo test`でTSバインディングを再生成する(`tauri.gen.ts`は手編集しない)
- 名前を`DebugMetrics`/`get_debug_metrics`という汎用的なものにし、将来他の指標(WS再接続回数など)を同じ構造体にフィールド追加するだけで拡張できるようにする(今回はキャッシュ関連の5フィールドのみ実装する)

### Backstage UI

`frontend/src/ui/Backstage.svelte`の展開パネルに、既存の「ログ」一覧に加えて「メトリクス」タブを追加する。

- パネル上部にタブ切り替え(「ログ」/「メトリクス」、既存の`open`state配下)
- 「メトリクス」タブを開いている間だけ`get_debug_metrics`をポーリング(常時ポーリングはしない。既存の`#pollStats`とは別の軽量なポーリングをタブのマウント中のみ動かす)
- 表示内容: backfillのhit率(`backfill_cache_hit / (全backfill試行数)`、試行0件なら`—`)、および各カウンタの生値(hit / fallback(境界未確定) / fallback(その他) / resume hit / resume fallback)をラベル付きで並べる
- 将来の指標追加を見越し、1行1指標のシンプルな箇条書き/テーブル構造にする(グラフ等は今回作らない)

## 3. devビルドでの常時ログ登録

`src-tauri/src/lib.rs:224`の条件を変更する。

```rust
// 変更前
if settings.load_ui().unwrap_or_default().enable_file_logging {

// 変更後
if cfg!(debug_assertions) || settings.load_ui().unwrap_or_default().enable_file_logging {
```

- `app.handle().plugin(...)`の呼び出しは1箇所のまま(2箇所にすると`log::set_logger`が2回目で失敗する)
- releaseビルドの挙動は変更なし(`enable_file_logging`設定通り)
- devビルドでは設定に関係なく常に`Stdout` + `LogDir`ターゲットが有効になり、`cargo tauri dev`のターミナルにログが流れるようになる
- `frontend`ターゲット(Debugレベル)・新設の`api`/`filter`ターゲット(Debugレベル)を含め、既存のレベル設定(`.level(Info)` + `.level_for("frontend", Debug)`)に`.level_for("api", Debug)` / `.level_for("filter", Debug)`を追加する

## 4. Streaming受信時のフィルタ/ミュートdropログ

「なぜこのノートが表示されない」の調査用に、ライブStreaming受信の1件ずつの判定分岐(`src-tauri/src/stream/connection.rs:751-763`)にログを追加する。過去ページ一括取得側(`commands/column.rs`の複数箇所にある`retain()`ベースのフィルタ)は一度に大量のノートを処理するためログ量が跳ね上がる。よってログ追加対象はライブStreaming受信のみとし、一括取得側は対象外とする。

```rust
if !filter.matches(&normalized, ctx) {
    log::debug!(target: "filter", "[{column_id}] note {id} dropped: TQL filter not matched");
    return HandleResult::None;
}
if let Some(state) = app.try_state::<AppState>() {
    if crate::filter::mute::is_muted(&normalized, &state.mute.lock().unwrap()) {
        log::debug!(target: "filter", "[{column_id}] note {id} dropped: local mute");
        return HandleResult::None;
    }
    if is_server_muted_note(&state, account_id, &normalized) {
        log::debug!(target: "filter", "[{column_id}] note {id} dropped: server mute/block");
        return HandleResult::None;
    }
    if state.is_word_muted(account_id, &normalized) {
        log::debug!(target: "filter", "[{column_id}] note {id} dropped: word mute");
        return HandleResult::None;
    }
    ...
```

- 同一ノートの多重配信排除(`sub.dedup.accept`、745行目)はフィルタ/ミュートとは別の仕組みのため今回は対象外とする
- レベルは`debug`。3節の変更でdevビルドでは既定表示、releaseビルドでは`enable_file_logging`時のみ表示

## テスト

- Rust:
  - `fetch_backfill`が`backfill_hit`/`backfill_fallback_boundary`/`backfill_fallback_other`を正しい条件でインクリメントすることを検証する単体テスト(境界未確定・範囲外・hit の3ケース)
  - `resume_column`が`resume_hit`/`resume_fallback`を正しくインクリメントすることを検証する単体テスト(キャッシュ有無の2ケース、通知カラムはインクリメントしないことも確認)
  - `client.rs::post`のアクセスログ、`connection.rs`のフィルタ/ミュートdropログはログ出力自体の単体テストは行わない(既存の`log::warn!`等と同様、出力内容の検証は割愛。フィルタ/ミュート判定そのものの正しさは既存のフィルタ/ミュートテストが担保する)
- フロントエンド:
  - Backstageメトリクスタブのhit率%表示・試行0件時の`—`表示のvitestテスト

## 影響範囲・互換性

- DB永続化なし(プロセス内メモリのみ)。アプリ再起動でカウンタは0にリセットされる(セッション単位の統計という位置付け)
- 新規コマンド`get_debug_metrics`追加により`tauri.gen.ts`が再生成される
- `Cargo.toml`/依存追加なし(`std::sync::atomic`/`std::time::Instant`のみ)

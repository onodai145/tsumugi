# デバッグログ・可観測性の改善 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #241 に基づき、(1) Misskey APIアクセスログ、(2) キャッシュhit/fallbackカウンタ + Backstage「メトリクス」タブ、(3) devビルドでの常時ログ登録、(4) Streaming受信時のフィルタ/ミュートdropログ、を実装する。

**Architecture:** Rust側は既存の`log`クレート運用(`tauri-plugin-log`、`target:`ベースのレベル制御)を踏襲し、新規カウンタは`AppState`にプロセス内`AtomicU64`として追加する。新規コマンド`get_debug_metrics`経由でフロントへ公開し、`frontend/src/ui/Backstage.svelte`の既存パネルにタブを追加して表示する。

**Tech Stack:** Rust(`log`, `tauri-plugin-log`, `std::sync::atomic`), `wiremock`(既存テスト依存), Svelte 5(runes) + Vitest。

## Global Constraints

- 作業ブランチは`feature/241-debug-logging-observability`(作成済み)。直接`main`にコミットしない。
- 新規コマンドは必ず`src-tauri/src/lib.rs`の`specta_builder()`内`collect_commands![...]`に登録する。TSバインディング(`frontend/src/bindings/tauri.gen.ts`)は`cargo test`(`generates_frontend_bindings`)で再生成し、手編集しない。
- フロントへ返す数値フィールドは`i32`にする(`specta-typescript`が`u64`を`bigint`にマップし、`toLocaleString()`等と相性が悪いため。既存の`note_count`/`prune_note_cache`と同じ方針)。
- コミットメッセージは1行(subjectのみ)。本文・箇条書きは書かない。末尾に`Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>`が別途付与される。
- Rustテストは`src-tauri`ディレクトリで`cargo test`、フロントは`frontend`ディレクトリで`pnpm check` / `pnpm test`。
- `cargo run`や`./target/debug/tsumugi`を直接実行しない。動作確認は`cargo tauri dev`(リポジトリルートから)を使う。
- 参照するspec: `docs/superpowers/specs/2026-09-17-debug-logging-observability-design.md`

---

## Task 1: Misskey APIアクセスログ(`client.rs::post`)

**Files:**
- Modify: `src-tauri/src/api/client.rs`(`post`関数、40行目台〜85行目付近)
- Test: `src-tauri/src/api/client.rs`(同ファイル内`#[cfg(test)] mod tests`、154行目〜)

**Interfaces:**
- Consumes: なし(このタスクは`client.rs`内で完結)
- Produces: `MisskeyClient::post`が成功/失敗どちらの分岐でも`log::debug!(target: "api", ...)`を1行出す(呼び出し元からは見えない内部変更)。以降のタスクはこれに依存しない。

このタスクは外部から見た`post`の戻り値・エラー種別を変えない**リファクタ+ログ追加**。新しい振る舞いを追加するわけではないため、通常のTDD(red→green)ではなく「現状の挙動を固定するテストを先に書いて通し、リファクタ後も通ることを確認する」手順を踏む。

- [ ] **Step 1: 現状の挙動を固定する回帰テストを書く**

`src-tauri/src/api/client.rs`の`mod tests`内、`maps_status_to_typed_error`テストの直後に追加する。

```rust
#[tokio::test]
async fn post_maps_error_body_to_typed_error() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/notes/show"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": { "code": "NO_SUCH_NOTE", "message": "No such note." }
        })))
        .mount(&mock)
        .await;

    let c = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
    let res: Result<serde_json::Value> = c.post("notes/show", &json!({})).await;

    match res {
        Err(Error::NotFound(detail)) => {
            assert!(detail.contains("NO_SUCH_NOTE"), "detail should contain error code: {detail}");
            assert!(detail.contains("No such note."), "detail should contain error message: {detail}");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}
```

- [ ] **Step 2: テストを実行し、現状の実装で通ることを確認する**

Run: `cd src-tauri && cargo test post_maps_error_body_to_typed_error`
Expected: PASS(まだ`post`を変更していないので、現行実装でそのまま通る)

- [ ] **Step 3: `post`をリファクタしてアクセスログを追加する**

`post`関数を以下に置き換える(既存の`build_body`呼び出し・`url`組み立ては変更しない)。

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
    let bytes = resp.bytes().await?;
    log::debug!(
        target: "api",
        "{endpoint} -> {status} ({} bytes, {}ms)",
        bytes.len(),
        started.elapsed().as_millis()
    );

    if status.is_success() {
        // 204 No Content 等、本文が空なら null として扱う
        if bytes.is_empty() {
            return Ok(serde_json::from_value(Value::Null)?);
        }
        return Ok(serde_json::from_slice(&bytes)?);
    }

    Err(Self::map_error(endpoint, status, String::from_utf8(bytes.to_vec()).ok()))
}
```

- [ ] **Step 4: テストを再実行し、リファクタ後も通ることを確認する**

Run: `cd src-tauri && cargo test post_maps_error_body_to_typed_error`
Expected: PASS

- [ ] **Step 5: 既存のclient.rsテスト全体を実行し、回帰が無いことを確認する**

Run: `cd src-tauri && cargo test --lib api::client::tests`
Expected: 全件PASS

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/api/client.rs
git commit -m "feat: Misskey APIアクセスログをclient.rs::postに追加"
```

---

## Task 2: `CacheMetrics`構造体とロジックのユニットテスト

**Files:**
- Modify: `src-tauri/src/state.rs`(`AppState`構造体・`impl AppState`・末尾の`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: なし
- Produces:
  - `pub enum BackfillOutcome { Hit, FallbackBoundaryUnset, FallbackOther }`(`Debug, Clone, Copy, PartialEq, Eq`導出)
  - `pub struct CacheMetrics`(`Default`導出)とそのメソッド:
    - `pub fn record_backfill(&self, outcome: BackfillOutcome)`
    - `pub fn record_resume(&self, hit: bool)`
    - `pub fn backfill_hit(&self) -> i32`
    - `pub fn backfill_fallback_boundary(&self) -> i32`
    - `pub fn backfill_fallback_other(&self) -> i32`
    - `pub fn resume_hit(&self) -> i32`
    - `pub fn resume_fallback(&self) -> i32`
  - `AppState`に`pub cache_metrics: CacheMetrics`フィールドを追加(Task 3で`commands/column.rs`から使う)

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/state.rs`の`#[cfg(test)] mod tests`内(ファイル末尾、既存の`restores_persisted_accounts_on_construction`等がある箇所)に追加する。

```rust
#[test]
fn cache_metrics_record_backfill_increments_the_matching_counter() {
    let m = CacheMetrics::default();
    m.record_backfill(BackfillOutcome::Hit);
    m.record_backfill(BackfillOutcome::Hit);
    m.record_backfill(BackfillOutcome::FallbackBoundaryUnset);
    m.record_backfill(BackfillOutcome::FallbackOther);

    assert_eq!(m.backfill_hit(), 2);
    assert_eq!(m.backfill_fallback_boundary(), 1);
    assert_eq!(m.backfill_fallback_other(), 1);
}

#[test]
fn cache_metrics_record_resume_increments_hit_or_fallback() {
    let m = CacheMetrics::default();
    m.record_resume(true);
    m.record_resume(true);
    m.record_resume(false);

    assert_eq!(m.resume_hit(), 2);
    assert_eq!(m.resume_fallback(), 1);
}
```

- [ ] **Step 2: テストを実行し、コンパイルエラー(型未定義)で失敗することを確認する**

Run: `cd src-tauri && cargo test cache_metrics_record`
Expected: FAIL(`cannot find type \`CacheMetrics\` in this scope` 等のコンパイルエラー)

- [ ] **Step 3: `CacheMetrics`/`BackfillOutcome`を実装する**

`state.rs`冒頭のuse文に追加:

```rust
use std::sync::atomic::{AtomicU64, Ordering};
```

`PendingMiAuth`構造体の直後(`AppState`定義の前)に追加:

```rust
/// `fetch_backfill`のキャッシュhit/fallback理由(Issue #241)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackfillOutcome {
    Hit,
    /// cache_eligibleだがbackfill境界(get_fetch_boundary)が未確定でAPIへ。
    /// Issue #228のPR #237で残課題として記載された「実は機能が働いていない」ケースを可視化する。
    FallbackBoundaryUnset,
    /// cache_eligibleだが境界は確定済み、範囲外/件数不足でAPIへ。
    FallbackOther,
}

/// キャッシュhit/fallback回数のプロセス内カウンタ(Issue #241)。DB永続化はせず、
/// アプリ再起動でリセットされるセッション単位の統計という位置付け。
#[derive(Default)]
pub struct CacheMetrics {
    backfill_hit: AtomicU64,
    backfill_fallback_boundary: AtomicU64,
    backfill_fallback_other: AtomicU64,
    resume_hit: AtomicU64,
    resume_fallback: AtomicU64,
}

impl CacheMetrics {
    pub fn record_backfill(&self, outcome: BackfillOutcome) {
        let counter = match outcome {
            BackfillOutcome::Hit => &self.backfill_hit,
            BackfillOutcome::FallbackBoundaryUnset => &self.backfill_fallback_boundary,
            BackfillOutcome::FallbackOther => &self.backfill_fallback_other,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_resume(&self, hit: bool) {
        let counter = if hit { &self.resume_hit } else { &self.resume_fallback };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn backfill_hit(&self) -> i32 {
        self.backfill_hit.load(Ordering::Relaxed) as i32
    }

    pub fn backfill_fallback_boundary(&self) -> i32 {
        self.backfill_fallback_boundary.load(Ordering::Relaxed) as i32
    }

    pub fn backfill_fallback_other(&self) -> i32 {
        self.backfill_fallback_other.load(Ordering::Relaxed) as i32
    }

    pub fn resume_hit(&self) -> i32 {
        self.resume_hit.load(Ordering::Relaxed) as i32
    }

    pub fn resume_fallback(&self) -> i32 {
        self.resume_fallback.load(Ordering::Relaxed) as i32
    }
}
```

`AppState`構造体に1フィールド追加(`sound: SoundPlayer,`の直後):

```rust
    /// キャッシュhit/fallback回数の集計(Issue #241)。Backstageの「メトリクス」タブ用。
    pub cache_metrics: CacheMetrics,
```

`new_with_sound`内、`Self { ... }`の`sound,`の直後に追加:

```rust
            cache_metrics: CacheMetrics::default(),
```

- [ ] **Step 4: テストを実行し、成功することを確認する**

Run: `cd src-tauri && cargo test cache_metrics_record`
Expected: PASS(2件とも)

- [ ] **Step 5: state.rs全体のテストを実行し、既存テストが壊れていないことを確認する**

Run: `cd src-tauri && cargo test --lib state::tests`
Expected: 全件PASS

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/state.rs
git commit -m "feat: キャッシュhit/fallbackカウンタCacheMetricsを追加"
```

---

## Task 3: `resume_column`/`fetch_backfill`への計装

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(14行目のuse文、`resume_column`286〜292行目、`fetch_backfill`430〜433行目付近)

**Interfaces:**
- Consumes: Task 2の`crate::state::{AppState, BackfillOutcome}`、`AppState::cache_metrics`、`CacheMetrics::record_backfill`/`record_resume`
- Produces: `resume_column`/`fetch_backfill`実行のたびに`state.cache_metrics`が更新される(Task 4のコマンドがこれを読む)

このタスクの対象コマンド(`resume_column`/`fetch_backfill`)は既存コードでも直接のユニットテストが無い(アカウント/host/token/Streaming接続を要するため)。カウント条件の分岐ロジック自体はTask 2で単体テスト済みなので、このタスクでは「正しい条件で正しいメソッドを呼んでいるか」をコードレビュー観点で慎重に書き、コンパイル+既存テストスイートの無回帰で確認する。UIからの実地確認はTask 7で行う。

- [ ] **Step 1: importを更新する**

`src-tauri/src/commands/column.rs`14行目を変更:

```rust
// 変更前
use crate::state::AppState;

// 変更後
use crate::state::{AppState, BackfillOutcome};
```

- [ ] **Step 2: `resume_column`に計装する**

286〜292行目付近の以下のブロック:

```rust
    // 通知以外はキャッシュ優先で即時表示（空なら REST）
    let notes = if is_notif {
        vec![]
    } else {
        let cached = state.cache.load_cached(&column.id, INITIAL_LIMIT).await?;
        if cached.is_empty() { vec![] } else { cached }
    };
```

を以下に変更する(`is_notif`でない場合のみ記録。通知カラムはそもそもキャッシュを試みないため分母に含めない):

```rust
    // 通知以外はキャッシュ優先で即時表示（空なら REST）
    let notes = if is_notif {
        vec![]
    } else {
        let cached = state.cache.load_cached(&column.id, INITIAL_LIMIT).await?;
        let notes = if cached.is_empty() { vec![] } else { cached };
        state.cache_metrics.record_resume(!notes.is_empty());
        notes
    };
```

- [ ] **Step 3: `fetch_backfill`に計装する**

`fetch_backfill`内、`if let Some(notes) = cache_backfill_page(boundary.as_deref(), &until_id, cached, INITIAL_LIMIT) { return Ok(notes); }`の行を以下に変更する:

```rust
        if let Some(notes) = cache_backfill_page(boundary.as_deref(), &until_id, cached, INITIAL_LIMIT) {
            state.cache_metrics.record_backfill(BackfillOutcome::Hit);
            return Ok(notes);
        }
        state.cache_metrics.record_backfill(if boundary.is_none() {
            BackfillOutcome::FallbackBoundaryUnset
        } else {
            BackfillOutcome::FallbackOther
        });
    }
```

(元々あった`}`(`if cache_eligible`ブロックの閉じ)の直前に`record_backfill`呼び出しを挿入する形。`cache_eligible`が`false`の場合はこのブロックに入らないため記録されない。)

- [ ] **Step 4: ビルドが通ることを確認する**

Run: `cd src-tauri && cargo build`
Expected: 成功(warningも無いこと)

- [ ] **Step 5: 既存テストスイート全体を実行し、無回帰を確認する**

Run: `cd src-tauri && cargo test`
Expected: 全件PASS

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "feat: resume_column/fetch_backfillにキャッシュ計装を追加"
```

---

## Task 4: `get_debug_metrics`コマンドとTSバインディング再生成

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`OpenedColumn`定義の直後あたりに`DebugMetrics`/`get_debug_metrics`を追加)
- Modify: `src-tauri/src/lib.rs`(`specta_builder()`の`collect_commands![...]`、51行目`commands::column::notes_since,`の直後)
- Generated: `frontend/src/bindings/tauri.gen.ts`(`cargo test`で自動生成、手編集しない)

**Interfaces:**
- Consumes: Task 2の`AppState::cache_metrics`とそのgetterメソッド群
- Produces:
  - `pub struct DebugMetrics { pub backfill_cache_hit: i32, pub backfill_cache_fallback_boundary: i32, pub backfill_cache_fallback_other: i32, pub resume_cache_hit: i32, pub resume_cache_fallback: i32 }`
  - `pub async fn get_debug_metrics(state: State<'_, AppState>) -> Result<DebugMetrics>`
  - フロント側TS: `commands.getDebugMetrics(): Promise<Result<DebugMetrics, Error>>`、型`DebugMetrics`(Task 7が使う)

- [ ] **Step 1: `DebugMetrics`構造体と`get_debug_metrics`コマンドを追加する**

`src-tauri/src/commands/column.rs`の`OpenedColumn`定義(29〜34行目)の直後に追加する:

```rust
/// キャッシュhit/fallback回数のスナップショット(Issue #241)。BackstageのメトリクスUI用。
/// フィールドを増やせば他の指標(WS再接続回数など)も同じ場所に追加できる想定の汎用DTO。
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DebugMetrics {
    pub backfill_cache_hit: i32,
    pub backfill_cache_fallback_boundary: i32,
    pub backfill_cache_fallback_other: i32,
    pub resume_cache_hit: i32,
    pub resume_cache_fallback: i32,
}

/// デバッグ用メトリクスのスナップショットを返す。Backstageの「メトリクス」タブがポーリングする。
#[tauri::command]
#[specta::specta]
pub async fn get_debug_metrics(state: State<'_, AppState>) -> Result<DebugMetrics> {
    Ok(DebugMetrics {
        backfill_cache_hit: state.cache_metrics.backfill_hit(),
        backfill_cache_fallback_boundary: state.cache_metrics.backfill_fallback_boundary(),
        backfill_cache_fallback_other: state.cache_metrics.backfill_fallback_other(),
        resume_cache_hit: state.cache_metrics.resume_hit(),
        resume_cache_fallback: state.cache_metrics.resume_fallback(),
    })
}
```

- [ ] **Step 2: `specta_builder()`に登録する**

`src-tauri/src/lib.rs`の`commands::column::notes_since,`(51行目)の直後に1行追加:

```rust
            commands::column::notes_since,
            commands::column::get_debug_metrics,
            commands::column::prune_note_cache,
```

- [ ] **Step 3: TSバインディングを再生成する**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS(`frontend/src/bindings/tauri.gen.ts`が更新される)

- [ ] **Step 4: 生成されたバインディングにコマンド・型が含まれることを確認する**

Run: `grep -n "getDebugMetrics\|DebugMetrics" frontend/src/bindings/tauri.gen.ts`
Expected: `getDebugMetrics`関数と`DebugMetrics`型定義が出力される

- [ ] **Step 5: Rust側テストスイート全体を実行する**

Run: `cd src-tauri && cargo test`
Expected: 全件PASS

- [ ] **Step 6: コミット(生成物含む)**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: get_debug_metricsコマンドを追加"
```

---

## Task 5: devビルドでの常時ログ登録

**Files:**
- Modify: `src-tauri/src/lib.rs`(220〜234行目付近のロガー登録ブロック)

**Interfaces:**
- Consumes: なし
- Produces: devビルド(`cfg!(debug_assertions)`)では`enable_file_logging`設定に関係なく`tauri-plugin-log`が常に登録される。`target: "api"`(Task 1)・`target: "filter"`(Task 6)のログがDebugレベルで出力対象になる。

ロガー登録タイミング自体の自動テストは行わない(既存コードにもこのプラグイン登録の単体テストは無く、`app.handle()`を要するため単純にはテストできない)。手動確認はTask 8でまとめて行う。

- [ ] **Step 1: 条件とlevel_forを変更する**

`src-tauri/src/lib.rs`の以下のブロック(220〜234行目付近):

```rust
            // 設定(UiPrefs.enable_file_logging)でON/OFFする(Issue #12: 「謎のタイミングで
            // 通知が来る」の調査用に、リリースビルドでもWS再接続/pingタイムアウトのログを
            // 残せるようにする)。既定ターゲット(Stdout + LogDir)のうち LogDir 側がアプリの
            // ログディレクトリに永続化される。切替はプラグイン登録の性質上、次回起動から反映。
            if settings.load_ui().unwrap_or_default().enable_file_logging {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        // フロントの"発火"系詳細ログ(commands::app::log_frontend_event)だけは
                        // Debugレベルで送っている(Backstage UIには出さずファイルにだけ残すため)。
                        // 全体をDebugにすると依存クレートのログまで大量に混ざるので target 限定で緩める。
                        .level_for("frontend", log::LevelFilter::Debug)
                        .build(),
                )?;
            }
```

を以下に置き換える:

```rust
            // 設定(UiPrefs.enable_file_logging)でON/OFFする(Issue #12: 「謎のタイミングで
            // 通知が来る」の調査用に、リリースビルドでもWS再接続/pingタイムアウトのログを
            // 残せるようにする)。既定ターゲット(Stdout + LogDir)のうち LogDir 側がアプリの
            // ログディレクトリに永続化される。切替はプラグイン登録の性質上、次回起動から反映。
            // devビルドでは設定に関係なく常に登録する(Issue #241: cargo tauri dev中に
            // log::info!等が無音でどこにも出ないと調査しづらいため)。releaseビルドの挙動は
            // enable_file_logging設定通りで変更なし。
            if cfg!(debug_assertions) || settings.load_ui().unwrap_or_default().enable_file_logging {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        // フロントの"発火"系詳細ログ(commands::app::log_frontend_event)だけは
                        // Debugレベルで送っている(Backstage UIには出さずファイルにだけ残すため)。
                        // 全体をDebugにすると依存クレートのログまで大量に混ざるので target 限定で緩める。
                        .level_for("frontend", log::LevelFilter::Debug)
                        // Misskey APIアクセスログ(Issue #241、api/client.rs::post)
                        .level_for("api", log::LevelFilter::Debug)
                        // Streaming受信時のフィルタ/ミュートdropログ(Issue #241、stream/connection.rs)
                        .level_for("filter", log::LevelFilter::Debug)
                        .build(),
                )?;
            }
```

- [ ] **Step 2: ビルドが通ることを確認する**

Run: `cd src-tauri && cargo build`
Expected: 成功

- [ ] **Step 3: 既存テストスイート全体を実行する**

Run: `cd src-tauri && cargo test`
Expected: 全件PASS

- [ ] **Step 4: コミット**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: devビルドでは設定に関係なく常にロガーを登録する"
```

---

## Task 6: Streaming受信時のフィルタ/ミュートdropログ

**Files:**
- Modify: `src-tauri/src/stream/connection.rs`(748〜764行目付近)

**Interfaces:**
- Consumes: Task 5でlevel_for登録された`target: "filter"`(このタスク単体でもコンパイル・動作はする。level_forが無くても`log`クレートのデフォルトフィルタで出力される場合があるが、実際にBackstage/ファイルへ意図通り出るのはTask 5完了後)
- Produces: ライブStreaming受信でノートがフィルタ/ミュートによりdropされた際、`log::debug!(target: "filter", ...)`が1行出る

- [ ] **Step 1: drop分岐にログを追加する**

`src-tauri/src/stream/connection.rs`の以下のブロック(748〜764行目付近):

```rust
            let id = note.id.clone();
            let normalized: Note = (*note).into();
            // フィルタを通過しないノートは出さない（キャッシュ・キャプチャもしない）
            if !filter.matches(&normalized, ctx) {
                return HandleResult::None;
            }
            if let Some(state) = app.try_state::<AppState>() {
                if crate::filter::mute::is_muted(&normalized, &state.mute.lock().unwrap()) {
                    return HandleResult::None;
                }
                if is_server_muted_note(&state, account_id, &normalized) {
                    return HandleResult::None;
                }
                if state.is_word_muted(account_id, &normalized) {
                    return HandleResult::None;
                }
                let _ = state.cache.cache_note(&column_id, &normalized).await;
            }
```

を以下に置き換える(各早期returnの直前にログを1行追加する。ログは`note.id`と`column_id`のみを含み、本文は出さない):

```rust
            let id = note.id.clone();
            let normalized: Note = (*note).into();
            // フィルタを通過しないノートは出さない（キャッシュ・キャプチャもしない）
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
                let _ = state.cache.cache_note(&column_id, &normalized).await;
            }
```

- [ ] **Step 2: ビルドが通ることを確認する**

Run: `cd src-tauri && cargo build`
Expected: 成功

- [ ] **Step 3: streamモジュールの既存テストを実行し、無回帰を確認する**

Run: `cd src-tauri && cargo test --lib stream::`
Expected: 全件PASS

- [ ] **Step 4: コミット**

```bash
git add src-tauri/src/stream/connection.rs
git commit -m "feat: Streaming受信時のフィルタ/ミュートdropログを追加"
```

---

## Task 7: Backstage「メトリクス」タブ(フロントエンド)

**Files:**
- Create: `frontend/src/lib/debugMetrics.ts`
- Create: `frontend/src/lib/debugMetrics.test.ts`
- Modify: `frontend/src/ui/Backstage.svelte`

**Interfaces:**
- Consumes: Task 4の`commands.getDebugMetrics(): Promise<Result<DebugMetrics, Error>>`、型`DebugMetrics`(`backfillCacheHit` / `backfillCacheFallbackBoundary` / `backfillCacheFallbackOther` / `resumeCacheHit` / `resumeCacheFallback`。specta camelCase化後の名前)、`unwrap`(`frontend/src/lib/ipc.ts`から)
- Produces: `export function formatHitRate(hit: number, fallbackA: number, fallbackB: number): string`(Task内でのみ使用)

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/debugMetrics.test.ts`を新規作成:

```typescript
import { describe, expect, it } from "vitest";
import { formatHitRate } from "./debugMetrics";

describe("formatHitRate", () => {
  it("試行が0件のときはダッシュを返す", () => {
    expect(formatHitRate(0, 0, 0)).toBe("—");
  });

  it("hitとfallbackの合計に対するhitの割合を四捨五入したパーセントで返す", () => {
    expect(formatHitRate(3, 1, 0)).toBe("75%");
    expect(formatHitRate(1, 1, 1)).toBe("33%");
    expect(formatHitRate(0, 1, 0)).toBe("0%");
  });
});
```

- [ ] **Step 2: テストを実行し、失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/lib/debugMetrics.test.ts`
Expected: FAIL(`Failed to resolve import "./debugMetrics"`)

- [ ] **Step 3: `formatHitRate`を実装する**

`frontend/src/lib/debugMetrics.ts`を新規作成:

```typescript
/** キャッシュhit率を表示用文字列にする。試行が0件なら"—"。Issue #241。 */
export function formatHitRate(hit: number, fallbackA: number, fallbackB: number): string {
  const total = hit + fallbackA + fallbackB;
  if (total === 0) return "—";
  return `${Math.round((hit / total) * 100)}%`;
}
```

- [ ] **Step 4: テストを実行し、成功することを確認する**

Run: `cd frontend && pnpm vitest run src/lib/debugMetrics.test.ts`
Expected: PASS(2件とも)

- [ ] **Step 5: `Backstage.svelte`にメトリクスタブを追加する**

`frontend/src/ui/Backstage.svelte`の`<script>`ブロックを変更する。まずimportを追加(既存の`import { Circle, ... } from "@lucide/svelte";`の下、`import { Button } ...`の下に追加):

```typescript
  import { commands, unwrap } from "../lib/ipc";
  import { formatHitRate } from "../lib/debugMetrics";
  import type { DebugMetrics } from "../bindings/tauri.gen";
```

既存の`let open = $state(false);`の直後に状態を追加:

```typescript
  let panelView = $state<"log" | "metrics">("log");
  let metrics = $state<DebugMetrics | null>(null);

  // メトリクスタブを開いている間だけポーリングする(常時ポーリングはしない)
  $effect(() => {
    if (!open || panelView !== "metrics") return;
    let cancelled = false;
    async function poll() {
      try {
        const m = await unwrap(commands.getDebugMetrics());
        if (!cancelled) metrics = m;
      } catch {
        // 補助情報なので失敗してもログには出さず静かに諦める
      }
    }
    poll();
    const id = setInterval(poll, 3000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  });
```

次にマークアップを変更する。既存の展開パネル部分:

```svelte
  {#if open}
    <div class="h-[min(38vh,320px)] overflow-y-auto border-b border-border bg-background font-mono text-xs">
      {#if app.logs.length === 0}
        <div class="p-3.5 text-center text-muted-foreground">ログはまだありません</div>
      {:else}
        {#each app.logs as l (l.id)}
          ...
        {/each}
      {/if}
    </div>
  {/if}
```

を以下に置き換える(タブ切り替えボタンを追加し、`app.logs`のループはそのまま`{#if panelView === "log"}`の中に移すだけで、中身は変更しない):

```svelte
  {#if open}
    <div class="flex gap-1 border-b border-border bg-card px-2.5 pt-1.5">
      <Button
        variant={panelView === "log" ? "secondary" : "ghost"}
        size="xs"
        onclick={() => (panelView = "log")}
      >ログ</Button>
      <Button
        variant={panelView === "metrics" ? "secondary" : "ghost"}
        size="xs"
        onclick={() => (panelView = "metrics")}
      >メトリクス</Button>
    </div>
    <div class="h-[min(38vh,320px)] overflow-y-auto border-b border-border bg-background font-mono text-xs">
      {#if panelView === "log"}
        {#if app.logs.length === 0}
          <div class="p-3.5 text-center text-muted-foreground">ログはまだありません</div>
        {:else}
          {#each app.logs as l (l.id)}
            {@const Ic = icon[l.level]}
            <div class="flex items-baseline gap-2 px-2.5 py-0.5 hover:bg-card" data-level={l.level}>
              <span
                class={[
                  "inline-flex flex-none",
                  {
                    "text-[var(--success)]": l.level === "success",
                    "text-[var(--warning)]": l.level === "warn",
                    "text-destructive": l.level === "error",
                    "text-muted-foreground": l.level === "info",
                  },
                ]}
              ><Ic size={12} /></span>
              <span class="flex-none text-muted-foreground">{hhmmss(l.at)}</span>
              <span class="flex-1 break-words">{l.text}</span>
              {#if l.reauthAccountId}
                <Button variant="outline" size="xs" onclick={() => onReauth(l.reauthAccountId!)}>再認証</Button>
              {/if}
            </div>
          {/each}
        {/if}
      {:else if !metrics}
        <div class="p-3.5 text-center text-muted-foreground">読み込み中…</div>
      {:else}
        <div class="flex flex-col gap-1 p-2.5">
          <div>backfill キャッシュhit率: {formatHitRate(metrics.backfillCacheHit, metrics.backfillCacheFallbackBoundary, metrics.backfillCacheFallbackOther)}</div>
          <div>backfill hit: {metrics.backfillCacheHit}</div>
          <div>backfill fallback(境界未確定): {metrics.backfillCacheFallbackBoundary}</div>
          <div>backfill fallback(その他): {metrics.backfillCacheFallbackOther}</div>
          <div>resume hit: {metrics.resumeCacheHit}</div>
          <div>resume fallback: {metrics.resumeCacheFallback}</div>
        </div>
      {/if}
    </div>
  {/if}
```

- [ ] **Step 6: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー無し

- [ ] **Step 7: フロントエンドのテストスイート全体を実行する**

Run: `cd frontend && pnpm test`
Expected: 全件PASS

- [ ] **Step 8: コミット**

```bash
git add frontend/src/lib/debugMetrics.ts frontend/src/lib/debugMetrics.test.ts frontend/src/ui/Backstage.svelte
git commit -m "feat: BackstageにキャッシュメトリクスのUIを追加"
```

---

## Task 8: 最終手動検証

**Files:** なし(検証のみ)

**Interfaces:**
- Consumes: Task 1〜7で実装した全機能
- Produces: なし(検証結果の確認のみ)

- [ ] **Step 1: Rust側テストスイート全体を実行する**

Run: `cd src-tauri && cargo test`
Expected: 全件PASS

- [ ] **Step 2: フロントエンド側の型チェック・テストを実行する**

Run: `cd frontend && pnpm check && pnpm test`
Expected: 両方エラー無し・全件PASS

- [ ] **Step 3: Xvfb越しに`cargo tauri dev`を起動し、devビルドでログが出ることを確認する**

実画面に影響させないため、Xvfb越しで起動する(既存メモリのfeedback-dev-server-verification-must-use-virtual-display方針)。

Run(リポジトリルートから、バックグラウンド起動):
```bash
xvfb-run -a cargo tauri dev
```

- ターミナルに`log::info!`/`log::debug!`由来のログ(起動時の設定読み込み等)が、`enable_file_logging`設定をONにしなくても出力されていることを確認する
- アプリでアカウントにログインし、いずれかのカラムを開いて上スクロール(backfill)し、ターミナルまたはBackstageの「ログ」に`target: "api"`のアクセスログ(`notes/... -> 200 (...bytes, ...ms)`のような行)が出ることを確認する
- Backstageを開き「メトリクス」タブに切り替え、`backfill hit`等の数値が0以外に増えることを確認する(境界未確定期間はfallbackが増えることも確認できるとなお良い)
- ミュートまたはTQLフィルタを設定したカラムでStreamingを受信させ、ターミナルに`target: "filter"`のdropログが出ることを確認する

- [ ] **Step 4: 起動した`cargo tauri dev`を終了する**

自分で起動した検証用プロセスなので、完了前に自分でkillする(正確なPIDを`ps aux`等で特定し、`pkill`/`killall`は使わない)。

- [ ] **Step 5: 最終確認**

Run: `cd src-tauri && git status && cd .. && git log --oneline feature/241-debug-logging-observability -10`
Expected: 作業ツリーがクリーンで、Task 1〜7の各コミットが順に並んでいる

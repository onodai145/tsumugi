# リアクション受け入れ設定 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** 投稿バーから Misskey の `notes/create` `reactionAcceptance`（全て/いいねのみ/全て(リモートはいいねのみ)/非センシティブのみ/非センシティブのみ(リモートはいいねのみ)）を選べるようにする(Issue #169)。ラベル文言は Misskey 本家 `locales/ja-JP.yml` に揃える。

**Architecture:** `src-tauri/src/api/notes.rs` の `NoteDraft` に `reaction_acceptance: Option<ReactionAcceptanceInput>` を追加し、既定(`All`)は `skip_serializing_if` でフィールドごと省略して Misskey 側のデフォルト(`null`＝全員)に委ねる。フロントは `VisibilitySelect.svelte` と同じ構造の新規 `ReactionAcceptanceSelect.svelte` を追加し、`ComposeBar.svelte` の CW/投票/チャンネル ボタン列に配置する。表示側(`NoteCard` 等)への反映は行わない(投稿時のみの設定)。

**Tech Stack:** Rust (serde, specta), Svelte 5 (runes), 既存の `tauri-specta` バインディング生成。

## Global Constraints

- `docs/superpowers/specs/2026-08-15-reaction-acceptance-design.md` の設計に従う。
- アカウント単位の既定値は Misskey 側に存在しないため対象外(投稿ごとの設定のみ)。
- 投稿後のノート表示(NoteCard等)への反映は対象外。

---

### Task 1: バックエンドに `reactionAcceptance` を追加(TDD)

**Files:**
- Modify: `src-tauri/src/api/notes.rs`(`NoteDraft` へのフィールド追加、`ReactionAcceptanceInput` enum 新設、ユニットテスト追加)

**Interfaces:**
- Consumes: なし(新規 enum)。
- Produces: `NoteDraft.reaction_acceptance: Option<ReactionAcceptanceInput>`。`create_note`/`post_note` コマンドは `NoteDraft` をそのまま Misskey `notes/create` へ渡すため、追加の配線は不要。

- [x] **Step 1: 失敗するテストを先に書く**

`src-tauri/src/api/notes.rs` の `poll_input_serializes` の直前に以下を追加:

```rust
#[test]
fn reaction_acceptance_omitted_by_default() {
    let d = NoteDraft {
        text: Some("hi".into()),
        visibility: VisibilityInput::Public,
        ..Default::default()
    };
    let v = serde_json::to_value(&d).unwrap();
    assert!(v.get("reactionAcceptance").is_none());
}

#[test]
fn reaction_acceptance_serializes_non_default_choice() {
    let d = NoteDraft {
        text: Some("hi".into()),
        visibility: VisibilityInput::Public,
        reaction_acceptance: Some(ReactionAcceptanceInput::LikeOnly),
        ..Default::default()
    };
    let v = serde_json::to_value(&d).unwrap();
    assert_eq!(v["reactionAcceptance"], "likeOnly");
}
```

Run: `cd src-tauri && cargo test --lib api::notes::tests::reaction_acceptance`
Expected: コンパイルエラー(`NoteDraft` に該当フィールドが無い / `ReactionAcceptanceInput` が未定義)。

- [x] **Step 2: `NoteDraft` にフィールドを追加**

`NoteDraft` 構造体の `local_only` フィールドの直後に追加:

```rust
    /// `All`（既定）はフィールドごと省略し、Misskey 側のデフォルト（`null`＝全員）に委ねる。
    #[serde(skip_serializing_if = "reaction_acceptance_is_default")]
    pub reaction_acceptance: Option<ReactionAcceptanceInput>,
}

fn reaction_acceptance_is_default(v: &Option<ReactionAcceptanceInput>) -> bool {
    matches!(v, None | Some(ReactionAcceptanceInput::All))
}
```

- [x] **Step 3: `ReactionAcceptanceInput` enum を追加**

`VisibilityInput` enum の直後に追加:

```rust
/// `notes/create` の `reactionAcceptance`。`All` は送信時 `null` 相当（フィールド省略）として扱う。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ReactionAcceptanceInput {
    #[default]
    All,
    LikeOnly,
    LikeOnlyForRemote,
    NonSensitiveOnly,
    NonSensitiveOnlyForLocalLikeOnlyForRemote,
}
```

- [x] **Step 4: テストを再実行しGREENを確認**

Run: `cd src-tauri && cargo test --lib api::notes::tests`
Expected: 追加した2件を含め全テストが通過する。

- [x] **Step 5: バインディングを再生成**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: `frontend/src/bindings/tauri.gen.ts` に `ReactionAcceptanceInput` 型と `NoteDraft.reactionAcceptance` フィールドが出力される。

---

### Task 2: 投稿バーに選択UIを追加

**Files:**
- Create: `frontend/src/ui/ReactionAcceptanceSelect.svelte`
- Modify: `frontend/src/ui/ComposeBar.svelte`(import・state・draft組み立て・リセット・配置)

**Interfaces:**
- Consumes: `ReactionAcceptanceInput`(生成済みバインディング)。
- Produces: `ReactionAcceptanceSelect` は `value: ReactionAcceptanceInput`(bindable)を公開する `VisibilitySelect.svelte` 相当のコンポーネント。他ファイルからの新規参照は `ComposeBar.svelte` のみ。

- [x] **Step 1: `ReactionAcceptanceSelect.svelte` を作成**

`VisibilitySelect.svelte` と同じ portal メニュー構造で、Misskey 本家 `locales/ja-JP.yml` に揃えた5択(全て/いいねのみ/全て(リモートはいいねのみ)/非センシティブのみ/非センシティブのみ(リモートはいいねのみ))のラベルを持つコンポーネントを作成する。

- [x] **Step 2: `ComposeBar.svelte` に state を追加**

```svelte
let reactionAcceptance = $state<ReactionAcceptanceInput>("all");
```

`localOnly` state の直後に配置。`import type { ReactionAcceptanceInput }` を型 import 群へ追加。`import ReactionAcceptanceSelect from "./ReactionAcceptanceSelect.svelte";` をコンポーネント import 群へ追加。

- [x] **Step 3: 送信時に `NoteDraft` へ渡す**

`submit()` 内の `draft` オブジェクト構築で `localOnly` の直後に追加:

```svelte
reactionAcceptance,
```

(Rust側の `skip_serializing_if` が `All`/`None` を自動的に省略するため、フロント側で `null` へ変換する処理は不要。)

- [x] **Step 4: 投稿完了後にリセット**

`submit()` 成功時のリセット処理(`localOnly = false;` の直後)に追加:

```svelte
reactionAcceptance = "all";
```

- [x] **Step 5: ボタン列に配置**

投稿バーの「チャンネル」ボタンの直後に追加:

```svelte
<ReactionAcceptanceSelect bind:value={reactionAcceptance} />
```

- [x] **Step 6: 型チェック**

Run: `cd frontend && pnpm check`
Expected: エラーなしで終了する。

---

### Task 3: 仕上げ・検証・コミット

- [x] **Step 1: `/simplify` でレビュー**

再利用・簡略化・効率・抽象度の4観点でレビューし、フロント側の冗長な `"all" → null` 変換(Rust側で既に処理済み)を除去。`Dropdown.svelte` への一本化提案は `VisibilitySelect.svelte` という既存の兄弟実装との一貫性を優先し、スコープ外として見送り。

- [x] **Step 2: 全テストを実行**

Run: `cd src-tauri && cargo test --lib`
Expected: 全件通過(201 passed)。

Run: `cd frontend && pnpm check`
Expected: エラーなし。

- [x] **Step 3: コミット・PR作成**

```bash
git add -A
git commit -m "feat: 投稿バーにリアクション受け入れ設定を追加"
git push -u origin feat/issue-169-reaction-acceptance
gh pr create --title "feat: 投稿バーにリアクション受け入れ設定を追加" --body "Fixes #169 ..."
```

- [x] **Step 4: ラベル文言を Misskey 本家に揃える**

`ReactionAcceptanceSelect.svelte` の `OPTIONS` ラベルを独自の日本語訳から Misskey 本家 `locales/ja-JP.yml` の文言(`全て`/`いいねのみ`/`全て (リモートはいいねのみ)`/`非センシティブのみ`/`非センシティブのみ (リモートはいいねのみ)`)へ差し替え。本家に説明文(desc)は無いため、メニュー項目も `Dropdown.svelte` と同じ単一行表示に簡略化。

```bash
git add frontend/src/ui/ReactionAcceptanceSelect.svelte docs/superpowers/specs/2026-08-15-reaction-acceptance-design.md docs/superpowers/plans/2026-08-15-reaction-acceptance.md
git commit -m "fix: リアクション受け入れ選択肢の文言をMisskey本家に揃える"
git push
```

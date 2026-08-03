# 絵文字ピッカー使用履歴 (Issue #108) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ノートへのリアクションで使った絵文字を最大16件、最新順で記録し、絵文字ピッカーの「ピン留め」セクションの上に「最近使った」として表示する。

**Architecture:** `UiPrefs`（Rust側、SQLite永続化）に`recent_emojis: Vec<String>`を追加し、`pinned_emojis`と同じ仕組みで保存する。フロントでは新規の純粋関数モジュール`lib/recentEmojis.ts`が「使用時に既存キーを除去して先頭へ追加、16件に切り詰め」を計算し、`store.svelte.ts`の`recordEmojiUsage()`がそれを`setUiPrefs`経由で永続化する。記録は`NoteCard.svelte`の`react()`からのみ呼ぶ（設定画面のピン留め選択では呼ばない）。`ReactionPicker.svelte`は`pinnedEntries`と同じキー解決ロジックを共通ヘルパーに抜き出し、「最近使った」セクションで再利用する。

**Tech Stack:** Rust (serde, rusqlite経由でUiPrefs全体をJSONとして保存), TypeScript/Svelte 5 (runes), Vitest, `cargo test`（フロントバインディング再生成）。

## Global Constraints

- カスタム絵文字の使用履歴キーは`:name@host:`形式（`customEmojiPinKey`）で保存する。`pinned_emojis`と同じグローバル設定であり、複数インスタンスのアカウント間で絵文字名が衝突しないようにするため。
- `ReactionPicker`の`onpick`が渡す生の値はホスト省略の自インスタンス形式`:name@.:`（`customEmojiKey`）なので、記録前に必ず変換すること。
- 上限は16件。重複除去は「同一キーを除去してから先頭に追加」で行う（タイムスタンプは持たない）。
- 記録対象は`NoteCard.svelte`の`react()`のみ。`ReactionSection.svelte`（`showPinned={false}`）や`ComposeBar.svelte`の`:emoji:`補完からは呼ばない。
- 新規Rustフィールドは`#[serde(default)]`にして既存ユーザーの設定ファイルとの後方互換を保つ。

---

### Task 1: `UiPrefs`に`recent_emojis`フィールドを追加

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`

**Interfaces:**
- Produces: `UiPrefs.recent_emojis: Vec<String>`（serde camelCase変換で`recentEmojis`としてTSへ露出）。デフォルトは空`Vec`。

- [ ] **Step 1: フィールドと`Default`実装を追加**

`pinned_emojis`フィールドの直後（`src-tauri/src/domain/ui.rs`、`struct UiPrefs`定義内、112〜115行目付近）に追加：

```rust
    /// リアクションピッカーで最近使った絵文字（Issue #108）。キー形式は pinned_emojis と同じ
    /// （Unicode絵文字はそのまま、カスタム絵文字は ":name@host:" 形式）。先頭が最新。
    #[serde(default)]
    pub recent_emojis: Vec<String>,
```

`impl Default for UiPrefs`（`pinned_emojis: default_pinned_emojis(),`の直後）に追加：

```rust
            recent_emojis: Vec::new(),
```

- [ ] **Step 2: 既存テストに`recent_emojis`の後方互換アサーションを追加**

`mod tests`内の`deserializes_legacy_json_without_new_fields`テスト（`pinned_emojis`のアサーション直後）に追加：

```rust
        // recent_emojis も同様に既定値(追加前は履歴なし)へフォールバックすること。
        assert!(v.recent_emojis.is_empty());
```

`roundtrips_keymap`テストの`UiPrefs`構築リテラル（`pinned_emojis: vec!["👍".into(), ":blob_cat:".into()],`の直後）に追加：

```rust
            recent_emojis: vec![":blob_cat@misskey.io:".into(), "😆".into()],
```

- [ ] **Step 3: テストを実行して確認**

Run: `cd src-tauri && cargo test domain::ui::tests`
Expected: PASS（`deserializes_legacy_json_without_new_fields`と`roundtrips_keymap`を含む全テストが通る）

- [ ] **Step 4: フロント向けTSバインディングを再生成**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts`の`UiPrefs`型に`recentEmojis?: string[]`が追加されていることを確認する:

Run: `grep -n "recentEmojis" ../frontend/src/bindings/tauri.gen.ts`
Expected: `recentEmojis?: string[],`が出力される

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain/ui.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: UiPrefsに絵文字使用履歴フィールドを追加"
```

---

### Task 2: フロント純粋ヘルパー`lib/recentEmojis.ts`（使用履歴リストの更新ロジック）

**Files:**
- Create: `frontend/src/lib/recentEmojis.ts`
- Test: `frontend/src/lib/recentEmojis.test.ts`

**Interfaces:**
- Consumes: なし（純粋関数、外部依存なし）
- Produces:
  - `export const RECENT_EMOJIS_MAX = 16`
  - `export function withRecentEmojiUsage(list: string[], key: string): string[]` — Task 3で`store.svelte.ts`の`recordEmojiUsage`が使う。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/recentEmojis.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { RECENT_EMOJIS_MAX, withRecentEmojiUsage } from "./recentEmojis";

describe("withRecentEmojiUsage", () => {
  it("prepends a new key to an empty list", () => {
    expect(withRecentEmojiUsage([], "👍")).toEqual(["👍"]);
  });

  it("moves an existing key to the front instead of duplicating it", () => {
    expect(withRecentEmojiUsage(["😆", "👍", "🎉"], "👍")).toEqual(["👍", "😆", "🎉"]);
  });

  it("truncates to RECENT_EMOJIS_MAX entries", () => {
    const list = Array.from({ length: RECENT_EMOJIS_MAX }, (_, i) => `emoji-${i}`);
    const result = withRecentEmojiUsage(list, "new-emoji");
    expect(result).toHaveLength(RECENT_EMOJIS_MAX);
    expect(result[0]).toBe("new-emoji");
    expect(result).not.toContain(`emoji-${RECENT_EMOJIS_MAX - 1}`);
  });

  it("re-adding the same key keeps the list length stable", () => {
    const list = ["👍", "😆"];
    expect(withRecentEmojiUsage(list, "👍")).toEqual(["👍", "😆"]);
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cd frontend && pnpm vitest run src/lib/recentEmojis.test.ts`
Expected: FAIL（`./recentEmojis`モジュールが存在しない）

- [ ] **Step 3: 実装を書く**

`frontend/src/lib/recentEmojis.ts`:

```ts
// リアクションピッカーの使用履歴（Issue #108）。キー形式は pinnedEmojis と同じ
// （Unicode絵文字はそのまま、カスタム絵文字は ":name@host:" 形式）。
export const RECENT_EMOJIS_MAX = 16;

// 使用のたびに呼ぶ。既存の同一キーを除去してから先頭に追加し、最大件数に切り詰める
// (タイムスタンプは持たず、配列の並び順で最新度を表す)。
export function withRecentEmojiUsage(list: string[], key: string): string[] {
  return [key, ...list.filter((k) => k !== key)].slice(0, RECENT_EMOJIS_MAX);
}
```

- [ ] **Step 4: テストを実行して成功を確認**

Run: `cd frontend && pnpm vitest run src/lib/recentEmojis.test.ts`
Expected: PASS（4件全て）

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/recentEmojis.ts frontend/src/lib/recentEmojis.test.ts
git commit -m "feat: 絵文字使用履歴リスト更新の純粋関数を追加"
```

---

### Task 3: `store.svelte.ts`に`recordEmojiUsage`を追加し、`recentEmojis`の既定値配線を整える

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: `withRecentEmojiUsage(list: string[], key: string): string[]`（Task 2, `./recentEmojis`）
- Produces: `app.recordEmojiUsage(key: string): Promise<void>`（Task 4で`NoteCard.svelte`が呼ぶ）。`app.ui.recentEmojis: string[] | undefined`（`ReactionPicker.svelte`が`?? []`で読む）。

- [ ] **Step 1: import追加**

`frontend/src/lib/store.svelte.ts`の先頭付近、既存の`import { ... } from "./unicodeEmojiList";`などが並ぶ箇所に追加：

```ts
import { withRecentEmojiUsage } from "./recentEmojis";
```

- [ ] **Step 2: ローカル既定値オブジェクトに`recentEmojis`を追加**

`ui = $state<UiPrefs>({ ... pinnedEmojis: DEFAULT_PINNED_EMOJIS, ... })`の`pinnedEmojis`行の直後（125〜146行目付近）に追加：

```ts
    recentEmojis: [],
```

- [ ] **Step 3: `boot()`内の`setUiPrefs`相当の初期化には手を加えない**

`boot()`（196行目付近）の`this.ui = { ...ui, ... }`ブロックには`pinnedEmojis`のフォールバックが元々含まれていない（各利用箇所で`?? DEFAULT_PINNED_EMOJIS`する既存方針）。`recentEmojis`も同じ方針に合わせ、ここには追加しない。

- [ ] **Step 4: `setUiPrefs`内のフォールバックに`recentEmojis`を追加**

`async setUiPrefs(prefs: UiPrefs)`内、`pinnedEmojis: prefs.pinnedEmojis ?? DEFAULT_PINNED_EMOJIS,`の直後（1098行目付近）に追加：

```ts
      recentEmojis: prefs.recentEmojis ?? [],
```

- [ ] **Step 5: `recordEmojiUsage`メソッドを追加**

`setPinnedEmojis`メソッド（1156〜1161行目付近）の直後に追加：

```ts
  /// リアクションピッカーで絵文字を使ったことを記録する（Issue #108）。
  /// キーは pinnedEmojis と同じ形式（カスタム絵文字はホスト付き ":name@host:"）で渡すこと。
  async recordEmojiUsage(key: string) {
    const list = withRecentEmojiUsage(this.ui.recentEmojis ?? [], key);
    await unwrap(commands.setUiPrefs({ ...this.ui, recentEmojis: list }));
    this.ui = { ...this.ui, recentEmojis: list };
  }
```

- [ ] **Step 6: 型チェックを実行**

Run: `cd frontend && pnpm check`
Expected: PASS（エラーなし）

- [ ] **Step 7: Commit**

```bash
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: storeに絵文字使用履歴の記録メソッドを追加"
```

---

### Task 4: `NoteCard.svelte`の`react()`から使用履歴を記録する

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte`

**Interfaces:**
- Consumes: `app.recordEmojiUsage(key: string): Promise<void>`（Task 3）、`isCustomEmojiKey`/`customEmojiPinKey`/`parseCustomEmojiPinKey`（`../lib/emojiKey`、既存）

- [ ] **Step 1: import追加**

`frontend/src/ui/NoteCard.svelte`の先頭、既存の`import { reactionEmoji, isRemoteCustomEmoji, proxiedEmojiMap } from "../lib/emoji";`の直後（15行目）に追加：

```ts
  import { isCustomEmojiKey, customEmojiPinKey, parseCustomEmojiPinKey } from "../lib/emojiKey";
```

- [ ] **Step 2: `react()`を変更**

96〜99行目の`function react(reaction: string) { ... }`を置き換え：

```ts
  function react(reaction: string) {
    app.reactPicker = null;
    if (accountId) {
      app.toggleReaction(accountId, inner.id, reaction);
      const host = app.accounts.find((a) => a.id === accountId)?.host;
      const stored =
        isCustomEmojiKey(reaction) && host
          ? customEmojiPinKey(parseCustomEmojiPinKey(reaction).name, host)
          : reaction;
      void app.recordEmojiUsage(stored);
    }
  }
```

- [ ] **Step 3: 型チェックを実行**

Run: `cd frontend && pnpm check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add frontend/src/ui/NoteCard.svelte
git commit -m "feat: ノートへのリアクションで絵文字使用履歴を記録"
```

---

### Task 5: `ReactionPicker.svelte`に「最近使った」セクションを表示する

**Files:**
- Modify: `frontend/src/input/ReactionPicker.svelte`

**Interfaces:**
- Consumes: `app.ui.recentEmojis: string[] | undefined`（Task 3）

- [ ] **Step 1: キー解決ロジックを共通ヘルパー関数に抜き出す**

`ReactionPicker.svelte`の`<script>`内、`pinnedEntries`の`$derived`定義（30〜42行目）を、共通関数呼び出しに置き換える。まず`accountHost`定義の直後に関数を追加：

```ts
  // ピン留め/使用履歴どちらも同じキー形式(Unicode文字 or ":name@host:")で保持するため、
  // 描画用の {char} | {name,url} への解決ロジックを共通化する。
  // カスタム絵文字は保存元インスタンス(host)が今開いているアカウントと一致する場合のみ解決する
  // (複数インスタンスのアカウントを使っている場合、同名だが別絵文字を誤って出すのを防ぐ)。
  // 未解決(host不一致・削除済み等)は表示から除外する。
  function resolveEmojiEntries(keys: string[]): { key: string; custom: EmojiDef | null }[] {
    return keys
      .map((key) => {
        if (isCustomEmojiKey(key)) {
          const { name, host } = parseCustomEmojiPinKey(key);
          if (host !== null && host !== accountHost) return null;
          const def = customEmojis.find((e) => e.name === name);
          return def ? { key, custom: def } : null;
        }
        return { key, custom: null as EmojiDef | null };
      })
      .filter((e): e is { key: string; custom: EmojiDef | null } => e !== null);
  }

  const pinnedEntries = $derived(resolveEmojiEntries(pinned));

  // ピン留め済みの絵文字は「最近使った」に重複表示しない。
  const recentEntries = $derived(
    resolveEmojiEntries((app.ui.recentEmojis ?? []).filter((key) => !pinned.includes(key))),
  );
```

削除する元の`pinnedEntries`定義（30〜42行目）:

```ts
  const pinnedEntries = $derived(
    pinned
      .map((key) => {
        if (isCustomEmojiKey(key)) {
          const { name, host } = parseCustomEmojiPinKey(key);
          if (host !== null && host !== accountHost) return null;
          const def = customEmojis.find((e) => e.name === name);
          return def ? { key, custom: def } : null;
        }
        return { key, custom: null as EmojiDef | null };
      })
      .filter((e): e is { key: string; custom: EmojiDef | null } => e !== null),
  );
```

- [ ] **Step 2: マークアップに「最近使った」セクションを追加**

`<section class="section">`（「ピン留め」セクション、`{#if showPinned}`ブロック内）の直前に追加：

```svelte
      {#if showPinned && recentEntries.length > 0}
        <section class="section">
          <h4 class="section-title">最近使った</h4>
          <div class="flat-grid">
            {#each recentEntries as e (e.key)}
              <button class="emoji-btn" title={e.key} onclick={() => onpick(reactionKeyOf(e))}>
                {#if e.custom}
                  <img src={e.custom.url} alt={e.key} loading="lazy" />
                {:else}
                  <UnicodeEmoji char={e.key} />
                {/if}
              </button>
            {/each}
          </div>
        </section>
      {/if}
```

（元の`{#if showPinned}` ... `{/if}`ブロックはそのまま残す。「最近使った」は独立した`{#if showPinned && recentEntries.length > 0}`ブロックとして、その直前に置く。0件時はセクションごと非表示。）

- [ ] **Step 3: 型チェックを実行**

Run: `cd frontend && pnpm check`
Expected: PASS

- [ ] **Step 4: 動作確認（手動）**

Run: `cargo tauri dev`

1. 適当なノートにリアクションする（Unicode絵文字とカスタム絵文字それぞれ最低1回）
2. 別のノートで絵文字ピッカーを開き、「ピン留め」の上に「最近使った」セクションが出て、直前にリアクションした絵文字が先頭に表示されることを確認
3. 既にピン留め済みの絵文字にリアクションしても「最近使った」に重複表示されないことを確認
4. 設定 → リアクション → 「＋」（ピン留め追加、`showPinned=false`）を開き、「最近使った」セクションが出ないことを確認
5. アプリを再起動し、履歴が永続化されていることを確認

Expected: 上記すべて期待通りに動作する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/input/ReactionPicker.svelte
git commit -m "feat: 絵文字ピッカーに最近使った絵文字セクションを追加"
```

---

## Self-Review Notes

- **Spec coverage:** データモデル(Task 1)、フロント永続化(Task 2,3)、記録箇所の限定(Task 4がNoteCardのみ、ReactionSection/ComposeBarには触れない)、表示(Task 5、ピン留め上・除外・0件非表示・検索中非表示は既存の`{#if queryLower}`分岐の外側=`{:else}`側にすべて収まっているため追加対応不要)を全カバー。
- **プレースホルダー:** なし。全ステップに実コードを記載。
- **型整合性:** `withRecentEmojiUsage(list: string[], key: string): string[]`（Task2定義）→ `recordEmojiUsage(key: string)`（Task3、`this.ui.recentEmojis ?? []`を渡す）→ `app.recordEmojiUsage(stored)`（Task4呼び出し）→ `app.ui.recentEmojis`（Task5参照）で一貫。`resolveEmojiEntries`の戻り値型`{ key: string; custom: EmojiDef | null }[]`は既存の`reactionKeyOf`のパラメータ型と一致。

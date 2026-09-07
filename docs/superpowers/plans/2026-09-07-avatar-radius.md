# アバター角丸カスタマイズ Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ユーザーがアバター画像の角丸（0%=直角〜100%=真円）を設定画面から変更できるようにする（Issue #94）。

**Architecture:** `UiPrefs`（Rust側 `src-tauri/src/domain/ui.rs`）に `avatar_radius: i32`（0〜100、既定20）を追加し、フロントの `store.svelte.ts` がロード/保存のたびに `--avatar-radius` CSS変数へ反映する。アプリ内7ファイルのアバター表示（画像・プレースホルダ双方）の `rounded-md`/`rounded-lg` を `rounded-[var(--avatar-radius,20%)]` に置き換え、単一の設定値で全アバターの見た目を統一的に変更できるようにする。設定UIは `AppearanceSection.svelte` に range スライダー＋プレビューを追加する。

**Tech Stack:** Rust (serde, specta), Svelte 5 (runes), Tailwind v4（任意値クラス `[var(--x,fallback)]`）, Vitest, cargo test。

## Global Constraints

- 角丸の適用範囲: アプリ内の全アバター表示（ノート・アカウント切替・プロフィール・フォロー一覧・リアクション一覧・通知・設定のアカウント一覧）を1つの設定値で一括変更する。文脈ごとの個別設定は行わない。
- 値の形式: 0〜100の整数（%）。0=直角、100=真円。
- 既定値: 20（%）。既存の `rounded-md`（34pxアバターで6px、約17.6%相当）に近い見た目を維持する。
- `specta_builder()` は `UiPrefs` 型全体を export 済みのため、フィールド追加のみで `frontend/src/bindings/tauri.gen.ts` は `cargo test` で自動再生成される。**手で編集しない。**
- コミットメッセージは件名のみ（本文なし）。Co-Authored-By トレーラーは別途自動付与される。

---

### Task 1: Rust `UiPrefs` に `avatar_radius` を追加

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`

**Interfaces:**
- Produces: `UiPrefs.avatar_radius: i32`（serde camelCase で `avatarRadius`）。`default_avatar_radius() -> i32` が既定値 `20` を返す。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/domain/ui.rs` の `mod tests` 内、`deserializes_legacy_json_without_new_fields` テストの末尾（`assert_eq!(v.summaly_proxy_url, "");` の直後）に追記する:

```rust
        // avatar_radius も同様に既定値(20%, Issue #94追加前の rounded-md 相当の見た目)へ
        // フォールバックすること。
        assert_eq!(v.avatar_radius, 20);
```

さらに `mod tests` の末尾（`instance_ticker_defaults_to_remote_for_legacy_json` テストの後）に新規テストを追加する:

```rust
    #[test]
    fn avatar_radius_defaults_to_20_for_legacy_json() {
        // avatar_radius 追加前に保存された JSON も読めること（#[serde(default)]）。
        // 既定は20%(追加前の rounded-md に近い見た目、Issue #94)。
        let v: UiPrefs = serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
        assert_eq!(v.avatar_radius, 20);
    }
```

そして `roundtrips_keymap` テストの `UiPrefs { .. }` リテラル（`instance_ticker: "always".into(),` の直後）に以下のフィールドを追加する:

```rust
            avatar_radius: 65,
```

- [ ] **Step 2: テストを実行し失敗することを確認する**

Run: `cd src-tauri && cargo test avatar_radius`
Expected: FAIL（`avatar_radius` フィールドが存在しない、`UiPrefs` の構造体リテラルに未知フィールドがある等のコンパイルエラー）

- [ ] **Step 3: 最小実装を書く**

`src-tauri/src/domain/ui.rs` の `UiPrefs` 構造体、`instance_ticker` フィールド（190行目付近）の直後に追加する:

```rust
    /// アバター画像の角丸（0=直角 〜 100=真円、%）。既定は20（Issue #94）。
    #[serde(default = "default_avatar_radius")]
    pub avatar_radius: i32,
```

`default_instance_ticker()` 関数（248行目付近）の直後に追加する:

```rust
fn default_avatar_radius() -> i32 {
    20
}
```

`impl Default for UiPrefs` の `instance_ticker: default_instance_ticker(),` の直後に追加する:

```rust
            avatar_radius: default_avatar_radius(),
```

- [ ] **Step 4: テストを実行し成功することを確認する**

Run: `cd src-tauri && cargo test avatar_radius`
Expected: PASS（`avatar_radius_defaults_to_20_for_legacy_json` および既存テストが通る）

- [ ] **Step 5: 全体のRustテストとバインディング再生成を確認する**

Run: `cd src-tauri && cargo test`
Expected: PASS。`generates_frontend_bindings` テストが `frontend/src/bindings/tauri.gen.ts` を再生成し、`avatarRadius: number` フィールドが `UiPrefs` 型に含まれることを確認する:

```bash
grep -n "avatarRadius" ../frontend/src/bindings/tauri.gen.ts
```

Expected: 1件以上ヒットする。

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/domain/ui.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: UiPrefsにavatarRadius(アイコンの丸み)を追加(Issue #94)"
```

---

### Task 2: `store.svelte.ts` に avatarRadius のロード/保存/CSS変数反映を追加

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`
- Test: `frontend/src/lib/store.svelte.test.ts`

**Interfaces:**
- Consumes: `UiPrefs.avatarRadius: number`（Task 1で生成された `tauri.gen.ts` の型）
- Produces: `app.ui.avatarRadius: number`（既定20）。`--avatar-radius` CSS変数（`document.documentElement.style` に `"${pct}%"` 形式でセット、pct は 0〜100 にクランプ済み）。プライベートメソッド `#applyAvatarRadius(pct: number): void`。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.test.ts` の末尾に以下を追加する（既存の `describe("#applyTheme ...")` ブロックと同じ並び、ファイル末尾に新規 `describe` を追加）:

```ts
describe("#applyAvatarRadius (Issue #94: アイコンの丸みカスタマイズ)", () => {
  afterEach(() => {
    document.documentElement.style.removeProperty("--avatar-radius");
  });

  it("setUiPrefsでavatarRadiusを指定すると--avatar-radius CSS変数に反映される", async () => {
    await app.setUiPrefs({ ...app.ui, avatarRadius: 65 });
    expect(document.documentElement.style.getPropertyValue("--avatar-radius")).toBe("65%");
  });

  it("avatarRadiusが範囲外(100超)でも100%にクランプされる", async () => {
    await app.setUiPrefs({ ...app.ui, avatarRadius: 150 });
    expect(document.documentElement.style.getPropertyValue("--avatar-radius")).toBe("100%");
  });

  it("avatarRadiusが範囲外(負数)でも0%にクランプされる", async () => {
    await app.setUiPrefs({ ...app.ui, avatarRadius: -10 });
    expect(document.documentElement.style.getPropertyValue("--avatar-radius")).toBe("0%");
  });
});
```

- [ ] **Step 2: テストを実行し失敗することを確認する**

Run: `cd frontend && pnpm vitest run store.svelte.test.ts -t "applyAvatarRadius"`
Expected: FAIL（`--avatar-radius` が未設定、または `avatarRadius` が `UiPrefs` の初期値に無く型エラー）

- [ ] **Step 3: 最小実装を書く**

`frontend/src/lib/store.svelte.ts` の `ui = $state<UiPrefs>({...})` 初期値リテラル（149行目付近、`noteCacheMaxSizeMb: 0,` の直後）に追加する:

```ts
    avatarRadius: 20,
```

`boot()` 内、`this.ui = { ...ui, ... }` の代入オブジェクト（241行目付近、`instanceTicker: ui.instanceTicker ?? "remote",` の直後）に追加する:

```ts
        avatarRadius: ui.avatarRadius ?? 20,
```

同じく `boot()` 内、`this.#applyBackground(this.ui);` の直後（258行目付近）に追加する:

```ts
      this.#applyAvatarRadius(this.ui.avatarRadius ?? 20);
```

`setUiPrefs(prefs: UiPrefs)` 内、`this.ui = { ...prefs, ... }` の代入オブジェクト（1311行目付近、`noteCacheMaxSizeMb: prefs.noteCacheMaxSizeMb ?? 0,` の直後）に追加する:

```ts
      avatarRadius: prefs.avatarRadius ?? 20,
```

同じく `setUiPrefs()` 内、`this.#applyBackground(this.ui);` の直後（1329行目付近）に追加する:

```ts
    this.#applyAvatarRadius(this.ui.avatarRadius ?? 20);
```

`#applyMediaThumbnailHeight` メソッドの直後（1494行目付近）に新規メソッドを追加する:

```ts
  /// アバター画像の角丸を <html> に反映する（0=直角 〜 100=真円、%。Issue #94）。
  /// 保存済み値が範囲外(不正な手動編集等)でもCSSが壊れないよう 0〜100 にクランプする。
  #applyAvatarRadius(pct: number) {
    const clamped = Math.min(100, Math.max(0, pct));
    document.documentElement.style.setProperty("--avatar-radius", `${clamped}%`);
  }
```

- [ ] **Step 4: テストを実行し成功することを確認する**

Run: `cd frontend && pnpm vitest run store.svelte.test.ts -t "applyAvatarRadius"`
Expected: PASS（3件とも通る）

- [ ] **Step 5: フロントの型チェックとテスト一式を実行する**

Run: `cd frontend && pnpm check && pnpm test`
Expected: PASS（既存テストも壊れていないこと）

- [ ] **Step 6: コミット**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "feat: store.svelte.tsでavatarRadiusを--avatar-radius CSS変数に反映(Issue #94)"
```

---

### Task 3: アバター表示7ファイルを `--avatar-radius` 参照に統一

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte`
- Modify: `frontend/src/ui/NotificationCard.svelte`
- Modify: `frontend/src/ui/AccountSelect.svelte`
- Modify: `frontend/src/ui/ReactionUsersPopover.svelte`
- Modify: `frontend/src/ui/FollowListModal.svelte`
- Modify: `frontend/src/ui/ProfileModal.svelte`
- Modify: `frontend/src/ui/settings/AccountsSection.svelte`

**Interfaces:**
- Consumes: `--avatar-radius` CSS変数（Task 2で `document.documentElement` に設定される）。フォールバック値 `20%` をクラス内に埋め込むため、変数が未設定でも壊れない。

このタスクにテストコードはない（対象コンポーネントに自動テストが存在せず、`docs/superpowers/specs/2026-09-07-avatar-radius-design.md` で確認済み）。`pnpm check` での型チェックと、Task 4完了後の手動確認（`cargo tauri dev`）で検証する。

- [ ] **Step 1: 各ファイルのアバター要素（画像・プレースホルダの両方）を置き換える**

`rounded-md` または `rounded-lg` を `rounded-[var(--avatar-radius,20%)]` に置き換える。**アバター以外の要素（ボタン、ポップオーバー枠、ドロップダウン項目等）の `rounded-md`/`rounded-lg` は変更しないこと。**

`frontend/src/ui/NoteCard.svelte`（2箇所、投稿者アイコンの画像とプレースホルダ）:

```svelte
        class="h-[34px] w-[34px] flex-none rounded-[var(--avatar-radius,20%)] object-cover"
```

```svelte
        class="avatar h-[34px] w-[34px] flex-none rounded-[var(--avatar-radius,20%)] outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
```

`frontend/src/ui/NotificationCard.svelte`（1箇所、送信者アイコン）:

```svelte
        class="h-6 w-6 flex-none rounded-[var(--avatar-radius,20%)] object-cover"
```

`frontend/src/ui/AccountSelect.svelte`（2箇所、アイコン画像とイニシャルプレースホルダ）:

```svelte
            <img src={a.avatarUrl} alt="" class="size-7 flex-none rounded-[var(--avatar-radius,20%)] object-cover" />
```

```svelte
              class="grid size-7 flex-none place-items-center rounded-[var(--avatar-radius,20%)] bg-accent text-[0.7rem] font-bold text-muted-foreground"
```

`frontend/src/ui/ReactionUsersPopover.svelte`（2箇所、アイコン画像とプレースホルダ）:

```svelte
              <img class="h-5 w-5 flex-shrink-0 rounded-[var(--avatar-radius,20%)] object-cover" src={u.avatarUrl} alt="" loading="lazy" />
```

```svelte
              <div class="h-5 w-5 flex-shrink-0 rounded-[var(--avatar-radius,20%)] bg-border"></div>
```

`frontend/src/ui/FollowListModal.svelte`（2箇所、アイコン画像とプレースホルダ）:

```svelte
            <img class="h-10 w-10 flex-none rounded-[var(--avatar-radius,20%)] object-cover" src={entry.user.avatarUrl} alt="" />
```

```svelte
            <div class="avatar-ph h-10 w-10 flex-none rounded-[var(--avatar-radius,20%)]"></div>
```

`frontend/src/ui/ProfileModal.svelte`（2箇所、アイコン画像とプレースホルダ）:

```svelte
        <img class="h-14 w-14 flex-none rounded-[var(--avatar-radius,20%)] border-2 border-background object-cover" src={profile.user.avatarUrl} alt="" />
```

```svelte
        <div class="avatar-ph h-14 w-14 flex-none rounded-[var(--avatar-radius,20%)] border-2 border-background"></div>
```

`frontend/src/ui/settings/AccountsSection.svelte`（2箇所、アイコン画像とイニシャルプレースホルダ）:

```svelte
          <img class="h-[34px] w-[34px] flex-none rounded-[var(--avatar-radius,20%)] object-cover" src={a.avatarUrl} alt="" />
```

```svelte
          <div class="grid h-[34px] w-[34px] flex-none place-items-center rounded-[var(--avatar-radius,20%)] bg-accent font-bold text-muted-foreground">{(a.displayName || a.username).charAt(0)}</div>
```

- [ ] **Step 2: 型チェックとフロントの既存テストを実行する**

Run: `cd frontend && pnpm check && pnpm test`
Expected: PASS。`NoteCard.test.ts` / `ProfileModal.test.ts` / `FollowListModal.test.ts` / `NotificationCard.test.ts` 等、`data-testid` ベースのテストは影響を受けないはず。

- [ ] **Step 3: コミット**

```bash
git add frontend/src/ui/NoteCard.svelte frontend/src/ui/NotificationCard.svelte frontend/src/ui/AccountSelect.svelte frontend/src/ui/ReactionUsersPopover.svelte frontend/src/ui/FollowListModal.svelte frontend/src/ui/ProfileModal.svelte frontend/src/ui/settings/AccountsSection.svelte
git commit -m "feat: 全アバター表示の角丸をCSS変数--avatar-radius参照に統一(Issue #94)"
```

---

### Task 4: 設定画面「アイコンの丸み」スライダーを追加

**Files:**
- Modify: `frontend/src/ui/settings/AppearanceSection.svelte`

**Interfaces:**
- Consumes: `app.ui.avatarRadius`（Task 2）、`app.setUiPrefs`（既存）。
- Produces: なし（末端のUI）。

このタスクにも自動テストはない（`AppearanceSection.svelte` に対応するテストファイルが存在しないため、Task 3と同じ理由）。`pnpm check` と手動確認で検証する。

- [ ] **Step 1: state とプレビュー・スライダーUIを追加する**

`frontend/src/ui/settings/AppearanceSection.svelte` の `let instanceTicker = $state(app.ui.instanceTicker ?? "remote");`（19行目付近）の直後に追加する:

```ts
  let avatarRadius = $state(app.ui.avatarRadius ?? 20);
```

`save()` 関数内、`await app.setUiPrefs({ ...app.ui, ... instanceTicker, });` の閉じ括弧の直前（227行目付近、`instanceTicker,` の直後）に追加する:

```ts
        avatarRadius,
```

テンプレート側、`<div class="mb-3 flex flex-col gap-1.5 text-sm">`（Instance Tickerのセクション、255〜272行目）の直後に新規セクションを追加する:

```svelte
<label class="mb-2.5 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">アイコンの丸み({avatarRadius}%)</span>
  <input class="w-full max-w-[320px] accent-primary" type="range" min="0" max="100" step="5" bind:value={avatarRadius} />
</label>
<p class="mb-2 mt-0 flex items-center gap-2 text-xs text-muted-foreground">
  プレビュー:
  <span
    class="inline-block h-8 w-8 flex-none bg-accent"
    style={`border-radius:${avatarRadius}%`}
  ></span>
</p>
<p class="mb-4 mt-0 text-xs text-muted-foreground">
  ノート・アカウント切替・プロフィール等、アプリ内すべてのアイコンに反映されます。
  0%が直角、100%が真円です。
</p>
```

- [ ] **Step 2: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: PASS

- [ ] **Step 3: 手動確認**

`cargo tauri dev` でアプリを起動し、設定 > 外観 で「アイコンの丸み」スライダーを動かしてプレビューが変化すること、保存後にノート・アカウント切替メニュー・プロフィールモーダル等のアイコンが実際に変化することを確認する。確認後は起動した `cargo tauri dev` プロセスを終了する。

- [ ] **Step 4: コミット**

```bash
git add frontend/src/ui/settings/AppearanceSection.svelte
git commit -m "feat: 設定画面にアイコンの丸みスライダーを追加(Issue #94)"
```

---

### Task 5: 最終検証とPR作成

**Files:** なし（検証とPR作成のみ）

- [ ] **Step 1: バックエンド・フロントエンドの全テストを実行する**

Run:
```bash
cd src-tauri && cargo test
cd ../frontend && pnpm check && pnpm test
```
Expected: すべてPASS

- [ ] **Step 2: ブランチをpushしてPRを作成する**

```bash
git push -u origin feature/issue-94-avatar-radius
gh pr create --title "feat: アイコンの丸みを設定可能にする" --body "$(cat <<'EOF'
## 概要
アバター画像の角丸を0%(直角)〜100%(真円)の範囲で設定画面から変更できるようにした。

## 変更内容
- `UiPrefs.avatarRadius`(既定20%)を追加
- `--avatar-radius` CSS変数として全アバター表示(ノート/アカウント切替/プロフィール/フォロー一覧/リアクション一覧/通知/設定のアカウント一覧)に反映
- 設定 > 外観 に「アイコンの丸み」スライダーとプレビューを追加

## 設計
docs/superpowers/specs/2026-09-07-avatar-radius-design.md 参照

Fixes #94

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Expected: PR URLが出力される。CI結果はユーザーが自分で確認する（Monitorでポーリングしない）。

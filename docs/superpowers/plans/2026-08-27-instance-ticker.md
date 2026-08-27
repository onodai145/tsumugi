# Instance Ticker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show an "Instance Ticker" (instance icon + name on the instance's theme color) below the author name/acct in NoteCard, for remote authors always and for local authors when the user opts in.

**Architecture:** Reuse Misskey's existing `UserLite.instance` field (already present in note payloads for remote users, zero extra API calls) for the remote case. For the local case, add a new `InstanceInfo` fetched once per account via `/api/meta` and cached on `Account`, refreshed fire-and-forget on app boot. A new `UiPrefs.instanceTicker` ("off"/"remote"/"always") setting gates all of it.

**Tech Stack:** Rust (Tauri commands, serde, specta), Svelte 5 (runes), Vitest, `cargo test`.

## Global Constraints

- Never hand-edit `frontend/src/bindings/tauri.gen.ts` — it regenerates via `cargo test` (the `generates_frontend_bindings` test) or `cargo tauri dev`.
- New/changed `#[tauri::command]`s must be registered in `specta_builder()` in `src-tauri/src/lib.rs`, not just `tauri::Builder`.
- Any new field on a domain type that's cached as JSON (note cache, settings file) needs `#[serde(default)]` for backward compatibility with already-persisted data.
- Commit messages: subject line only, no body (per project workflow rules).
- Run `cd src-tauri && cargo test` and `cd frontend && pnpm check && pnpm test` before considering a task done, per this repo's verification norms.

---

### Task 1: `InstanceInfo` domain type + `User.instance`

**Files:**
- Modify: `src-tauri/src/domain/user.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src-tauri/src/api/normalize.rs`
- Modify (test-only `User { ... }` literals — add `instance: None,` after the existing `banner_url: None,` line in each): `src-tauri/src/filter/mute.rs:89`, `src-tauri/src/api/users.rs:137`, `src-tauri/src/domain/note.rs:164`, `src-tauri/src/api/notes.rs:334`, `src-tauri/src/filter/eval.rs:296`, `src-tauri/src/filter/mod.rs:89`, `src-tauri/src/commands/column.rs:1195`, `src-tauri/src/stream/connection.rs:1145`, `src-tauri/src/store/note_cache.rs:535`

**Interfaces:**
- Produces: `domain::InstanceInfo { name: Option<String>, icon_url: Option<String>, theme_color: Option<String> }` (specta `Type`, camelCase, `PartialEq`). `domain::User.instance: Option<InstanceInfo>`.

- [ ] **Step 1: Write the failing normalize tests**

Add to `src-tauri/src/api/normalize.rs` (in the existing `#[cfg(test)] mod tests` block, near the other `raw_user_*` tests):

```rust
#[test]
fn raw_user_maps_instance_for_remote_user() {
    let raw: RawUser = serde_json::from_str(
        r#"{"id":"u1","username":"alice","host":"remote.example",
            "instance":{"name":"Remote Instance","iconUrl":"https://remote.example/icon.png",
            "themeColor":"#ff8800"}}"#,
    )
    .unwrap();
    let user: User = raw.into();
    let instance = user.instance.expect("instance should be present for remote user");
    assert_eq!(instance.name, Some("Remote Instance".to_string()));
    assert_eq!(instance.icon_url, Some("https://remote.example/icon.png".to_string()));
    assert_eq!(instance.theme_color, Some("#ff8800".to_string()));
}

#[test]
fn raw_user_has_no_instance_for_local_user() {
    let raw: RawUser =
        serde_json::from_str(r#"{"id":"u1","username":"alice","host":null}"#).unwrap();
    let user: User = raw.into();
    assert_eq!(user.instance, None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test raw_user_maps_instance_for_remote_user raw_user_has_no_instance_for_local_user`
Expected: compile error (`RawUser` has no field `instance`, `User` has no field `instance`) or FAIL.

- [ ] **Step 3: Add `InstanceInfo` and wire it through**

In `src-tauri/src/domain/user.rs`, add above `pub struct User`:

```rust
/// 投稿元インスタンスの表示情報（Instance Ticker用、Issue #103）。
/// リモートユーザーは Misskey の `UserLite.instance` から、ローカルユーザーは
/// 接続先インスタンスの `/api/meta`（[`Account::instance`]）から埋める。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InstanceInfo {
    pub name: Option<String>,
    pub icon_url: Option<String>,
    pub theme_color: Option<String>,
}
```

Add to `User` (after `banner_url`):

```rust
    /// 投稿元インスタンス情報。リモートユーザーのみ Some（Misskeyがローカルユーザーには
    /// このフィールドを付与しない）。追加前に保存されたキャッシュ済みJSONとの後方互換のため default。
    #[serde(default)]
    pub instance: Option<InstanceInfo>,
```

In `src-tauri/src/domain/mod.rs`, change:
```rust
pub use user::User;
```
to:
```rust
pub use user::{InstanceInfo, User};
```

In `src-tauri/src/api/normalize.rs`, add to `RawUser` (after `banner_url`):

```rust
    #[serde(default)]
    pub instance: Option<RawInstanceInfo>,
```

Above `RawUser`, add:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawInstanceInfo {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub theme_color: Option<String>,
}

impl From<RawInstanceInfo> for crate::domain::InstanceInfo {
    fn from(r: RawInstanceInfo) -> Self {
        crate::domain::InstanceInfo {
            name: r.name,
            icon_url: r.icon_url,
            theme_color: r.theme_color,
        }
    }
}
```

In `impl From<RawUser> for User`, add `instance: r.instance.map(Into::into),` after `banner_url: r.banner_url,`.

- [ ] **Step 4: Fix the now-broken `User { ... }` test literals**

Add `instance: None,` right after the `banner_url: None,` line in each of these 9 files (all inside `#[cfg(test)] mod tests` blocks):
`src-tauri/src/filter/mute.rs`, `src-tauri/src/api/users.rs`, `src-tauri/src/domain/note.rs`, `src-tauri/src/api/notes.rs`, `src-tauri/src/filter/eval.rs`, `src-tauri/src/filter/mod.rs`, `src-tauri/src/commands/column.rs`, `src-tauri/src/stream/connection.rs`, `src-tauri/src/store/note_cache.rs`.

- [ ] **Step 5: Run tests to verify they pass and nothing else broke**

Run: `cd src-tauri && cargo test`
Expected: all pass (including the two new tests and the two existing `user.rs` backward-compat tests, which must still pass unmodified since `instance` is `#[serde(default)]`).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/domain/user.rs src-tauri/src/domain/mod.rs src-tauri/src/api/normalize.rs \
  src-tauri/src/filter/mute.rs src-tauri/src/api/users.rs src-tauri/src/domain/note.rs \
  src-tauri/src/api/notes.rs src-tauri/src/filter/eval.rs src-tauri/src/filter/mod.rs \
  src-tauri/src/commands/column.rs src-tauri/src/stream/connection.rs src-tauri/src/store/note_cache.rs
git commit -m "feat: User.instanceにInstance Ticker用インスタンス情報を追加"
```

---

### Task 2: `Account.instance` + `/api/meta` fetch + `AccountManager::update_instance`

**Files:**
- Modify: `src-tauri/src/domain/account.rs`
- Modify: `src-tauri/src/api/meta.rs`
- Modify: `src-tauri/src/session/account_manager.rs`
- Modify (test-only `Account { ... }` literals — add `instance: None,` after `avatar_url: ...,`): `src-tauri/src/commands/user.rs:121`, `src-tauri/src/session/account_manager.rs:81`, `src-tauri/src/store/settings.rs:508`, `src-tauri/src/state.rs:169`, `src-tauri/src/stream/connection.rs:1174`
- Modify (production `Account { ... }` literals — add `instance: None,`): `src-tauri/src/commands/account.rs:130` (`build_account`), `src-tauri/src/store/settings.rs:411` (`migrate_from_legacy_sqlite`)

**Interfaces:**
- Consumes: `domain::InstanceInfo` (Task 1).
- Produces: `domain::Account.instance: Option<InstanceInfo>`. `api::meta::fetch_meta(client: &MisskeyClient) -> Result<InstanceInfo>`. `AccountManager::update_instance(&mut self, account_id: &str, instance: Option<InstanceInfo>) -> Result<Account>` (returns `Error::Invalid` for unknown id; does not change the active account).

- [ ] **Step 1: Write the failing `AccountManager::update_instance` test**

Add to `src-tauri/src/session/account_manager.rs` test module:

```rust
#[test]
fn update_instance_sets_field_without_changing_active() {
    let mut m = AccountManager::default();
    m.upsert(acc("id1", "u1"));
    m.upsert(acc("id2", "u2")); // id2 becomes active
    let info = crate::domain::InstanceInfo {
        name: Some("Misskey.io".into()),
        icon_url: Some("https://misskey.io/icon.png".into()),
        theme_color: Some("#86b300".into()),
    };
    let updated = m.update_instance("id1", Some(info.clone())).unwrap();
    assert_eq!(updated.instance, Some(info));
    assert_eq!(m.active_id(), Some("id2")); // 変わらない
    assert_eq!(m.get("id1").unwrap().instance.as_ref().unwrap().name.as_deref(), Some("Misskey.io"));
}

#[test]
fn update_instance_rejects_unknown_account() {
    let mut m = AccountManager::default();
    assert!(m.update_instance("nope", None).is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test update_instance`
Expected: compile error (no `instance` field on `Account`, no `update_instance` method).

- [ ] **Step 3: Add `Account.instance`**

In `src-tauri/src/domain/account.rs`, add to `Account` (after `avatar_url`):

```rust
    /// 接続先インスタンスの表示情報（Instance Ticker用、Issue #103）。ログイン/起動時に
    /// `/api/meta` から取得して埋める。取得前・失敗時は None。
    #[serde(default)]
    pub instance: Option<crate::domain::InstanceInfo>,
```

- [ ] **Step 4: Add `AccountManager::update_instance`**

In `src-tauri/src/session/account_manager.rs`, add after `upsert`:

```rust
    /// account_id の instance を更新する。active は変更しない（メタ取得は
    /// バックグラウンド更新であり、ユーザ操作としての切替とは無関係のため）。
    pub fn update_instance(
        &mut self,
        account_id: &str,
        instance: Option<crate::domain::InstanceInfo>,
    ) -> Result<Account> {
        let account = self
            .accounts
            .iter_mut()
            .find(|a| a.id == account_id)
            .ok_or_else(|| Error::Invalid(format!("unknown account: {account_id}")))?;
        account.instance = instance;
        Ok(account.clone())
    }
```

- [ ] **Step 5: Add `api::meta::fetch_meta`**

In `src-tauri/src/api/meta.rs`, add:

```rust
/// 接続先インスタンスの名前・アイコン・テーマカラー（Instance Ticker用、Issue #103）。
/// `/api/meta` は認証不要だが、他エンドポイントと同じ経路(`client.post`)で叩く。
/// `detail: false` で軽量なレスポンス(MetaLite相当)にする。
pub async fn fetch_meta(client: &MisskeyClient) -> Result<crate::domain::InstanceInfo> {
    let raw: RawMeta = client.post("meta", &json!({ "detail": false })).await?;
    Ok(raw.into())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMeta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    theme_color: Option<String>,
}

impl From<RawMeta> for crate::domain::InstanceInfo {
    fn from(r: RawMeta) -> Self {
        crate::domain::InstanceInfo {
            name: r.name,
            icon_url: r.icon_url,
            theme_color: r.theme_color,
        }
    }
}
```

- [ ] **Step 6: Write the failing `fetch_meta`/`RawMeta` parse test**

Add to `src-tauri/src/api/meta.rs` test module (create one if `meta.rs` doesn't have `#[cfg(test)] mod tests` yet):

```rust
#[cfg(test)]
mod meta_info_tests {
    use super::*;

    #[test]
    fn raw_meta_maps_all_fields() {
        let raw: RawMeta = serde_json::from_str(
            r#"{"name":"Misskey.io","iconUrl":"https://misskey.io/icon.png","themeColor":"#86b300"}"#,
        )
        .unwrap();
        let info: crate::domain::InstanceInfo = raw.into();
        assert_eq!(info.name, Some("Misskey.io".to_string()));
        assert_eq!(info.icon_url, Some("https://misskey.io/icon.png".to_string()));
        assert_eq!(info.theme_color, Some("#86b300".to_string()));
    }

    #[test]
    fn raw_meta_defaults_missing_fields_to_none() {
        let raw: RawMeta = serde_json::from_str(r#"{}"#).unwrap();
        let info: crate::domain::InstanceInfo = raw.into();
        assert_eq!(info.name, None);
        assert_eq!(info.icon_url, None);
        assert_eq!(info.theme_color, None);
    }
}
```

- [ ] **Step 7: Fix the now-broken `Account { ... }` literals**

Add `instance: None,` to each of these 7 sites (right after the `avatar_url: ...,` line):
`src-tauri/src/commands/user.rs:121`, `src-tauri/src/session/account_manager.rs:81` (the `acc()` test helper), `src-tauri/src/store/settings.rs:411` (`migrate_from_legacy_sqlite`, production — legacy DB has no such column, always `None`), `src-tauri/src/store/settings.rs:508`, `src-tauri/src/state.rs:169`, `src-tauri/src/stream/connection.rs:1174`, `src-tauri/src/commands/account.rs:130` (`build_account`, production — filled in later by Task 3's `refresh_instance_meta`).

- [ ] **Step 8: Run tests to verify everything passes**

Run: `cd src-tauri && cargo test`
Expected: all pass, including the 4 new tests from steps 1 and 6.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/domain/account.rs src-tauri/src/api/meta.rs src-tauri/src/session/account_manager.rs \
  src-tauri/src/commands/user.rs src-tauri/src/store/settings.rs src-tauri/src/state.rs \
  src-tauri/src/stream/connection.rs src-tauri/src/commands/account.rs
git commit -m "feat: Account.instanceと/api/meta取得を追加"
```

---

### Task 3: `refresh_instance_meta` command + registration + binding regen

**Files:**
- Modify: `src-tauri/src/commands/account.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `state.client_for`, `api::meta::fetch_meta`, `state.accounts.lock().unwrap().update_instance`, `state.settings.upsert_account` (all from Tasks 1–2 and pre-existing code).
- Produces: `#[tauri::command] refresh_instance_meta(account_id: String) -> Result<Account>`, registered in `specta_builder()` and exported to `frontend/src/bindings/tauri.gen.ts` as `commands.refreshInstanceMeta(accountId: string)`.

- [ ] **Step 1: Add the command**

In `src-tauri/src/commands/account.rs`, add after `whoami`:

```rust
/// 接続先インスタンスの `/api/meta` を取得し、Account.instance を更新して返す
/// （Instance Ticker用、Issue #103）。boot時にフロントから全アカウント分呼ばれる想定。
#[tauri::command]
#[specta::specta]
pub async fn refresh_instance_meta(state: State<'_, AppState>, account_id: String) -> Result<Account> {
    let client = state.client_for(&account_id)?;
    let info = crate::api::meta::fetch_meta(&client).await?;
    let account = state.accounts.lock().unwrap().update_instance(&account_id, Some(info))?;
    state.settings.upsert_account(&account)?;
    Ok(account)
}
```

- [ ] **Step 2: Register in `specta_builder()`**

In `src-tauri/src/lib.rs`, in the `commands![...]` list inside `specta_builder()`, add `commands::account::refresh_instance_meta,` after `commands::account::whoami,`.

- [ ] **Step 3: Regenerate frontend bindings and run the binding-generation test**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS, and `frontend/src/bindings/tauri.gen.ts` now contains `refreshInstanceMeta` in `commands` and `instance?: InstanceInfo | null` on the generated `Account`/`User` types (from Tasks 1–2). Do not hand-edit this file — just confirm the diff via `git diff frontend/src/bindings/tauri.gen.ts`.

- [ ] **Step 4: Run the full Rust test suite once more**

Run: `cd src-tauri && cargo test`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/account.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: refresh_instance_metaコマンドを追加"
```

---

### Task 4: `UiPrefs.instanceTicker` setting

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`

**Interfaces:**
- Produces: `UiPrefs.instance_ticker: String` (`"off" | "remote" | "always"`, default `"remote"`), exported as `instanceTicker` on the TS `UiPrefs` type.

- [ ] **Step 1: Write the failing legacy-JSON backward-compat test**

Add to `src-tauri/src/domain/ui.rs` test module, next to `deserializes_legacy_json_without_new_fields`:

```rust
#[test]
fn instance_ticker_defaults_to_remote_for_legacy_json() {
    // instance_ticker 追加前に保存された JSON も読めること（#[serde(default)]）。
    let v: UiPrefs = serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
    assert_eq!(v.instance_ticker, "remote");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test instance_ticker_defaults_to_remote_for_legacy_json`
Expected: compile error (no `instance_ticker` field).

- [ ] **Step 3: Add the field**

In `src-tauri/src/domain/ui.rs`, add to `UiPrefs` (after `summaly_proxy_url`):

```rust
    /// Instance Ticker の表示モード（Issue #103）。
    /// "off" = 表示しない / "remote" = リモートユーザーの投稿にのみ表示(既定) /
    /// "always" = ローカルユーザー（自分と同一インスタンス）の投稿にも表示。
    #[serde(default = "default_instance_ticker")]
    pub instance_ticker: String,
```

Add the default function near the other `default_*` functions:

```rust
fn default_instance_ticker() -> String {
    "remote".into()
}
```

Add to `impl Default for UiPrefs`, after `summaly_proxy_url: String::new(),`:

```rust
            instance_ticker: default_instance_ticker(),
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test`
Expected: all pass, including the existing `deserializes_legacy_json_without_new_fields` (unaffected) and the new test.

- [ ] **Step 5: Regenerate bindings**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS; `frontend/src/bindings/tauri.gen.ts`'s `UiPrefs` type now has `instanceTicker?: string`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/domain/ui.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: UiPrefs.instanceTicker設定を追加"
```

---

### Task 5: `readableTextColor` helper (frontend)

**Files:**
- Create: `frontend/src/lib/color.ts`
- Create: `frontend/src/lib/color.test.ts`

**Interfaces:**
- Produces: `readableTextColor(hex: string): "#000000" | "#ffffff"` — given a `"#rrggbb"` background color, returns the WCAG-relative-luminance-appropriate text color. Any input that isn't a valid `#rrggbb` hex string returns `"#ffffff"` (safe default; callers are expected to skip rendering the colored background entirely when the source color is invalid, per Task 7).

- [ ] **Step 1: Write the failing test file**

Create `frontend/src/lib/color.test.ts`:

```typescript
import { describe, expect, it } from "vitest";
import { readableTextColor } from "./color";

describe("readableTextColor", () => {
  it("returns white text for a dark background", () => {
    expect(readableTextColor("#000000")).toBe("#ffffff");
  });

  it("returns black text for a light background", () => {
    expect(readableTextColor("#ffffff")).toBe("#000000");
  });

  it("returns white text for a saturated dark accent color", () => {
    expect(readableTextColor("#ff0000")).toBe("#ffffff");
  });

  it("returns black text for a pale accent color", () => {
    expect(readableTextColor("#ffff00")).toBe("#000000");
  });

  it("falls back to white text for an invalid hex value", () => {
    expect(readableTextColor("not-a-color")).toBe("#ffffff");
    expect(readableTextColor("")).toBe("#ffffff");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm vitest run src/lib/color.test.ts`
Expected: FAIL (module `./color` doesn't exist).

- [ ] **Step 3: Implement `readableTextColor`**

Create `frontend/src/lib/color.ts`:

```typescript
// 任意インスタンスの themeColor（Instance Ticker用、Issue #103）は可読性が保証されない
// 第三者由来の色なので、相対輝度から自動で黒/白の文字色を選ぶ。
// 参考: WCAG 2.0 の相対輝度式 (https://www.w3.org/TR/WCAG20/#relativeluminancedef)
const HEX_RE = /^#([0-9a-fA-F]{6})$/;

function srgbToLinear(c: number): number {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

export function readableTextColor(hex: string): "#000000" | "#ffffff" {
  const m = HEX_RE.exec(hex);
  if (!m) return "#ffffff";
  const n = parseInt(m[1], 16);
  const r = srgbToLinear((n >> 16) & 0xff);
  const g = srgbToLinear((n >> 8) & 0xff);
  const b = srgbToLinear(n & 0xff);
  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  // 背景の相対輝度が高い(明るい)ほど黒文字が読みやすい。しきい値0.179は
  // WCAG的に「白文字とのコントラスト比 >= 4.5」が概ね崩れ始める境目。
  return luminance > 0.179 ? "#000000" : "#ffffff";
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm vitest run src/lib/color.test.ts`
Expected: PASS (all 6 cases).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/color.ts frontend/src/lib/color.test.ts
git commit -m "feat: 背景色から可読な文字色を選ぶreadableTextColorを追加"
```

---

### Task 6: Boot-time instance meta refresh + `instanceTicker` default (frontend store)

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: `commands.refreshInstanceMeta(accountId: string)` (Task 3), `unwrapAcc` (existing, from `./ipc`).
- Produces: `app.ui.instanceTicker: string` populated in `boot()` (defaulting to `"remote"`); a private `#refreshInstanceMeta()` method that updates `app.accounts` in place as each account's meta resolves, called fire-and-forget from `boot()`.

- [ ] **Step 1: Add the `instanceTicker` default in `boot()`**

In `frontend/src/lib/store.svelte.ts`, inside the `this.ui = { ...ui, ... }` object literal in `boot()`, add:

```typescript
        instanceTicker: ui.instanceTicker ?? "remote",
```

- [ ] **Step 2: Add `#refreshInstanceMeta` and call it from `boot()`**

Add a new private method near `#syncServerMutes`:

```typescript
  /// 全アカウント分の接続先インスタンス情報（アイコン・名前・テーマカラー）を
  /// バックグラウンドで取得し、解決したものから順に app.accounts へ反映する
  /// （Instance Ticker「常に表示」モード用、Issue #103）。起動をブロックしないよう
  /// boot() からは await せずに呼ぶ。個別の失敗は無視（次回起動で再試行される）。
  async #refreshInstanceMeta() {
    const results = await Promise.allSettled(
      this.accounts.map((a) => unwrapAcc(a.id, commands.refreshInstanceMeta(a.id))),
    );
    for (const r of results) {
      if (r.status === "fulfilled") {
        const updated = r.value;
        this.accounts = this.accounts.map((a) => (a.id === updated.id ? updated : a));
      }
    }
  }
```

In `boot()`, right after `this.accounts = await unwrap(commands.listAccounts());`, add:

```typescript
      void this.#refreshInstanceMeta();
```

- [ ] **Step 3: Type-check**

Run: `cd frontend && pnpm check`
Expected: no errors (relies on `frontend/src/bindings/tauri.gen.ts` already having `refreshInstanceMeta` and `instanceTicker` from Tasks 3–4 — if this fails with "property does not exist", re-run `cd src-tauri && cargo test generates_frontend_bindings` first).

- [ ] **Step 4: Run the existing frontend test suite**

Run: `cd frontend && pnpm test`
Expected: all pass (no existing test exercises `boot()`'s network calls directly; this is a smoke check that nothing broke).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: boot時に全アカウントのインスタンス情報を取得"
```

---

### Task 7: `instanceTicker` setting UI (AppearanceSection)

**Files:**
- Modify: `frontend/src/ui/settings/AppearanceSection.svelte`

**Interfaces:**
- Consumes: `app.ui.instanceTicker` (Task 6), `app.setUiPrefs` (existing).

- [ ] **Step 1: Add local state**

In the `<script>` block of `AppearanceSection.svelte`, add near the other `let ... = $state(app.ui.X ?? ...)` lines:

```typescript
  let instanceTicker = $state(app.ui.instanceTicker ?? "remote");
```

Add the option list near `const themes: ...`:

```typescript
  const instanceTickerOptions: { id: string; label: string }[] = [
    { id: "off", label: "表示しない" },
    { id: "remote", label: "リモートのみ" },
    { id: "always", label: "常に表示" },
  ];
```

- [ ] **Step 2: Add the segmented control to the template**

Insert this block right after the closing `</div>` of the existing "テーマ" segmented-control block (the one built from `themes`):

```svelte
<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">Instance Ticker（投稿元インスタンス表示）</span>
  <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
    {#each instanceTickerOptions as t (t.id)}
      <button
        type="button"
        class={instanceTicker === t.id
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-sm text-primary-foreground last:border-r-0"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-sm text-foreground last:border-r-0"}
        onclick={() => (instanceTicker = t.id)}
      >{t.label}</button>
    {/each}
  </div>
  <p class="mb-4 mt-0 text-xs text-muted-foreground">
    ノートの投稿者名の下に、投稿元インスタンスのアイコン・名前をテーマカラーで表示します。
    「常に表示」はローカルユーザー（自分と同じインスタンス）の投稿にも表示します。
  </p>
</div>
```

- [ ] **Step 3: Save the field**

In the `save()` function's `await app.setUiPrefs({ ...app.ui, ... })` call, add `instanceTicker,` to the object.

- [ ] **Step 4: Type-check and manually sanity-check the build**

Run: `cd frontend && pnpm check`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/settings/AppearanceSection.svelte
git commit -m "feat: 外観設定にInstance Tickerの表示モードを追加"
```

---

### Task 8: Render the ticker in NoteCard

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte`
- Modify: `frontend/src/ui/NoteCard.test.ts`

**Interfaces:**
- Consumes: `readableTextColor` (Task 5), `inner.user.instance` / `inner.user.host` (Task 1), `app.accounts[].instance` (Task 2), `app.ui.instanceTicker` (Task 4/6), the existing `emojiAcct` derived value (already computed in `NoteCard.svelte` for emoji proxying — reuse it for account lookup so the ticker also works inside nested quoted-Renote `<Self>` calls, which pass `emojiAccountId` but not `accountId`).

- [ ] **Step 1: Write the failing NoteCard tests**

Add the import near the top of `frontend/src/ui/NoteCard.test.ts` (after the existing `../bindings/tauri.gen` import):

```typescript
import { app } from "../lib/store.svelte";
```

Add near the other `describe` blocks (reuse the existing `makeNote`/`makeUser`/`render` helpers already defined at the top of the file):

```typescript
describe("instance ticker", () => {
  function remoteUser(): User {
    return makeUser({
      host: "remote.example",
      instance: {
        name: "Remote Instance",
        iconUrl: "https://remote.example/icon.png",
        themeColor: "#ff8800",
      },
    });
  }

  it("shows the ticker for a remote author by default (mode=remote)", async () => {
    const { getByText } = render(NoteCard, {
      note: makeNote({ user: remoteUser() }),
    });
    expect(getByText("Remote Instance")).toBeTruthy();
  });

  it("hides the ticker entirely when mode=off", async () => {
    app.ui = { ...app.ui, instanceTicker: "off" };
    const { queryByText } = render(NoteCard, {
      note: makeNote({ user: remoteUser() }),
    });
    expect(queryByText("Remote Instance")).toBeNull();
    app.ui = { ...app.ui, instanceTicker: "remote" };
  });

  it("does not show a ticker for a local author when mode=remote", async () => {
    const { queryByText } = render(NoteCard, {
      note: makeNote({ user: makeUser({ host: null }) }),
    });
    expect(queryByText("Alice")).toBeTruthy(); // 投稿者名は出る
    expect(document.querySelector("[data-testid='note-instance-ticker']")).toBeNull();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "instance ticker"`
Expected: FAIL (no ticker rendered yet, `data-testid` doesn't exist).

- [ ] **Step 3: Add the derived ticker value and markup**

In `frontend/src/ui/NoteCard.svelte`, add the import:

```svelte
  import { readableTextColor } from "../lib/color";
```

Add a derived value near the existing `instanceHost` derivation:

```svelte
  // Instance Ticker（Issue #103）: リモートユーザーは note.user.instance をそのまま使う。
  // ローカルユーザーは mode="always" のときだけ、閲覧中アカウントの instance を使う
  // （accountId ではなく emojiAcct を使うのは、引用Renote内側のSelfが accountId を
  // 渡さないため。emojiAcct は同じ理由で既存の絵文字プロキシ解決にも使われている）。
  const ticker = $derived.by(() => {
    const mode = app.ui.instanceTicker ?? "remote";
    if (mode === "off") return null;
    if (inner.user.instance) return inner.user.instance;
    if (inner.user.host === null && mode === "always") {
      const acc = emojiAcct ? app.accounts.find((a) => a.id === emojiAcct) : undefined;
      return acc?.instance ?? null;
    }
    return null;
  });
  const tickerLabel = $derived(ticker?.name ?? (inner.user.host ?? instanceHost ?? ""));
```

Insert this block right after the closing `</header>` tag and before the `{#if inner.cw}` block:

```svelte
      {#if ticker && (ticker.themeColor || ticker.iconUrl || tickerLabel)}
        <div
          class="mt-1 inline-flex w-fit max-w-full items-center gap-1 overflow-hidden rounded-sm px-1.5 py-0.5 text-xs"
          data-testid="note-instance-ticker"
          style={ticker.themeColor
            ? `background:${ticker.themeColor};color:${readableTextColor(ticker.themeColor)}`
            : undefined}
          class:bg-muted={!ticker.themeColor}
          class:text-muted-foreground={!ticker.themeColor}
        >
          {#if ticker.iconUrl}
            <img src={ticker.iconUrl} alt="" class="size-3 flex-none rounded-full object-cover" />
          {/if}
          <span class="overflow-hidden text-ellipsis whitespace-nowrap">{tickerLabel}</span>
        </div>
      {/if}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts`
Expected: all pass, including the pre-existing NoteCard tests (unaffected).

- [ ] **Step 5: Run the full frontend check**

Run: `cd frontend && pnpm check && pnpm test`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/ui/NoteCard.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "feat: NoteCardにInstance Tickerを表示"
```

---

### Task 9: Manual verification

**Files:** none (verification only).

- [ ] **Step 1: Run the app**

Run: `cargo tauri dev` (from repo root, per CLAUDE.md — never `cargo run`/`./target/debug/tsumugi` directly).

- [ ] **Step 2: Verify remote-mode ticker**

Open a column showing notes from a remote-instance account (or federated timeline). Confirm a colored pill with the remote instance's icon+name appears below the author name/acct, with legible text color regardless of the instance's theme color.

- [ ] **Step 3: Verify the setting toggle**

Open Settings → 外観, switch Instance Ticker between 表示しない/リモートのみ/常に表示, and confirm NoteCard updates accordingly (常に表示 should make local-instance authors' notes also show a ticker, using the account's own instance icon/name/color; 表示しない hides all tickers).

- [ ] **Step 4: Kill the dev server**

Since this was started for manual verification, kill the `cargo tauri dev` process you started (per project convention — don't leave verification dev servers running).

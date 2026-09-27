# 強制猫化 / 強制人間化（Issue #42）

## 背景・目的

現状、`isCat` なユーザーのノート本文・CWは `nyaize()` で「にゃん語化」され、アバターには猫耳が付く。
ユーザーの好みで、`isCat` に関係なく全ユーザーを猫化、または全ユーザーを人間化（nyaize・猫耳なし）できるようにする。

## スコープ

- 粒度: 全体一括の設定（アカウントごと・ユーザーごとの指定は対象外）。
- 影響範囲: ノート本文・CWの nyaize と、アバターの猫耳の両方。

## 設計

### 設定値

`UiPrefs`（`src-tauri/src/domain/ui.rs`）に `cat_mode: Option<String>` を追加する（camelCase で `catMode`）。

- `"respect"`: 既定。従来どおり `isCat` に従う。
- `"cat"`: 全ユーザーを猫扱い。
- `"human"`: 全ユーザーを人間扱い。

`None` は `"respect"` と同義。`catMode` を持たない既存の設定JSONもそのまま読める。
`instanceTicker` と同様、TS側は specta 生成の `UiPrefs` 型と `app.ui.catMode` 経由で扱う。

### 判定ロジック

`frontend/src/lib/catMode.ts` に集約する。

```ts
export function effectiveIsCat(isCat: boolean): boolean
```

`app.ui.catMode` を参照し、`"cat"` なら true、`"human"` なら false、それ以外は引数の `isCat` を返す。

### 適用箇所

- `Avatar.svelte`: 内部で `isCat` を `effectiveIsCat()` に通してから猫耳を描画する。呼び出し元（NoteCard、通知、プロフィール、フォロー一覧、リアクション、アカウント選択・設定）は変更不要。
- `NoteCard.svelte`: CW・本文の `nyaize={inner.user.isCat}` の2か所を `effectiveIsCat(inner.user.isCat)` に変更する。コピー時の元文字列復元（`data-original-text`）は既存実装のまま機能する。

### UI

`AppearanceSection.svelte` に「猫化」項目を追加する（`instanceTicker` と同形式）。
選択肢: 「ユーザー設定に従う」/「全員を猫化」/「全員を人間化」。保存は既存の UiPrefs 保存経路を使う。

## テスト

- Vitest: `effectiveIsCat` の3モード、`Avatar` の猫耳表示、`NoteCard` の nyaize 切替。
- Rust: `catMode` を含まない JSON から `UiPrefs` が読めること。`cargo test` の bindings 生成テストで `catMode` が出力されること。

## ドキュメント

`docs/guide/user-guide.md` に設定項目の説明を1つ追記する。

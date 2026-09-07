# アイコンの丸みを変えられるようにする（Issue #94）

- 作成日: 2026-09-07
- Issue: #94「アイコンの丸みを変えられるようにする」（四角から丸まで任意に）

## 背景・現状

アバター画像（ユーザーアイコン）は次の7箇所でそれぞれ `<img>` に角丸クラスが直書きされており、値がばらついている。

| ファイル | クラス | 実測値 |
|---|---|---|
| `NoteCard.svelte`（ノートの投稿者アイコン） | `rounded-md` | 6px |
| `NotificationCard.svelte`（通知の送信者アイコン） | `rounded-md` | 6px |
| `AccountSelect.svelte`（アカウント切替メニュー） | `rounded-md` | 6px |
| `ReactionUsersPopover.svelte`（リアクションユーザー一覧） | `rounded-md` | 6px |
| `AccountsSection.svelte`（設定 > アカウント一覧） | `rounded-lg` | 8px |
| `FollowListModal.svelte`（フォロー/フォロワー一覧） | `rounded-lg` | 8px |
| `ProfileModal.svelte`（プロフィールモーダル） | `rounded-lg` | 8px |

値はユーザーが変更できず、四角寄りの中間的な角丸に固定されている。Issue #94 は「四角から丸まで任意に」変更できるようにすることを求めている。

## 方針

- 適用範囲: 上記7箇所すべてを1つの設定値で一括変更する（文脈ごとの個別設定はスコープ外）。
- 値の持たせ方: 本家Misskeyのクライアント設定「アイコンの丸み」に準拠し、0〜100（%）の整数値とする。border-radius をパーセント指定にすることで、正方形画像なら 0%=直角、100%=真円、中間で任意の丸みになる。
- 既定値: 20%。現行の `rounded-md`（34pxアバターで6px、約17.6%相当）に近い見た目を維持しつつ、離散値ではなく任意の中間値を選べるようにする。

## データモデル

`src-tauri/src/domain/ui.rs` の `UiPrefs` に以下を追加する（他のパーセント系フィールド `column_opacity` と同じパターン）。

```rust
/// アバター画像の角丸（0=直角 〜 100=真円、%）。既定は20（Issue #94）。
#[serde(default = "default_avatar_radius")]
pub avatar_radius: i32,
```

```rust
fn default_avatar_radius() -> i32 {
    20
}
```

`Default for UiPrefs` にも `avatar_radius: default_avatar_radius()` を追加する。範囲外の値（負数や100超）が保存されていた場合はフロント側の適用時にクランプする（Rust側では他のパーセント系フィールドと同様、型としては制約しない）。

`specta_builder()` は `UiPrefs` 型全体をすでに export しているため、フィールド追加のみで `tauri.gen.ts` に自動反映される（`cargo test` の `generates_frontend_bindings` で再生成・検証）。

## フロント側の反映経路

`store.svelte.ts` の既存パターン（`columnOpacity` → `--column-opacity` CSS変数）を踏襲する。

1. state 初期値・ロード時のデフォルトマージ（`ui.avatarRadius ?? 20`）に追加。
2. 見た目適用処理（`--column-opacity` 等を `setProperty` している箇所と同じ関数）で以下を設定する。

```ts
const radius = Math.min(100, Math.max(0, prefs.avatarRadius ?? 20));
root.style.setProperty("--avatar-radius", `${radius}%`);
```

3. 保存経路（`setUiPrefs`）は既存フィールドと同様、呼び出し元が `{ ...app.ui, avatarRadius }` の形で渡すだけで良く、追加のIPC変更は不要。

## UI（設定画面）

`frontend/src/ui/settings/AppearanceSection.svelte` に「アイコンの丸み」セクションを追加する。既存の「MFMアニメーション」チェックボックスなどと同様、他フィールドと合わせて画面下部の「保存」ボタンで一括保存する（このセクションの他コントロールと同じ扱い）。

- `<input type="range" min="0" max="100" step="5" bind:value={avatarRadius} />`
- 現在値をパーセント表示（例: `20%`）
- プレビュー: ダミーの正方形要素（背景色ブロックで可、実アバター画像である必要はない）に `style="border-radius: {avatarRadius}%"` を当てて、四角〜丸までの変化をその場で確認できるようにする。

## 対象コンポーネントの変更

7ファイルすべてで、アバター `<img>` の `rounded-md` / `rounded-lg` を `rounded-[var(--avatar-radius)]` に置き換える（Tailwind の任意値クラスでCSS変数を参照する）。`--avatar-radius` はルート要素に設定されるため、モーダル内の `<img>` からも参照できる。

`NoteCard.svelte` の `.avatar` スタイルブロック（613行目付近）等、クラス以外でも角丸を指定している箇所があれば同様に統一する。

## 影響しない範囲

- 絵文字・カスタム絵文字の角丸は対象外（別ドメイン）。
- カード・モーダル・ポップオーバー等、アバター以外のUI要素の角丸（style-guide.mdのトークン）は変更しない。
- 正方形以外の形状（六角形等）はスコープ外。

## テスト

- `src-tauri/src/domain/ui.rs`: `avatar_radius` のデフォルト値・シリアライズ/デシリアライズ（既存の `UiPrefs` シリアライズテストに追加）。
- `frontend/src/lib/store.svelte.test.ts`: `avatarRadius` の保存→ロードのラウンドトリップ、デフォルト値のフォールバック。
- `pnpm check` でスライダー追加箇所の型チェック。
- 既存の `NoteCard.test.ts` 等、`rounded-md`/`rounded-lg` クラスを直接アサートしているテストがあれば `rounded-[var(--avatar-radius)]` に合わせて更新する。

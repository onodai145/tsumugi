# MiAuth 再認証 UI 設計 (Issue #99 スコープ外項目)

## 背景

Issue #99(#104でバックエンド対応済み)で、MiAuth 再認証時に `account.id` が維持されるようになった。これにより、既存アカウントのトークンを新しい権限スコープで差し替えても、カラム/グループの紐付けが失われなくなった。ただしバックエンドは「host+user_id が一致すれば id を再利用する」というだけで、それを呼び出すフロント側の導線が無い。

#99 で意図的にスコープ外とした以下2点を今回設計する:

1. UI側の「再認証」ボタンの具体的な導線・文言
2. 権限不足(403)を検知して自動的に再認証を促す仕組み

両者は同じ再認証フロー(`AddAccount.svelte` を起点に `start_miauth`/`complete_miauth` を叩く)を共有するため、1つの設計としてまとめる。

## スコープ

- 設定画面のアカウント一覧に、アカウントごとの「再認証」ボタンを追加。
- 403エラー発生時、既存の操作ログ(Backstage)に「再認証」アクション付きの警告を出す仕組みを追加。
- 上記2つの入口はどちらも同じ `AddAccount.svelte`(reauth モード)に合流する。

### 非対象(YAGNI)

- 再認証中に別ユーザーを認可してしまった場合の専用エラーメッセージ(通常の新規アカウント登録として扱う。バックエンド側で警告は出さない)。
- 403以外のエラー(401 Unauthorized 等)からの再認証誘導。今回は `Error::Forbidden`(kind: `"forbidden"`)のみを対象とする。
- 全ての `accountId` を伴う IPC 呼び出しへの `unwrapAcc` 適用。既存で403判定をしていた `syncServerMutes` と、ノート操作系(`postNote`/`renote`/`deleteNoteCmd`/`react`/`unreact`/`favoriteNote`/`unfavoriteNote`/`createClip`/`addNoteToClip`/`votePoll`)、および `addColumn`/`resolveUserAcct`/`listUserLists`/`listAntennas`/`listChannels` を対象とする。UI設定系(`setUiPrefs`/`setMute`/`setNotify` 等、accountId を取らないもの)は対象外。

## アーキテクチャ

```
[手動] 設定→アカウント一覧の「再認証」ボタン
[自動] 403検知時、Backstageログ行の「再認証」アクション
              ↓ どちらも accountId を App.svelte の reauthAccount state に渡す
   AddAccount.svelte（reauthAccount props 経由でreauthモード）
              ↓ host欄をスキップして自動 start_miauth → ブラウザで認可 → complete_miauth
   バックエンド（#104）が host+userId 一致で既存 account.id を維持
```

認可完了後のバックエンドロジックは #104 で実装済みのため変更しない。フロントは「再認証フローへの入口を2つ用意し、host入力を省略する」ことだけを行う。

## コンポーネント詳細

### `AddAccount.svelte`(reauth モード追加)

```ts
let { onclose, reauthAccount }: {
  onclose?: () => void;
  reauthAccount?: { id: string; host: string; username: string };
} = $props();
```

- `reauthAccount` が渡された場合、host入力ステップを省略し、mount時に自動で `start(reauthAccount.host)` を実行してブラウザを開く。
- 見出しを「アカウントを追加」→「再認証: @{username}@{host}」に切り替え。
- 説明文を「スコープが更新されたトークンを取得します。ブラウザで認可を完了したら…」に変更。
- `complete()` 完了後の挙動(`onclose?.()` 呼び出し)は既存のまま。

### `AccountsSection.svelte`(「再認証」ボタン追加)

- 既存の「既定に設定」「削除」ボタンと並べて常時表示(手動でいつでも実行可能。403検知を待たずに使える)。
- クリックで `onReauth(account)` を呼び、`Settings.svelte` 経由で `App.svelte` に伝播する(既存の `onAddAccount` と同じ配線パターン)。

### `App.svelte`(配線)

- `let reauthAccount = $state<Account | null>(null)` を追加。
- Settings側の「再認証」ボタン、Backstageログの「再認証」アクションの両方がこの state をセットする一本の入口にする。
- `reauthAccount` が非nullの間、既存の `showAdd` 分岐に統合する形で `<AddAccount reauthAccount={...} onclose={() => (reauthAccount = null)} />` を表示する。
- 既にモーダルが開いている間に別アカウントの403が来ても上書きしない(先勝ち。ユーザーが閉じてから次を処理する)。

### 自動403検知(`LogEntry` 拡張 + `unwrapAcc` ヘルパー)

`error.rs` の `Error` は `#[serde(tag = "kind", ...)]` で型付けされており、`Error::Forbidden` は `kind: "forbidden"` としてフロントに渡る(`ApiError.kind === "forbidden"` で判定可能、既存の `store.svelte.ts:817` のような文字列 regex 判定は不要になる)。

`lib/store.svelte.ts` の `LogEntry` に以下を追加:

```ts
export interface LogEntry {
  id: number;
  at: number;
  level: LogLevel;
  text: string;
  reauthAccountId?: string; // 存在すれば Backstage が「再認証」ボタンを表示
}
```

`lib/ipc.ts` に `unwrapAcc(accountId: string, p: Promise<Result<T>>): Promise<T>` を追加。`r.error.kind === "forbidden"` の場合、投げる `Error` に `accountId` を持たせて(例: カスタムプロパティ、または `{ accountId, message }` を throw)呼び出し元(`store.svelte.ts` の各アクション)の既存 `catch` 節で `#log("warn", ..., { reauthAccountId: accountId })` を積めるようにする。

- スコープ節に挙げた各アクション(`syncServerMutes` の既存 regex 判定を置き換え、加えて `postNote`/`react`/`favoriteNote`/`addNoteToClip` 等)の `unwrap(commands.xxx(accountId, ...))` を `unwrapAcc(accountId, commands.xxx(accountId, ...))` に置き換える。
- 既存の各 `catch` ブロックの挙動(エラー表示・ロールバック等)は変更しない。ログへの「再認証」アクション追加は副作用の追加であり、エラー処理フロー自体は変えない。

### `Backstage.svelte`(表示)

- ログ行に `reauthAccountId` があれば「再認証」ボタンを描画。
- クリックで `onReauth(accountId)` prop を呼ぶ(`App.svelte` から配線し、`reauthAccount` state をセットする)。

## エラーハンドリング

- 再認証フロー中の失敗(`start_miauth`/`complete_miauth` 失敗)は、既存の `AddAccount.svelte` の `err` state 表示をそのまま使う。
- reauthモードで元と別のユーザーを認可してしまった場合、`complete_miauth` は host+user_id が不一致のため新規アカウントとして登録される(元のアカウントは変化しない)。これは意図した挙動として許容し、専用の警告は出さない(非対象節を参照)。
- `unwrapAcc` 経由の403は re-throw されるため、既存の各画面のエラー表示・トースト等の挙動は変わらない。

## テスト

- バックエンド: 変更なし(#104で対応済み)。
- フロントエンド: `pnpm check` で型チェックが通ることを確認。ロジックテストの仕組みが現状無いため、以下は `cargo tauri dev` 上で手動確認する:
  - 設定→アカウントの「再認証」→ブラウザが開く→認可完了→カラム/グループが維持されたままトークンが更新されることを確認。
  - 古いスコープのトークンで403を意図的に起こす→ Backstageログに「再認証」ボタン付きの警告が出ることを確認。
  - Backstage側の「再認証」ボタンからも同じモーダルが開くことを確認。
  - 通常の「アカウントを追加」(host未知)と reauth モード(host既知)の両方でAddAccountの表示切り替えが正しいことを確認。

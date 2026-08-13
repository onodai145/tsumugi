# グローバルエラーバナー廃止 → モーダル/ログ振り分け設計 (Issue #183)

## 背景・課題

`App.svelte` は `app.error` が非nullのとき、投稿欄(ComposeBar)の下にバナーを表示する。このバナーは
`store.svelte.ts` の `#fail(e)` が呼ばれるたびに立ち、同時に Backstage ログ(`app.logs`)にも同じ内容が
記録される。

- `#fail(e)` は25箇所以上から呼ばれており、うち postNote / addColumn / updateColumn / fetchUserLists 等は
  呼び出し元(ComposeBar.svelte、AddColumnModal.svelte 等)が独自にエラーモーダル/インライン表示を持つため、
  同じ失敗がバナーとモーダルの二重表示になる。
- Renote・リアクション・お気に入り・投票・クリップ追加・カラム名変更・カラム通知設定変更など、呼び出し元に
  エラー表示を持たない操作は、バナーが唯一のユーザー通知手段になっている。
- ドラッグ&ドロップ/ペインレイアウト操作、`loadMore`、起動時のカラム再開など、バックグラウンド・頻発・非同期的な
  失敗もバナーで表示されており、ユーザー操作への直接応答ではないため過剰に目立つ。

Issue #183 の要望: バナーはBackstageログと重複するため不要。ユーザーへの注意喚起が必要な場合はモーダルで出す。

## 対象範囲

「ユーザー直接操作」の失敗のみ新規モーダル対象とする(今回のスコープ)。ドラッグ操作・ペインレイアウト操作・
`loadMore`・起動時のカラム再開は対象外とし、Backstageログのみに記録する(頻発する可能性がある操作やUI操作の
連続性を妨げないため)。

## 設計

### 1. `errorModal` state と `#failModal` の新設 (`store.svelte.ts`)

```ts
errorModal = $state<string | null>(null);

#failModal(e: unknown) {
  const msg = String(e);
  this.errorModal = msg;
  this.#log("error", msg, e instanceof ForbiddenError ? e.accountId : undefined);
}
```

`#fail` と同じログ記録を行うが、`error`(バナー用)ではなく `errorModal` にセットする点のみ異なる。

### 2. 呼び出し元の振り分け

現行の `#fail(e)` 呼び出し(25箇所)を、次のいずれかに置き換える。

**`#failModal(e)` に置き換え(ユーザー直接操作、新規モーダル対象):**
- `renote()` (:1511)
- `react()` (:1580)
- `favorite()` / お気に入りトグル (:1608)
- `votePoll()` (:1652)
- `addNoteToClip()` (:1637)
- `renameColumn()` (:420)
- `setColumnNotify()` (:440)

**`#logFailure(e)` に置き換え(ログのみ、モーダルなし):**
- ドラッグ/ペインレイアウト操作: `moveTab`(:623), `endDragGroup`(:649), `persistGroupWidth`(:661),
  `setGroupAuto`(:671), `splitPane`(:685), `discardEmptyGroup`(:702), `setPaneAuto`(:766), `resizePane`(:775)
- バックグラウンド処理: `loadMore`(:1460)、起動時のカラム再開(:249, :254)
- 呼び出し元が既に独自エラー表示を持つもの: `addColumn`(:1020, re-throw)、`fetchUserLists`(:1067,
  re-throw)、`fetchAntennas`(:1076, re-throw)、`fetchChannels`(:1085, re-throw)、`resolveUser`(:1095,
  re-throw)、`postNote`(:1501, re-throw)、`deleteNote`(:1521, re-throw)、`listClips`(:1616, re-throw)、
  `createClip`(:1627, re-throw)

re-throw の有無・呼び出しシグネチャ・楽観的更新のロールバック処理(reaction/favorite/votePollのbackups復元等)は
既存のまま変更しない。変更するのは失敗時に呼ぶメソッド(`#fail` → `#failModal` または `#logFailure`)のみ。

`reportError(e)`(MediaGrid用、:390-392)は `#logFailure` ベースのまま維持する(背景的な画像読み込み失敗のため
対象外)。

### 3. `#fail` の削除

全呼び出し元を上記のいずれかに置き換えた後、`#fail` メソッド自体と `error` state (:120)、およびそれらに依存する
コードを削除する。

### 4. UI: `App.svelte`

- 既存のバナー(:129-138, `{#if app.error}...`)を削除。
- 代わりに、`errorModal` が非nullのとき `Modal.svelte` を使ったエラーモーダルを表示する。
  `ComposeBar.svelte:757-766` の既存パターン(`title="エラー"`, `onclose` でクローズ, 「わかった」ボタン)を踏襲する。

```svelte
{#if app.errorModal}
  <Modal title="エラー" onclose={() => (app.errorModal = null)}>
    <p>{app.errorModal}</p>
    <div class="flex justify-end">
      <Button onclick={() => (app.errorModal = null)}>わかった</Button>
    </div>
  </Modal>
{/if}
```

### 5. テスト

- `store.svelte.test.ts:223` 付近の既存テスト(`app.error` を検証しているもの)を、`app.error` 削除に合わせて
  `app.errorModal` ベース、または対象メソッドが `#logFailure` を使うことを検証する形に更新する。
- 新規: `renote`/`react`/`favorite`/`votePoll`/`addNoteToClip`/`renameColumn`/`setColumnNotify` が失敗時に
  `app.errorModal` をセットすることを検証するテストを追加する。
- 新規: ドラッグ/ペインレイアウト操作系・`loadMore`・起動時カラム再開が失敗時に `app.errorModal` を変化させ
  ないこと(ログのみ)を検証するテストを追加する(既存挙動の回帰防止)。

## 影響範囲外(変更しない)

- ComposeBar.svelte / AddColumnModal.svelte / NoteMenu.svelte 等、既に独自のエラー表示(モーダル/インライン)を
  持つコンポーネントの表示ロジック自体は変更しない。
- 楽観的更新のロールバック処理、re-throw の有無、各メソッドのシグネチャ。

# Issue #614: 表示設定の全量保存で並行する書き込みが消える

## 問題

`UiPrefs` の保存は全量置換で、呼び出し側は `{ ...app.ui, 変更分 }` のスナップショットを作って送る。

- Rust: `commands/mute.rs::set_ui_prefs` → `SettingsStore::save_ui` は `guard.ui = prefs.clone()` で丸ごと置換する。
- フロント: `store.svelte.ts::setUiPrefs` は IPC の完了後に `this.ui = {...prefs}` で置換する。`this.ui` が更新されるのは **IPC が返ったあと**。

したがって、IPC 往復の最中に別の保存が始まると、後発のスナップショットには先発の変更が入っておらず、後に届いた方が勝って先発の変更がディスク上で消える。メモリ上(`this.ui`)は後で書き戻されるので、再起動まで画面と `settings.json` が食い違う。

実在する自動書き込み: `recordEmojiUsage`、`#applyTheme` / `#applySyntaxTheme` のフォールバック(`theme: "auto"` / `codeHighlightTheme: "auto"`)。`recordEmojiUsage` は `this.ui.recentEmojis` を IPC の前に読むので、素早く 2 回呼ぶと片方の履歴も消える。

実行時の書き込み経路は `set_ui_prefs` だけで(`save_ui` の他の呼び出しはテスト)、書き手は単一ウィンドウのフロントだけ。

## 方針(フロントのみ・Rust は変えない)

**保存を直列化し、保存の実行時点の最新の `this.ui` に変更分をマージする。**

`AppStore` に追加:

```ts
updateUiPrefs(patch: Partial<UiPrefs> | ((cur: UiPrefs) => Partial<UiPrefs>)): Promise<void>
```

- 内部の Promise チェーン(`#uiPrefsQueue`)で、呼び出し順に 1 件ずつ実行する。
- 各タスクは、実行開始時点の `this.ui` に `patch`(関数なら `patch(this.ui)` の戻り値)をマージし、IPC を送り、成功したら `this.ui` を正規化して更新する(既存の `setUiPrefs` の正規化をそのまま使う)。
- あるタスクが失敗(Rust の検証エラーなど)しても、後続のタスクは止めない。失敗は、そのタスクを呼んだ呼び出し元にだけ reject として返す。失敗したタスクは `this.ui` を変えない。
- 関数形は、「最新の値から派生する」更新に使う(`recordEmojiUsage` の `recentEmojis`、削除済みテーマへのフォールバックを「まだ同じテーマのときだけ」行う条件)。

`setUiPrefs(prefs: UiPrefs)`(全量置換)は **公開をやめる**。新しい全量置換の呼び出しが入らないよう、private の `#writeUiPrefs` に降格し、`updateUiPrefs` の中だけから呼ぶ。

### 呼び出し側の書き換え

| 場所 | 変更 |
| --- | --- |
| 設定の各節(About / Accounts / Appearance×5 / Background / Data / Developer / ExternalIntegration / Layout / Mobile) | `app.setUiPrefs({ ...app.ui, a, b })` → `app.updateUiPrefs({ a, b })` |
| `store.svelte.ts` の `setKeymap` / `setPinnedEmojis` | `updateUiPrefs({ keymap })` / `updateUiPrefs({ pinnedEmojis })` |
| `recordEmojiUsage` | `updateUiPrefs((cur) => ({ recentEmojis: withRecentEmojiUsage(cur.recentEmojis ?? [], key) }))` |
| `#applyTheme` のフォールバック | `updateUiPrefs((cur) => cur.theme === theme ? { theme: "auto" } : {})` |
| `#applySyntaxTheme` のフォールバック | 同様に `codeHighlightTheme` を条件にする |
| `AppearanceSection` の `removeCustomSyntaxTheme` | `app.ui.codeHighlightTheme` を読む部分を関数形にする |

`AppearanceSection:138`(`removeCustomTheme`)の `clearing ? "auto" : app.ui.theme` は、「消すテーマが選択中のときだけ `theme` を変える」ので、パッチ側では `clearing` のときだけ `theme: "auto"` を含める(含めなければ最新値が残る)。

### 保存した後に `this.ui` を信用してよい理由

直列化によって、タスクの開始時点で、先行タスクの `this.ui` 更新が済んでいる。各節が、自分が編集していないキーを送らなくなるので、他の節の保存や自動書き込みと競合しない。

## 範囲外

- 同じキーを 2 つの画面が同時に書く場合は、後勝ちのまま(これは意図した挙動)。
- Rust 側のフィールド単位マージ(`set_ui_prefs` を部分更新にする案)は採らない。`null`(クリア)と未指定の区別が `specta` の型に出てしまい、生成される TS 型と全呼び出しに波及するため。書き手が単一ウィンドウである限り、フロントの直列化で十分。
- `trustSummalyProxy` のように、`updateUiPrefs` の完了後に別の IPC を呼ぶ手順はそのまま。

## テスト(Vitest、`store.svelte.test.ts`)

1. IPC を遅延させ、`updateUiPrefs({a})` と `updateUiPrefs({b})` を続けて呼ぶ。2 回目の IPC の引数が `a` と `b` の両方を含む(修正前は `a` が欠ける)。
2. `recordEmojiUsage("x")` と `recordEmojiUsage("y")` を続けて呼ぶ。最後の IPC の `recentEmojis` に両方が入る。
3. 1 件目が reject しても、2 件目は実行され、1 件目の呼び出し元だけが reject を受け取る。`this.ui` は 1 件目の変更を含まない。
4. 関数形のパッチが、直前のタスクの結果を見る。
5. 既存の設定各節のテスト(`AboutSection` / `LayoutSection` / `ExternalIntegrationSection` / `AppearanceSection` と `store.svelte.test.ts`)を、`updateUiPrefs` のパッチ引数を検証するように更新する。

## 確認

`cd frontend && pnpm check && pnpm test`。Rust は変更なし。実画面での確認は、設定の保存中にリアクションを付けて `settings.json` の両方が残ることを、Xvfb 越しの dev ビルドで見る。

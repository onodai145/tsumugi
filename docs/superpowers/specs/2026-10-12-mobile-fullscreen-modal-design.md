# モバイルUIのモーダルを全画面ページ化する

Issue: #450

## 背景・目的

モバイルUIでモーダル(箱型ダイアログ)が多用されており、箱の余白や二重スクロールで見づらい。モバイルUI(`app.useMobileUi()`が真)では、モーダルを全画面ページ(上部に「←」とタイトル)として表示し、戻るジェスチャ/ボタンで閉じられるようにする。PC版の見た目・挙動は変えない。

## スコープ

対象:
- `ui/Modal.svelte`を使う全画面化: 設定・カラム追加/編集・検索・プロフィール・フォロー一覧・カラム設定・ドライブ選択・予約一覧
- `App.svelte`のモバイル投稿用独自オーバーレイを`Modal`へ置き換え

対象外(現状維持):
- エラー表示(`App.svelte`・`ComposeBar.svelte`の`<Modal title="エラー">`)は`compact`指定で従来の箱型のまま
- `ConfirmDialog`、`MediaViewer`、各ポップオーバー/ドロップダウン(z-1010)
- PC版UI

## 設計

### 1. `Modal.svelte`のモバイル全画面レイアウト

- 切り替え条件は`app.useMobileUi()`。設定の「表示」でのUIモード上書きにも追従する(OS判定ではない)。
- 新しい任意prop `compact?: boolean`(既定false)。`compact`指定時はモバイルでも従来の箱型で、履歴にも参加しない。
- 全画面時の構造:
  - 最外殻: `fixed inset-0 z-[1000] flex flex-col bg-background`。黒オーバーレイ・角丸・枠線なし。
  - 上部バー: 戻るボタン(←)+タイトル。戻るボタンは`onclose`を呼び、`data-testid="modal-close"`を維持する。上端は`var(--safe-top)`の余白。
  - 本文: `flex-1 min-h-0 overflow-y-auto`の1枚だけがスクロールする。`p-4`相当のパディングと、下端`var(--safe-bottom)`の余白。
  - 背景クリックでは閉じない(背景が無い)。Escapeでは従来どおり閉じる。
  - `width` / `maxHeight` propはモバイル全画面では無視する。
- `DrivePicker` / `ScheduledModal`は`maxHeight`指定で`flex-col`+内部スクロールを持つが、全画面では本文が`flex-1 min-h-0`になるためそのまま収まる。

### 2. 戻るジェスチャ/ボタン対応(`lib/modalHistory.ts`)

履歴の出し入れを`lib/modalHistory.ts`に切り出し、全画面表示された`Modal`のみが使う。

- 開いたとき: `history.pushState({ tsumugiModal: id }, "")`で履歴を1つ積み、モジュール内スタックに登録する。
- `popstate`: スタック最上段のモーダルだけ`onclose`を呼ぶ。重なっていても1枚ずつ閉じる。
- UIで閉じたとき(←、Escape、保存後の自動クローズ): destroyで積んだ履歴を`history.back()`で1つ戻す。その`popstate`は無視カウンタで握りつぶし、`onclose`を二重に呼ばない。
- 戻る操作で閉じたとき: 履歴は既に戻っているので、destroyでは`back()`を呼ばない(エントリごとに`popped`フラグ)。
- 同一tickで複数枚が閉じても、無視カウンタの加算で整合する。
- モーダルが無いときは履歴を積まないため、戻るで従来どおりアプリが閉じる。
- PCでUIモードを「モバイル」に上書きした場合も`pushState`/`back`は無害なので同じ経路で動く。

Androidでは生成済み`WryActivity.kt`が、`mWebView.canGoBack()`なら`goBack()`、そうでなければアプリ終了、という挙動を取る。`pushState`の履歴が`canGoBack()`に反映されることが前提(下記「実機確認」参照)。

### 3. 個別対応

**`Settings.svelte`**: モバイルでは左タブ(`w-40`)をやめ、上部の横スクロール式タブ帯にする。
- 選択中のタブは自動で見える位置までスクロールする。本文は1カラム。
- スクロールは`Modal`本文の1枚に任せ、タブ帯は`sticky top-0`で固定する。`max-h-[calc(84vh-3rem)]`・`rounded-b-[11px]`・ネガティブマージンはモバイルでは外す。
- `app.useMobileUi()`でクラスを切り替えるのみ。各セクション子コンポーネントは変更しない。
- 将来の拡張: 「一覧 → 詳細」の2階層(Android設定アプリ風)。階層ごとの戻る処理と履歴管理が要るため今回は対象外。

**モバイル投稿オーバーレイ(`App.svelte`)**: 独自の`fixed`オーバーレイを`<Modal title="投稿">`に置き換える。中身の`ComposeBar expanded`と`onPosted`で閉じる動作は維持する。

**その他の呼び出し側**(`AddColumnModal`/`SearchModal`/`ColumnSettings`/`ProfileModal`/`FollowListModal`/`DrivePicker`/`ScheduledModal`): `Modal`の変更のみで全画面になり、呼び出し側の改修は不要。`compact`を付けるのはエラー表示2箇所のみ。

## テスト方針

- Vitest `Modal.test.ts`: `app.useMobileUi()`をモバイルにした状態で、全画面クラス・背景クリックで閉じない・←とEscapeで閉じる・`width`/`maxHeight`無視・`compact`時は箱型で履歴非参加、を確認する。PC側の既存テストは無変更で通ること。
- Vitest `modalHistory.test.ts`(新規、jsdomの`history`を使用): 開くと履歴が1つ積まれる/`popstate`で最上段のみ閉じる/UIで閉じると`back()`が1回呼ばれその`popstate`では二重に閉じない/同一tick複数枚/戻る操作で閉じたときは`back()`を呼ばない。
- Vitest `Settings.test.ts`: モバイル時にタブ帯が横並びになり、タブ切り替えが動く。
- E2E: `e2e/`の既存モバイルシナリオ(`2026-09-26-mobile-e2e-design.md`)がモーダルのセレクタに依存していないか、実装計画の段階で確認する。`data-testid="modal-close"`は維持するため基本は壊れない想定。

## ドキュメント更新

- `docs/design/style-guide.md`: モバイルのモーダル規約(全画面を既定、`compact`は短い通知系のみ)を追記する。
- `docs/guide/user-guide.md`: モバイルで戻る操作により画面が閉じることを追記する。

## 実機確認(Android)

- `pushState`の履歴が戻るジェスチャで辿れるか(最大の未検証点)。反映されない場合は、Kotlin側でJSブリッジ経由の戻る処理に切り替える代替案を取る。
- ソフトキーボード表示時の投稿画面で入力欄が隠れないか。隠れる場合は`dvh`系の高さ指定で対処する。
- 安全領域(ノッチ、ナビゲーションバー)が上下で正しく確保されるか。
- `ProfileModal` → `FollowListModal`など、重なったときの戻る挙動。

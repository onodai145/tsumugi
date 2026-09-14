# モバイル版: タブ/カラムの並び替え (Issue #354)

## 背景

`Column.svelte` のタブバーは、タブ自体（各タブ要素）とカラム自体（`GripVertical`グリップアイコン）の
並び替えを、いずれもHTML5 native drag-and-drop（`draggable`属性 + `dragstart`/`dragover`/`dragend`）
のみで実装している。native DnDはマウス操作専用のAPIであり、タッチ操作では`dragstart`が発火しないため、
モバイル版（`app.useMobileUi()`が`true`）ではタブの並び替えが一切できない。Issue #354で報告された
症状はこれ。カラム自体の並び替えも同じ仕組みのため同様に機能しない。

デスクトップ版の既存動作（マウスでのnative DnD）は変更しない。本設計はモバイル版
（`app.useMobileUi()`が`true`）にのみ、タッチ向けの並び替え手段を追加するもの。

## スコープ

- タブの並び替え（カラム内での順序変更、グループ間移動を含む）: 長押しドラッグで対応。
- カラム自体の並び替え: 長押しドラッグ、および「…」メニューへの「左に移動」「右に移動」の
  2つの手段で対応。対象は`topLevelLeafGroupIds(app.paneRoot)`（最上位row直下のleafカラムのみ、
  `frontend/src/lib/swipeNav.ts`の既存関数を流用）。ネストした分割配下のカラムは対象外
  （既存のモバイルスワイプ機能と同じスコープ制限を踏襲）。
- タブバー内でのオートスクロール（多数タブがあり並び替え中に端まで来た場合の自動スクロール）は
  v1スコープ外。既知の制限として明記する。
- デスクトップ版でのタッチ操作（タッチ対応ノートPC等）は非スコープ。あくまで`useMobileUi()`が
  `true`の場合のみ長押しドラッグ・メニュー項目を有効化する。

## 全体アーキテクチャ

新規モジュール`frontend/src/lib/longPressDrag.ts`に、「長押し検知→ドラッグ状態遷移」を行う
DOM非依存の状態機械を切り出し、タブ・カラムグリップの両方から共有する。

状態機械が扱うのは以下のみで、実際のDOM操作（`setPointerCapture`、視覚効果の適用、
`elementsFromPoint`によるヒットテスト）は呼び出し側（`Column.svelte`）が行う:

- `pointerdown`時刻と座標を受け取り、長押しタイマー（400ms）を開始する
- `pointermove`で座標更新。タイマー成立前に閾値（8px）を超えたら`cancelled`を返す
- タイマー成立で`armed`を返す（呼び出し側はここで`vibrate("light")`・視覚効果・
  `app.startDragTab`/カラムドラッグ開始を行う）
- `pointerup`/`pointercancel`で終了を返す

既存のデスクトップ用DnD実装（`draggable`属性、`ondragstart`等）はそのまま残す。
`Column.svelte`側で`pointerdown`ハンドラを追加し、`e.pointerType === "touch"`かつ
`app.useMobileUi()`の場合のみ長押しドラッグのフローに入る。マウス操作時は何もせず、
既存のnative DnDフローに委ねる。

## タブの並び替え

各タブ要素に`onpointerdown`を追加する。

1. **長押し検知**: `pointerdown`（`pointerType === "touch"`）で`longPressDrag.ts`のタイマーを開始。
   8px以上動くか、タイマー成立前に`pointerup`が来たら中断し、これまで通りタップ（`onclick`での
   タブ切り替え）・横スクロール（タブバーの`overflow-x-auto`によるネイティブスクロール）として扱う。
2. **ドラッグ開始**: タイマー成立で`vibrate("light")`（`frontend/src/lib/ipc.ts`の既存`vibrate()`、
   `isMobilePlatform`と`app.ui.hapticsEnabled`を尊重する既存ガードに従う）、
   `el.setPointerCapture(e.pointerId)`、対象タブ要素に持ち上げ視覚効果（`scale`+`shadow`のCSSクラス）
   を付与し、`app.startDragTab(tabId)`（既存メソッド、変更不要）を呼ぶ。
3. **ドラッグ中**: `pointermove`のたびに`document.elementsFromPoint(e.clientX, e.clientY)`で
   その座標にある要素から`data-tab-id`/`data-group-id`（新規に付与するdata属性）を持つ要素を探し、
   ヒットしたタブ要素があれば既存の`app.dragOverTab(groupId, tabId)`を、タブバーの空き領域なら
   既存の`app.dragOverTabBarEnd(groupId)`を呼ぶ。これは現行の`ondragover`ハンドラが呼んでいるのと
   同じメソッドで、ロジックの重複はない。
4. **ドラッグ終了**: `pointerup`/`pointercancel`で持ち上げ視覚効果を解除し、`app.endDragTab()`
   （既存、内部で`commands.moveTab`→`commands.reorderGroups`→`commands.loadPaneLayout`を呼ぶ）
   を呼ぶ。

オートスクロールは実装しない（v1スコープ外、既知の制限）。

## カラムの並び替え

### 長押しドラッグ + 前後ヒント

グリップアイコン（`GripVertical`）に同じ長押し検知を追加する。モバイル版は1カラムが画面全幅で
表示されるため、隣のカラムが画面外にあり「どこに入れ替わったか」を並べて見せることができない。
そのため、位置に追従する自由なドラッグではなく、「前へ/次へ」の1ステップ移動として実装する。

1. **ドラッグ開始**: 長押し成立で`vibrate("light")`。画面上部に「◀ 前へ / 次へ ▶」のヒント
   オーバーレイを表示する。
2. **ドラッグ中**: 指のX移動量（開始位置からの差分）が閾値（40px）を超えたら該当方向
   （前/次）をハイライト表示する。閾値未満に戻れば非アクティブ表示に戻る（トグル式）。
   位置に追従した連続移動（複数ステップ先への移動）は行わない。
3. **ドラッグ終了**: ハイライト状態が「前へ」または「次へ」であれば、共通関数
   `moveColumnAdjacent(groupId, direction)`（後述）を呼ぶ。ハイライトなしで指を離した場合は
   何もしない。

対象は`topLevelLeafGroupIds(app.paneRoot)`に含まれるカラムのみ。先頭カラムでの「前へ」、
末尾カラムでの「次へ」は無効（ヒントを非活性表示にする）。

### 「…」メニューへの「左に移動」「右に移動」

既存の`menuOpen`ドロップダウン（`タブを追加`/`右に分割`/`下に分割`/`カラム設定`）に、
`topLevelLeafGroupIds`内での現在位置に応じて追加する。

- 先頭カラムでは「左に移動」を非表示、末尾カラムでは「右に移動」を非表示。
- クリックで`moveColumnAdjacent(groupId, direction)`を呼ぶ（長押しドラッグと共通の関数、
  ロジックの重複はない）。
- モバイル・デスクトップ両方のUIモードでこのメニュー項目を表示する（メニュー自体は既に
  両方のUIモードに存在するため）。デスクトップでは既存の座標ベースnative DnDと共存する形に
  なるが、確実に動かせる代替手段として有用。

### 共通関数 `moveColumnAdjacent`

`store.svelte.ts`に、既存の`endDragTab`/`endDragGroup`と並べて新規メソッドとして追加する。

```
moveColumnAdjacent(groupId: string, direction: "prev" | "next") {
  const order = topLevelLeafGroupIds(this.paneRoot);
  const i = order.indexOf(groupId);
  const j = direction === "prev" ? i - 1 : i + 1;
  if (i < 0 || j < 0 || j >= order.length) return; // 端では何もしない
  // this.groups内で2つのgroupIdの位置を入れ替え、reorderGroupsで永続化する
  ...
  await unwrap(commands.reorderGroups(this.groups.map((g) => g.id)));
}
```

`this.groups`は全カラムのフラット配列（ネスト構造を問わない全体順序）であり、既存の
`endDragTab`末尾で`commands.reorderGroups(this.groups.map((g) => g.id))`を呼んでいるのと
同じ永続化経路を使う。ネストした分割配下のカラムの相対順序には触れない
（swapするのは`order[i]`と`order[j]`の2つの top-level leaf groupId のみ）。

## テスト方針

- `longPressDrag.ts`の状態機械（タイマー成立/中断、閾値超えでの中断）は、DOM非依存の
  純粋なロジックとして切り出し、Vitestの`vi.useFakeTimers()`でユニットテストする。
- `moveColumnAdjacent`（および内部で使う「`topLevelLeafGroupIds`上のインデックス入れ替え」
  ロジック）も純粋関数として切り出し、Vitestでユニットテストする（先頭/末尾での無効化、
  ネスト配下カラムが対象外であることを含む）。
- 実際のタッチ長押し・`pointercancel`・ネイティブスクロールとの共存については、jsdomでは
  検証できないため、Android実機での動作確認を完了条件に含める（`chrome://inspect`での
  pointerイベントトレースも活用する）。過去に横スワイプ実装（Issue #296）で机上レビューのみ
  完了と判断し実機で機能しないことが判明した教訓
  （`touch-action-pan-y-pointercancel-native-scroll-conflict`）があるため、今回も実機確認を
  省略しない。

  なお「長押し成立まで指が静止しているのだから、ネイティブスクロールとの競合リスクは低い」
  という説明は誤りなので採らない。スクロールと独自ジェスチャーのどちらにポインターを渡すかの
  調停は、ブラウザが`touch-action`というCSSプロパティを見てジェスチャー開始時点で決めるもので
  あり、JS側の`armed`フラグがタイマー発火時にどうなっているかは調停に影響しない。

  現状の実装では`touch-action: none`を指定しているのはカラムのグリップだけで（グリップは
  ネイティブスクロールを必要としないため無条件に安全）、タブ要素側には指定していない。タブ要素は
  タブバーの横スクロール対象そのものであり、ここに`touch-action: none`を置くとタブバーの
  横スクロールが完全に効かなくなるため、机上では決められない。つまり**タブの長押しと横スクロールの
  調停は未解決**であり、Task 8の実機確認で個別に検証して結論を出す必要がある。実機では以下を
  それぞれ明示的に確認する。

  - タブ: 長押しがタブバーの横スクロールに競り負けないか（長押し中に`pointercancel`が飛んで
    ドラッグが始まらない／始まってもすぐ中断されないか）。
  - タブ: 逆に、タブバーを横スクロールしようとしただけで長押し判定が誤発火しないか。
  - カラム: グリップの長押しドラッグが、カラム間のScroll Snap（横スワイプ）に競り負けないか。

## 非スコープ・既知の制限

- タブバー内でのオートスクロール（並び替え中に端まで来た場合の自動スクロール）。
- カラムドラッグ中の連続的な複数ステップ移動（1ジェスチャーにつき隣接1つ分の移動のみ）。
- ネストした分割（split）配下のカラムの並び替え（既存のモバイルスワイプ機能と同じスコープ外）。
- デスクトップUIでのタッチ操作対応。

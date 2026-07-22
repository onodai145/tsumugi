# カラムの縦分割（ペイン化） 設計 (Issue #31)

## 背景

現在カラム(`ColumnGroup`)は横一列にしか並べられない（`App.svelte` の `.columns` は `display:flex; flex-direction:row` の1行固定）。最近の大型モニターでは横だけでなく縦の分割も欲しい、という要望。tmuxのペイン分割に近い自由な2D分割を、「ペイン」という新概念で実現する。

## スコープ

- 各 `ColumnGroup`（1ペイン=1ColumnGroup、中身のタブ集合はそのまま）を任意の位置で上下左右に分割できる、tmux風の再帰的2分割ツリー。
- ペイン境界のドラッグによるリサイズ（幅・高さとも）。
- ドラッグ&ドロップによるペインの移動（行をまたいだ移動も含む）。
- 対象外: ペイン中央ドロップでのタブ統合、ペインのズーム/最大化、キーボードショートカットでの分割操作。既存のタブ単位のドラッグ&ドロップ（同一グループ内タブ並べ替え/別グループへの移動）はそのまま維持し変更しない。

## データモデル（`src-tauri/src/domain/pane.rs` 新設）

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SplitDirection { Row, Column } // Row=横並び, Column=縦並び

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PaneNode {
    Leaf { id: String, group_id: String },
    Split { id: String, direction: SplitDirection, children: Vec<PaneChild> },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaneChild {
    pub node: PaneNode,
    /// 親の主軸方向におけるこの子のサイズ。
    /// 親が Row のときは px（現行の ColumnGroup.width と同じ意味・同じ 220-720 clamp）。
    /// 親が Column のときは相対ウェイト（CSSの `flex-grow` にそのまま渡す正の数。
    /// 兄弟間で合計1.0である必要はない＝厳密な正規化はしない。描画は
    /// `flex: {size} 1 0` なので、実際の高さ比率は「自分のsize ÷ 兄弟のsize合計」に
    /// ブラウザが自動で正規化してくれる。初期値は 1.0＝均等割り）。
    /// node が Leaf/Split どちらであっても同じ意味を持つ（Splitがネストしても幅/高さが必ず定まる）。
    pub size: f32,
    /// true なら size を無視し `flex:1 1 0` で余白を自動的に埋める。Row の子にのみ意味を持つ
    /// （Column の子は常に size ウェイトで配分するため auto は無視する）。
    pub auto: bool,
}
```

- `Leaf.id` / `Split.id` は各ノードを一意に指すUUID。リサイズ・移動コマンドの対象指定に使う。
- `ColumnGroup.width` / `ColumnGroup.auto` フィールドは廃止し、`PaneChild.size` / `PaneChild.auto` に完全統合する（二重管理を避けるため）。既存の `set_group_width` / `set_group_auto` コマンドは削除し、後述の `resize_pane` / `set_pane_auto` に置き換える。
- ルート全体は1つの `PaneNode`（`SettingsData.pane_layout: Option<PaneNode>`, `#[serde(default)]`）として保存。

## 永続化（`src-tauri/src/store/settings.rs`）

```rust
pub fn load_pane_layout(&self) -> Result<PaneNode> {
    // Some ならそれを返す。
    // None（旧バージョンからの移行）なら、既存 groups を order 順に並べた
    // Split{ direction: Row, children: [PaneChild{ Leaf{group_id}, size: DEFAULT_WIDTH, auto:false }, ...] }
    // をその場で組み立てて返す。ファイルへの書き込みは行わない（初回の分割/移動/リサイズ操作時に確定して保存）。
}
pub fn save_pane_layout(&self, root: &PaneNode) -> Result<()> {
    // まるごと書き換えて保存（他の設定と同じ「全体読み込み→変更→まるごと保存」方式）。
}
```

- `delete_empty_groups`（タブが0になったグループの自動削除）と連動させ、木からも該当 `Leaf` を除去する。

**挿入・削除の正規化ルール（tmuxに倣い、常に「隣接する1ペイン」とだけ取り分をやり取りする。無関係な兄弟には一切影響させない）:**

- **挿入(insert_sibling)**: `reference` の直後に新規ペインを挿入するとき、`reference` 自身の `size` を半分にし、新規ペインがもう半分を受け取る。他の兄弟の `size` は変更しない（Row=px、Column=ウェイトのどちらでも同じ計算式。`reference` の親が挿入方向と異なる Split の場合は後述の「ラップ」を参照）。
  - `reference` の親が既に挿入方向と同じ `direction` の Split なら、その子リストに `reference` の直後の位置へ差し込むだけでよい。
  - `reference` の親が異なる `direction`（またはrefがルート直下でSplitに包まれていない）の場合は、`reference` の位置を「`reference` 自身 + 新規ペイン」の2子からなる新しい `direction` の Split で置き換える（＝ラップ）。ラップ後の新Split自体が親から見て持つ `size`/`auto` は、ラップ前に `reference` が持っていた値をそのまま引き継ぐ（外側の並びは変わらない）。ラップ内部の2子の `size` は、Row(px)なら`DEFAULT_WIDTH`を折半、Column(ウェイト)なら 1.0/1.0（均等）とする（内側は新しい軸なので `reference` の元の `size` は単位が異なり流用できないため）。
- **削除(remove_group)**: `Leaf` を1つ取り除いたら、空いた `size` は隣接する兄弟1つに全て譲る（直前の兄弟があればそちらへ、無ければ直後の兄弟へ）。他の兄弟には影響しない。取り除いた結果、親の子が1つだけになったら、その親 `Split` ノードを残った子で置き換えて畳む（tmuxのペイン閉じと同じ挙動）。畳んだ際、生き残った子の `size`/`auto` は、畳まれた `Split` 自身が親から見て持っていた `PaneChild.size`/`auto` を引き継ぐ（1子Splitの中にまた1子Splitが残るケースも同様に再帰的に畳む）。
- **移動(move_pane)**: 内部的には「削除」→「挿入」の組み合わせ（それぞれ上記ルールをそのまま適用）。

## Tauriコマンド（`src-tauri/src/commands/column.rs`）

```rust
/// reference_group_id の隣に空の新規グループ(タブなし)を挿入し、その ColumnGroup を返す。
/// direction: Row(右に分割) / Column(下に分割)
async fn split_pane(reference_group_id: String, direction: SplitDirection) -> Result<ColumnGroup>

/// dragged_group_id を木から取り外し(親が1子になれば畳む)、target_group_id の指定エッジに挿入する。
/// edge に応じて Row/Column どちらの方向に挿入するか決まる。
async fn move_pane(dragged_group_id: String, target_group_id: String, edge: Edge) -> Result<()>
// Edge = Left | Right | Top | Bottom

/// ノード(Leaf/Split どちらのidでも可)の主軸サイズを更新する。
async fn resize_pane(node_id: String, size: f32) -> Result<()>

/// ノードの auto フラグを更新する（Row内でのみ意味を持つ）。
async fn set_pane_auto(node_id: String, auto: bool) -> Result<()>

/// 現在の木を返す（起動時のレイアウト復元用）。
async fn load_pane_layout() -> Result<PaneNode>
```

- `split_pane` は空グループを作るだけ。フロントは戻り値の `group_id` を使って既存の `AddColumnModal` を「このグループにタブ追加」モードで開く。ユーザーがキャンセルしたら、空グループごと木から取り除く（`delete_empty_groups` 相当のロジックをこのタイミングでも呼ぶ）。
- 挿入位置・初期サイズは「## 永続化」節の **挿入(insert_sibling)** ルールをそのまま使う（`reference_group_id` の取り分を半分に分け合うだけで、他の兄弟ペインの幅/高さは変わらない。tmuxの分割と同じ体感になる）。既存の「＋カラム」ボタンの挙動は変更しない（後述）。
- `move_pane`: エッジがLeft/RightならRow方向、Top/BottomならColumn方向として、「## 永続化」節の **削除→挿入** をそのまま適用する（target が挿入時の `reference` になる）。
- `close_column`（タブを閉じる）は変更なし。内部で呼んでいる `delete_empty_groups` の実装に「木からも畳む」処理を足すだけで、ペイン削除は自然に連動する。
- 既存の `add_column`(`group_id: None`) は「グローバルに新規グループを作る」だったが、木構造導入後は「フォーカス中の行の末尾に追加」に変更する。具体的には、フロント側で `app.focusedGroupId` から所属する最も近い祖先の Row Split を特定し、そのグループIDを `split_pane(reference_group_id, Row)` 相当のヘルパーに渡す（未フォーカス時はルートの最初のRowの末尾）。ただし**この「＋カラム」経由の追加だけは初期サイズを既定幅 `DEFAULT_WIDTH`(px)にする**（前述の「半分に割る」初期サイズルールは分割ボタン専用で、「＋カラム」には適用しない）。これにより「横スクロールで多数のカラムを並べていく」既存の使い方・体感は一切変えない。
- `lib.rs` の `specta_builder()` に新規コマンドと `PaneNode`/`PaneChild`/`SplitDirection`/`Edge` 型を登録する。`set_group_width`/`set_group_auto`/`reorder_groups` コマンドは削除（`reorder_groups` は木の並び順そのものが真実になるため不要）。

## フロントエンド

### store (`frontend/src/lib/store.svelte.ts`)

- `paneRoot: PaneNode` を保持。起動時 `commands.loadPaneLayout()` で取得。
- `splitPane(referenceGroupId, direction)` / `movePane(draggedGroupId, targetGroupId, edge)` / `resizePane(nodeId, size)` / `setPaneAuto(nodeId, auto)` をラップして `paneRoot` を楽観的に更新（既存の `setGroupWidthLocal`/`persistGroupWidth` と同じ「ドラッグ中はローカルのみ更新→ポインタアップで永続化」パターンをリサイズに踏襲）。

### UI (`ui/Pane.svelte` 新設)

再帰コンポーネント。
- `Leaf` → 既存の `Column.svelte` をそのまま描画。
- `Split(direction: Row)` → `display:flex; flex-direction:row; overflow-x:auto`。各子は `node` が Leaf/Split どちらでも `flex: ${auto ? "1 1 0" : "0 0 " + size + "px"}` で統一的にサイズ指定できる（Leaf/Splitで分岐しない）。
- `Split(direction: Column)` → `display:flex; flex-direction:column`。各子は `flex: ${size} 1 0`（`size`をそのまま`flex-grow`として使う。兄弟間で合計1.0である必要はなく、ブラウザが自動的に「自分のsize÷合計size」の比率で高さを割り付ける）。境界に縦方向リサイズハンドルを置き、ドラッグ中は挟む2つの子（ドラッグハンドルの上と下）の `size` を「2子の合計ウェイトを保ったまま」ペアで増減させる（既存 `Column.svelte` の横方向リサイズ `onResizeDown/Move/Up` の縦版だが、Rowと違い2子ペアで連動させる点が異なる）。ポインタアップ時に2子それぞれについて `resize_pane(node_id, size)` を1回ずつ呼んで永続化する（`resize_pane` は1ノードずつの更新なので、ペアの反映には2回呼ぶ）。
- `App.svelte` の `.columns` div直書きを `<Pane node={app.paneRoot} />` 1つに置き換える。

### UI (`ui/Column.svelte` 拡張)

- ヘッダに分割ボタンを追加。クリックで「右に分割 / 下に分割」の小さいメニューを開き、選択で `app.splitPane(group.id, direction)` を呼ぶ→新規グループIDで `AddColumnModal` をタブ追加モードでオープン。
- 既存の grip ドラッグ（`app.draggingGroupId`/`dragOverGroup`等）を拡張。ドラッグ中、ドロップ先候補の `Column.svelte` の四辺に薄いドロップゾーンオーバーレイ（Left/Right/Top/Bottom）を表示し、離した位置に応じて `app.movePane(draggedGroupId, targetGroupId, edge)` を呼ぶ。中央エリアは対象外（既存のタブドラッグとは独立した機能のため、ペイン移動のドロップゾーンには含めない）。同一行内での並べ替えは Left/Right エッジへのドロップとして自然に包含される。

### UI (`ui/ColumnSettings.svelte` 拡張)

現状は「幅（px）」の固定/自動トグル＋数値入力のみ（`group.width`/`group.auto` を直接編集）。`ColumnGroup.width`/`auto` 廃止に伴い、このモーダルはグループが属する `PaneChild`（自分の `Leaf.id` を親から見た子として持つ `size`/`auto`）を編集する形に置き換える。

- 親が **Row** の場合: 現行と同じUI（固定/自動ラジオ＋px数値入力 220〜720）。値変更は `app.resizePane(node_id, size)` / `app.setPaneAuto(node_id, auto)` を呼ぶ。
- 親が **Column** の場合: ラジオは出さず（Column内には auto の概念が無いため）、「高さ（%、5〜95）」の数値入力を表示する。表示/入力時は他の兄弟の `size` 合計 `othersSum`（自分以外の `size` の合計）を使って相互変換する: 表示は `size / (size + othersSum) * 100`、入力された%を `p` とすると新しい `size = othersSum * p / (100 - p)` を計算して `app.resizePane(node_id, size)` を呼ぶ。他の兄弟の `size` 自体は変更しない（`othersSum` は変わらないので、結果として入力した%が正しく反映される）。95%までに制限するのは 100% ちょうど(他の兄弟が高さ0になる)を避けるため。
- `resize_pane`（ドラッグ・数値入力とも）は「対象ノード1つの `size` をその場で上書きする」操作。Column内では他の兄弟の `size` を変えないため、兄弟間の合計は1.0固定ではなくなるが、`flex: {size} 1 0` はブラウザが自動的に合計に対する比率で高さを割り付けるため、これは想定通りの挙動であり問題ない。

## マイグレーション／互換性

- 旧バージョンのJSON（`pane_layout`キー無し、`ColumnGroup.width`/`auto` 有り）を読み込む際は、`pane_layout: None` → 起動時に `load_pane_layout()` が group一覧(order順)から `Split{Row, [...]}` を組み立てて返すため、**アップデート後も見た目は変わらない**（横一列のまま）。
- `ColumnGroup` からの `width`/`auto` フィールド削除に伴い、旧JSONに残っている値は読み捨てられる（`serde(default)` で無視、もしくは移行時の一度だけ `PaneChild.size` の初期値として読み取ってから捨てる）。実装時にどちらでも良いが、後者の方が既存ユーザーの幅設定を保持できるため採用する。
- 初めて分割・移動・リサイズ操作をした時点で `pane_layout` が実体化・保存される。
- `cargo test` の `generates_frontend_bindings` テストで新コマンド/型のTSバインディング生成を確認する。

## テスト

- Rust: `domain/pane.rs` に木構造の挿入/畳み込み/再正規化のユニットテストを追加（split→move→closeの一連の操作で木の整合性が保たれることを確認）。`store/settings.rs` に永続化のラウンドトリップテストを追加。
- フロント: `pnpm check` で型チェックが通ることを確認。UI動作は `cargo tauri dev` 上で手動確認（右分割/下分割/リサイズ/行またぎドラッグ移動/最後の1タブを閉じたときのペイン自動畳み込み、の5系統）。

## 非対象(YAGNI)

- ペイン中央へのドロップでの「タブ統合」
- ペインのズーム/最大化
- キーボードショートカットでの分割/移動
- `reorder_groups` に依存していた挙動の後方互換用ラッパー（呼び出し元をすべて `move_pane`/木構造ベースに置き換えるため不要）

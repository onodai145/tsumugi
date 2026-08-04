# TQL入力補完 設計 (Issue #23)

## 背景

`AddColumnModal` にはTQL(Tsumugi Query Language)を入力する箇所が2つある。

- エキスパートモード: `from ... where ...` のフルクエリを書く `<textarea class="tql-input">`
- 簡単モード: `where` 述語のみを書く `<input>`（`filterText`。フィルタ欄）

どちらも現状は自由入力＋入力後の非同期バリデーション（`app.validateTqlQuery` / `app.validateFilter`）でエラーメッセージを出すだけで、キーワード・フィールド名・演算子の入力補完はない。文法（`docs/design/filter-dsl-design.md`）を覚えていないと書けず、UI下部のヒント文字列を見ながら手打ちする必要がある。これを解消し、入力中に文脈に応じた候補をドロップダウンで出す。

## スコープ

- 対象: `AddColumnModal` のエキスパート用textarea（`Query`モード＝`from...where...`）と簡単モードのfilter input（`Predicate`モード＝`where`述語のみ）の両方。
- 補完対象: `from`/`where`キーワード、ソース名（`home`/`local`/`hybrid`/`global`/`mentions`/`cache`/`list`/`antenna`/`channel`/`user`/`tag`/`search`）、フィールド名（`docs/design/filter-dsl-design.md` §10のcanonical表記）、比較演算子（`contains`/`in`/`startswith`/`endswith`/`match`/`==`/`!=`/`<`/`>`/`<=`/`>=`/`->`/`<-`）、論理演算子続き（`&&`/`||`）。
- `list("...")` / `antenna("...")` / `channel("...")` の引数（生ID文字列）も、選択中アカウントが実際に持つリスト/アンテナ/チャンネルから候補を出す。
- 対象外: `user("@acct")` のアカウント名補完、`tag("...")`/`search("...")` の自由入力文字列補完、SQL射影(`filter/sql.rs`)側の変更、TQL文法自体の変更。

## アーキテクチャ概要

1. **Rust側 補完エンジン** (`src-tauri/src/filter/complete.rs` 新設): カーソル位置までのテキストを文脈分類し、キーワード/ソース名/フィールド名/演算子の候補を返す。新規コマンド `tql_complete` として公開。
2. **フロント ID補完**: `list(`/`antenna(`/`channel(` の引数文字列内は、Rustを呼ばずフロントが既に保持しているアカウントのリスト/アンテナ/チャンネル一覧から直接候補を出す。
3. **フロント UIコンポーネント** `TqlAutocomplete.svelte`: textarea/input 両対応の汎用ドロップダウン。caret位置に表示し、キーボード/クリックで確定する。

## 1. Rust側 補完エンジン

### コマンドI/F

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TqlEditMode {
    /// from ... where ... のフルクエリ（エキスパートモードのtextarea）
    Query,
    /// where 述語のみ（簡単モードのfilter input）
    Predicate,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TqlCompletionKind { Keyword, Source, Field, Operator }

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TqlCompletionItem {
    /// ドロップダウンに表示する文字列（例: "has_files"）
    pub label: String,
    /// 確定時に [word_start, cursor) を置き換える文字列（末尾スペースや `("` を含む。例: "has_files "）
    pub insert: String,
    pub kind: TqlCompletionKind,
}

/// text[..cursor] を文脈分類し、前方一致する候補を返す。cursorはUTF-8バイトオフセット。
/// tokenize失敗時は空配列を返す（エラーにしない＝入力途中の不完全な文字列でも落とさない）。
pub fn complete(text: &str, cursor: usize, mode: TqlEditMode) -> Vec<TqlCompletionItem>;
```

`commands/column.rs` に薄いラッパーを追加（`validate_tql_query` の並び）:

```rust
#[tauri::command]
#[specta::specta]
pub fn tql_complete(text: String, cursor: u32, mode: TqlEditMode) -> Vec<TqlCompletionItem> {
    filter::complete::complete(&text, cursor as usize, mode)
}
```

`specta_builder()` に `commands::column::tql_complete` を登録。`TqlEditMode`/`TqlCompletionKind`/`TqlCompletionItem` は `filter::complete` から `domain` 相当としてTS export対象にする（`specta::Type` 付与）。

### 文脈分類ロジック

1. カーソル直前を後方スキャンし、識別子文字（英数字/`_`）が続く限り遡って「入力中の単語」を切り出す（`word_start`, `partial`）。空でもよい（直前が空白や記号ならpartialは空文字列）。
2. `text[..word_start]` を `token::tokenize` でトークン化。失敗したら空配列を返して終了。
3. 末尾側から文脈を判定する（`mode`で分岐）:
   - **`Query`モードかつトークン列に `Ident("where")` が未出現**:
     - トークン列が空、または末尾が `Comma` → **ソース名文脈**
     - 末尾が「引数なしソース名の `Ident`」（`home`/`local`/`hybrid`/`global`/`mentions`/`cache`のいずれか）、または直前の `(...)` を閉じた `RParen` → **ソース名文脈 + `where` キーワードも候補に追加**
     - それ以外（ソース名の直後で `(` 待ち、または引数の文字列リテラル内など）→ 候補なし
   - **`where` 出現後（`Query`）、または `Predicate`モード全体**:
     - トークン列が空、または末尾が `AndAnd`/`OrOr`/`Not`/`LParen` → **フィールド名文脈**
     - 末尾が既知フィールド名の `Ident`（`ast::Field::from_name` が `Some` を返す）→ **演算子文脈**（比較演算子ワード群＋`&&`/`||`）
     - 末尾が `Str`/`Num`/`RBracket`/`RParen`（＝値が完結した直後）→ **論理演算子文脈**（`&&`/`||`のみ）
     - それ以外 → 候補なし
4. 各文脈の候補プールから `partial` で前方一致（大文字小文字を区別しない）フィルタし、`TqlCompletionItem` に変換して返す。
   - ソース名文脈: `home `, `local `, `hybrid `, `global `, `mentions `, `cache `（末尾スペース）／ `list("`, `antenna("`, `channel("`, `user("`, `tag("`, `search("`（引用符まで挿入、フロント側でカーソルを引用符内に移動）
   - `where` キーワード: `where `
   - フィールド名文脈: `docs/design/filter-dsl-design.md` §10 の canonical表記全件（例: `has_files `, `reactions `, `user.followers ` ...）。エイリアス（`has_media`等）は候補に出さずcanonical名のみ。
   - 演算子文脈: `contains `, `in `, `startswith `, `endswith `, `match `, `== `, `!= `, `< `, `> `, `<= `, `>= `, `-> `, `<- `, `&& `, `|| `
   - 論理演算子文脈: `&& `, `|| `

### テスト

`filter/complete.rs` にユニットテストを追加（各文脈の代表ケース）:
- `from ` の直後 → ソース名一覧
- `from home,` の直後 → ソース名一覧
- `from home ` の直後 → ソース名一覧 + `where`
- `from home where ` の直後 → フィールド名一覧
- `from home where has_fi` → `has_files` のみ（前方一致）
- `has_files &&` の直後 → フィールド名一覧（`Predicate`モード）
- `reactions ` の直後（フィールド名の後）→ 演算子一覧
- `reactions >= 10 ` の直後（値の後）→ `&&`/`||`のみ
- 壊れた入力（`list("` で終わる等、tokenize失敗）→ 空配列

## 2. `list`/`antenna`/`channel` 引数のID補完（フロントのみ）

`AddColumnModal` は選択中アカウントの `lists: UserList[]` / `antennas: SourceItem[]` / `channels: SourceItem[]`（各 `id`+`name`）を既にロード済み（既存の `list_user_lists`/`list_antennas`/`list_channels` コマンド経由、ガイドモードのドロップダウンで使用中のもの）。

カーソル直前のテキストが正規表現 `/(list|antenna|channel)\(\s*"([^"]*)$/` にマッチしたら（＝閉じられていない文字列リテラルの中にいる＝Rustの`tokenize`には渡さない）、マッチした関数名に応じて対応する配列から `name`（無ければ`id`）が候補2番目のグループに前方一致する項目を抽出し、表示は `name || id`、確定時の挿入テキストは `"{id}")` （残りの `")` まで補完してカーソルをそこに置く）とする。この文脈ではRustの`tql_complete`は呼ばない。

`Predicate`モードのfilter inputは`from`節を持たないため、この特殊ケースは`Query`モード（エキスパートtextarea）でのみ発生する。

## 3. フロント UIコンポーネント

### `frontend/src/input/TqlAutocomplete.svelte`（新設）

`<textarea>` と `<input>` の両方にアタッチできる汎用コンポーネント。使用イメージ（`AddColumnModal.svelte`側）:

```svelte
<div class="tql-field">
  <textarea class="tql-input" bind:value={tqlText} bind:this={tqlEl}
    oninput={onTqlInput} onkeydown={(e) => tqlAutocomplete?.handleKeydown(e)} .../>
  <TqlAutocomplete bind:this={tqlAutocomplete} target={tqlEl} mode="query"
    bind:value={tqlText} {lists} {antennas} {channels} />
</div>
```

- **caret座標算出**: textarea/inputと同じフォント・padding・widthのミラー`<div>`（`visibility:hidden; position:absolute; white-space:pre-wrap`）を裏で保持し、カーソル位置までのテキストを流し込んで`<span>`マーカーの`getBoundingClientRect()`でpx座標を得る（ブラウザ標準の手法、追加ライブラリ不要）。取得した座標を要素の`getBoundingClientRect()`基準に変換し`position: fixed`で配置。ウィンドウ端は`ComposeBar.svelte`の絵文字ピッカーと同じ方式でクランプする。
- **候補取得**: `oninput`/`click`/`keyup`（矢印キーでのカーソル移動）で現在のcursor位置を再計算し、
  1. `mode==="query"` かつ ID補完の正規表現にマッチ → ローカル配列から候補生成
  2. それ以外 → `app.tqlComplete(text, cursor, mode)` を呼ぶ（新規ラッパー、`lib/store.svelte.ts`に`validateTqlQuery`と同様の形で追加）
  候補が0件ならポップアップを閉じる。
- **キーボード操作**: `ArrowDown`/`ArrowUp`で選択項目を循環、`Tab`/`Enter`で確定、`Escape`で閉じる（いずれも textarea/input のデフォルト動作を`preventDefault`で止める。ポップアップが閉じているときは何もしない＝通常の改行/フォーム送信を妨げない）。
- **確定処理**: `value`の`[word_start, cursor)`を選択項目の`insert`で置換し、`value`をイベントで親に反映（`bind:value`）。カーソル位置は挿入後のテキスト末尾（ID補完の`"` の位置など、挿入テキストにマーカーがある場合はその位置）に移動。
- **外側クリック/blur**: ポップアップを閉じる。

### 呼び出し側の変更

- `AddColumnModal.svelte`: エキスパートtextarea・簡単モードfilter inputそれぞれに `TqlAutocomplete` を追加。既存の `onTqlInput`/`onFilterInput`（バリデーション）はそのまま維持し、補完とは独立して動く。

## エラーハンドリング

- `tql_complete` はエラーを返さない（`Vec::new()`で握りつぶす）。バリデーションエラー表示（`tqlErr`/`filterErr`）とは独立した機能なので、補完が効かなくても入力自体は妨げない。
- IPC呼び出し（`app.tqlComplete`）が失敗した場合もフロント側でcatchしポップアップを単に開かない。

## テスト計画

- Rust: `filter/complete.rs` のユニットテスト（上記ケース）。`cd src-tauri && cargo test`。
- フロント: `pnpm check`（型チェック）。`cargo tauri dev` でAddColumnModalを開き、エキスパート/簡単両モードで各文脈の候補が出ること、`list("`入力時に実際のリスト名が候補に出ること、キーボード操作（矢印/Tab/Enter/Escape）が機能することを手動確認。

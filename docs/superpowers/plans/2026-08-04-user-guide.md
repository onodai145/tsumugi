# ユーザーガイド追加 + docs/ 再編成 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `docs/` を設計書(`design/`)とガイド(`guide/`)に再編成し、エンドユーザー向けの日本語ユーザーガイド `docs/guide/user-guide.md` を追加する(Issue #16)。

**Architecture:** ドキュメントのみの変更。既存の設計書4本を `git mv` で `docs/design/` に移動し、それらのファイルを参照している `CLAUDE.md` とRustソースのドキュメントコメントのパス文字列を更新する。次に `docs/guide/user-guide.md` を新規作成し、README にもリンクを追加する。`docs/superpowers/` はbrainstormingスキルの固定パス規約なので触らない。

**Tech Stack:** Markdown, git, ripgrep/grep, cargo test(バインディング再生成確認用)

## Global Constraints

- 言語は日本語のみ(設計書 `docs/superpowers/specs/2026-08-04-user-guide-design.md` の決定事項)
- 実装済み機能のみ記載し、未実装機能(TQLの`mentions`ソース等)は書かない
- スクリーンショットは今回は入れずテキストのみ
- `frontend/src/bindings/tauri.gen.ts` は生成物なので直接編集しない(Rustドキュメントコメントを直せば `cargo test` の `generates_frontend_bindings` で自動追随する)
- コミットメッセージは件名のみ(本文・箇条書きなし)

---

### Task 1: docs/ 再編成(設計書を docs/design/ へ移動 + 参照更新)

**Files:**
- Move: `docs/misskey-multicolumn-client-design.md` → `docs/design/misskey-multicolumn-client-design.md`
- Move: `docs/filter-dsl-design.md` → `docs/design/filter-dsl-design.md`
- Move: `docs/misskey-client-prompts.md` → `docs/design/misskey-client-prompts.md`
- Move: `docs/phase0-scaffold.md` → `docs/design/phase0-scaffold.md`
- Modify: `CLAUDE.md:7`, `CLAUDE.md:44`, `CLAUDE.md:58`, `CLAUDE.md:61`
- Modify: `README.md:5`
- Modify: `src-tauri/src/filter/mod.rs:1`
- Modify: `src-tauri/src/filter/token.rs:1`
- Modify: `src-tauri/src/filter/ast.rs:1`
- Modify: `src-tauri/src/filter/parser.rs:1`
- Modify: `src-tauri/src/filter/eval.rs:1`
- Modify: `src-tauri/src/filter/sql.rs:1`
- Modify: `src-tauri/src/domain/mod.rs:2`
- Modify: `src-tauri/src/domain/note.rs:6`
- Modify: `src-tauri/src/domain/user.rs:5`
- Modify: `src-tauri/src/domain/reaction.rs:8`
- Modify: `src-tauri/src/api/mod.rs:4`

**Interfaces:** なし(ドキュメント・コメントのみの変更。コード動作に影響しない)。

- [ ] **Step 1: 設計書4本を docs/design/ へ移動**

```bash
mkdir -p docs/design
git mv docs/misskey-multicolumn-client-design.md docs/design/misskey-multicolumn-client-design.md
git mv docs/filter-dsl-design.md docs/design/filter-dsl-design.md
git mv docs/misskey-client-prompts.md docs/design/misskey-client-prompts.md
git mv docs/phase0-scaffold.md docs/design/phase0-scaffold.md
```

- [ ] **Step 2: CLAUDE.md のパス参照を更新**

`CLAUDE.md:7` の以下の行:

```
Design docs live in `docs/` — `docs/misskey-multicolumn-client-design.md` is the authoritative design doc; if any other doc conflicts with it, the design doc wins.
```

を次のように変更する:

```
Design docs live in `docs/design/` — `docs/design/misskey-multicolumn-client-design.md` is the authoritative design doc; if any other doc conflicts with it, the design doc wins. User-facing documentation lives in `docs/guide/user-guide.md`.
```

`CLAUDE.md:44` の `docs/filter-dsl-design.md` を `docs/design/filter-dsl-design.md` に変更(2箇所出現するうち両方):

```
... — see `docs/filter-dsl-design.md` for the grammar. ...
```
↓
```
... — see `docs/design/filter-dsl-design.md` for the grammar. ...
```

`CLAUDE.md:58` の `docs/misskey-multicolumn-client-design.md` を `docs/design/misskey-multicolumn-client-design.md` に変更:

```
... This was tried and rejected during Phase 1 — see `docs/misskey-multicolumn-client-design.md` §6.1. ...
```
↓
```
... This was tried and rejected during Phase 1 — see `docs/design/misskey-multicolumn-client-design.md` §6.1. ...
```

`CLAUDE.md:61` の `docs/phase0-scaffold.md` を `docs/design/phase0-scaffold.md` に変更:

```
... — see `docs/phase0-scaffold.md` for context.
```
↓
```
... — see `docs/design/phase0-scaffold.md` for context.
```

- [ ] **Step 3: README.md のリンクを更新**

`README.md:5` の以下の行:

```
設計は [`docs/`](docs/) を参照（設計書 / TQL フィルタDSL / ロードマップ）。
```

を次のように変更する(設計書へのリンクとユーザーガイドへのリンクを分離):

```
設計は [`docs/design/`](docs/design/) を参照（設計書 / TQL フィルタDSL / ロードマップ）。
使い方は [`docs/guide/user-guide.md`](docs/guide/user-guide.md) を参照。
```

- [ ] **Step 4: Rustソースのドキュメントコメントを更新**

以下、各ファイルの該当行を編集する(いずれも `docs/` を `docs/design/` に置き換えるのみ)。

`src-tauri/src/filter/mod.rs:1`:
```
//! TQL(Tsumugi Query Language) フィルタ評価。docs/filter-dsl-design.md。
```
↓
```
//! TQL(Tsumugi Query Language) フィルタ評価。docs/design/filter-dsl-design.md。
```

`src-tauri/src/filter/token.rs:1`:
```
//! TQL の字句解析。docs/filter-dsl-design.md §1(EBNF)・§3(演算子)。
```
↓
```
//! TQL の字句解析。docs/design/filter-dsl-design.md §1(EBNF)・§3(演算子)。
```

`src-tauri/src/filter/ast.rs:1`:
```
//! TQL(Tsumugi Query Language) の AST。docs/filter-dsl-design.md §8。
```
↓
```
//! TQL(Tsumugi Query Language) の AST。docs/design/filter-dsl-design.md §8。
```

`src-tauri/src/filter/parser.rs:1`:
```
//! TQL の構文解析（再帰下降）＋型検査。docs/filter-dsl-design.md §1・§8。
```
↓
```
//! TQL の構文解析（再帰下降）＋型検査。docs/design/filter-dsl-design.md §1・§8。
```

`src-tauri/src/filter/eval.rs:1`:
```
//! TQL のインメモリ評価。docs/filter-dsl-design.md §10。
```
↓
```
//! TQL のインメモリ評価。docs/design/filter-dsl-design.md §10。
```

`src-tauri/src/filter/sql.rs:1`:
```
//! TQL の SQL 射影。docs/filter-dsl-design.md §10・§11。
```
↓
```
//! TQL の SQL 射影。docs/design/filter-dsl-design.md §10・§11。
```

`src-tauri/src/domain/mod.rs:2`:
```
//! 定義は docs/phase0-scaffold.md §2 / docs/filter-dsl-design.md §7 に対応。
```
↓
```
//! 定義は docs/design/phase0-scaffold.md §2 / docs/design/filter-dsl-design.md §7 に対応。
```

`src-tauri/src/domain/note.rs:6`:
```
/// docs/filter-dsl-design.md §7 / 設計書§5.1。フィルタ評価の対象そのもの。
```
↓
```
/// docs/design/filter-dsl-design.md §7 / 設計書§5.1。フィルタ評価の対象そのもの。
```

`src-tauri/src/domain/user.rs:5`:
```
/// docs/filter-dsl-design.md §7。`host` が None ならローカルユーザ。
```
↓
```
/// docs/design/filter-dsl-design.md §7。`host` が None ならローカルユーザ。
```

`src-tauri/src/domain/reaction.rs:8`:
```
/// Misskey は返さない（docs/filter-dsl-design.md §3.4）。
```
↓
```
/// Misskey は返さない（docs/design/filter-dsl-design.md §3.4）。
```

`src-tauri/src/api/mod.rs:4`:
```
//! （`openapiv3` crate が 3.1 を parse 不可。docs/phase0-scaffold §「未確定」と検証結果参照）。
```
↓
```
//! （`openapiv3` crate が 3.1 を parse 不可。docs/design/phase0-scaffold §「未確定」と検証結果参照）。
```

- [ ] **Step 5: 旧パス参照が残っていないか確認**

Run:
```bash
grep -rn "docs/misskey-multicolumn-client-design\.md\|docs/filter-dsl-design\.md\|docs/misskey-client-prompts\.md\|docs/phase0-scaffold\.md\|docs/phase0-scaffold[^/]\|docs/filter-dsl-design[^/]" \
  --include="*.md" --include="*.rs" --include="*.ts" --include="*.svelte" . \
  | grep -v "^\./docs/superpowers/" | grep -v "^\./docs/design/"
```
Expected: 出力なし(旧パス`docs/xxx.md`形式での参照が残っていないこと。`docs/design/`配下のファイル自身の自己参照と`docs/superpowers/`は対象外)。

- [ ] **Step 6: cargo test でバインディング生成が壊れていないことを確認**

Run:
```bash
cd src-tauri && cargo test generates_frontend_bindings
```
Expected: PASS(ドキュメントコメントの変更のみなので型定義自体は変わらず、バインディング生成テストは通る)。

- [ ] **Step 7: git status で移動と変更内容を確認してコミット**

```bash
git status
git add -A
git commit -m "docs: 設計書をdocs/design/へ再編成"
```

---

### Task 2: docs/guide/user-guide.md の作成

**Files:**
- Create: `docs/guide/user-guide.md`
- Modify: `README.md`(Task 1で追加したリンクが正しく機能する前提。まだ`docs/guide/user-guide.md`が存在しないため、Task 1の時点ではリンク切れになるが、本Taskで解消される)

**Interfaces:** なし(静的ドキュメント)。

- [ ] **Step 1: docs/guide/user-guide.md を作成**

```bash
mkdir -p docs/guide
```

以下の内容で `docs/guide/user-guide.md` を作成する:

````markdown
# tsumugi ユーザーガイド

## tsumugiとは

tsumugiは、Misskey向けのマルチカラムデスクトップクライアントです。Krile風のUXで、複数のタイムライン・リスト・アンテナ・通知などを横に並べて同時に見られます。1つの「カラム」の中に複数の「タブ」を持たせることもでき、タブ単位でTQL（後述）によるフィルタも設定できます。

## インストール・起動

開発者向けのビルド手順・必要環境は [README](../../README.md) を参照してください。`cargo tauri build` でスタンドアロンアプリを生成できます。

> **Linux/Wayland環境の注意:** WebKitGTKの描画で問題が出る場合は README のトラブルシューティングを参照してください。

## 基本操作

### アカウントの追加

設定画面の「アカウント」セクションから「＋ アカウントを追加」でMisskeyアカウントを追加します。複数アカウントを同時にログインでき、投稿バーやカラムごとにどのアカウントを使うか選べます。

### カラムとタブ

- **カラム**: 画面に横に並ぶ視覚的な列です。幅を持ち、並び替えられます。
- **タブ**: 1つのカラムの中に複数持てる、実際のタイムライン(受信ソース＋フィルタ)です。カラムの上部にタブバーとして並びます。

### カラムの追加とソース種別

タブバー右端の「＋」ボタンからカラム（またはタブ）を追加します。追加モーダルには「簡単」モードと「エキスパート(TQL)」モードがあります。

「簡単」モードで選べるソース種別:

| 種別 | 内容 |
|---|---|
| Home | ホームタイムライン（フォロー中のユーザーの投稿） |
| Local | ローカルタイムライン |
| Hybrid | ソーシャル（ホーム＋ローカル）タイムライン |
| Global | グローバルタイムライン |
| List | 指定したリストのタイムライン |
| Antenna | 指定したアンテナのノート |
| Channel | フォロー中チャンネルのタイムライン |
| User | 指定ユーザー（`@user@host`）の投稿一覧。ライブ更新なし |
| Tag | 指定ハッシュタグの検索結果。ライブ更新なし |
| Search | キーワード検索結果。ライブ更新なし |
| Notifications | 通知 |

List/Antenna/Channelは、Misskey側に作成済みのものが一覧に表示され、そこから選びます（無い場合はその旨のメッセージが表示されます）。

各カラム/タブには任意で名前を付けられます（空欄なら種別から自動生成されます）。

### タブのフィルタ（TQL）

通知タブ以外では、TQL（Tsumugi Query Language）によるフィルタを設定できます。空欄なら全件表示です。例:

- `has_files` — 添付ファイルがあるノートのみ
- `!bot && local` — bot以外のローカルユーザーの投稿のみ
- `reactions >= 10` — リアクション合計10以上
- `text -> "rust"` — 本文に「rust」を含む

「エキスパート(TQL)」モードでは `from home, list("id") where has_files && !cw` のように、複数ソースを合成した完全なクエリを直接書けます。使えるソース: `home` / `local` / `hybrid` / `global` / `list("id")` / `antenna("id")` / `channel("id")` / `user("@acct")` / `tag("name")` / `search("q")` / `cache`（ローカルキャッシュ検索）。list/antenna/channelは生のIDを指定する必要があります。

TQLの文法の詳細は設計書 [`docs/design/filter-dsl-design.md`](../design/filter-dsl-design.md) を参照してください。

### このタブの通知

search/user/tag以外のソースでは、タブごとに「デスクトップ通知」「通知音」のON/OFFを設定できます。通知音はプリセットの他、任意の音声ファイルを選んで試聴もできます（未指定なら設定画面のグローバル選択を継承します）。

## タブ・カラムの並び替え/幅調整

- **カラムの並び替え**: タブバー左端のグリップアイコンをドラッグ&ドロップで並び替えます。ダブルクリックでカラム設定（幅・自動調整など）が開きます。
- **タブの並び替え・移動**: 各タブ自体もドラッグでき、同じカラム内はもちろん別カラムへも移動できます。
- **タブの編集**: タブ名をダブルクリックすると、そのタブの設定（種別・フィルタ・通知）を編集できます。
- **タブを閉じる**: アクティブなタブの右側に出る×ボタンで閉じます。
- **カラムを縦に分割**: タブバー右端のボタンでカラムを縦方向に分割し、新しいペインを作れます。
- **カラム幅の調整**: カラム右端の境界をドラッグして220〜720pxの範囲で調整します。「自動」設定のカラムはウィンドウ幅に応じて自動調整されるため、この操作はできません。
- 各タブ名の左のドットの色が接続状態を表します（接続済み/接続中・再接続中/エラー）。

## 投稿

投稿バーは画面下部にあります。

- **本文**: `Ctrl+Enter`（macOSは`⌘+Enter`）で投稿できます。`:絵文字名:`・`@メンション`・`#タグ`・MFM構文の入力補完に対応し、クリップボードから画像を直接貼り付けることもできます。
- **公開範囲**: 「公開」「ホーム」「フォロワー」「ダイレクト」から選べます。
- **CW（内容警告）**: 「CW」ボタンで内容警告欄をON/OFFできます。
- **投票**: 「投票」ボタンでON/OFFできます。選択肢は最大10個まで追加でき、複数選択可否や、無期限/日時指定/期間指定の締切を設定できます。
- **添付ファイル**: 画像アイコンから「ローカルから選択」（OSのファイルピッカー）または「ドライブから選択」（Misskeyドライブ）で添付できます。
- **連合なし**: チェックを入れると他インスタンスへ配送されないローカル限定投稿になります。
- **返信・引用**: ノートの返信/引用ボタンから開始すると、投稿バー上部にその対象が表示され、×でキャンセルできます。

## リアクション・Renote・引用・返信、通知アクション

各ノートの下部に以下のアクションボタンが並びます（自分のアカウントのタブでのみ表示）。

- **返信**: 投稿バーに返信モードで反映されます。
- **Renote**: クリックで即座にRenoteします（連打防止のため3秒間のクールダウンがあります）。ホバーで誰がRenoteしたか確認できます。フォロワー限定投稿は自分の投稿のときのみ、ダイレクト投稿はRenoteボタン自体が表示されません。
- **引用**: 投稿バーに引用モードで反映されます。
- **リアクション**: クリックでリアクションピッカーを開閉します。既存のリアクションチップをクリックするとそのリアクションをトグル（自分が付けていれば解除）できます。ホバーで誰がリアクションしたか確認できます。
- **その他メニュー**（「…」）: 以下を含みます。
  - お気に入り登録/解除
  - クリップに追加（既存クリップへの追加、または新規クリップを作成して追加）
- **投票**: 投票付きノートでは選択肢をクリックし、確認ダイアログで「投票する」を選ぶと投票できます。締切済み・投票済みの選択肢は無効化されます。
- **CW付きノート**: 「続きを見る」で本文を展開でき、展開後は「隠す」で再度隠せます。

通知カード自体は表示専用ですが、mention/reply/renote/quote/reaction/pollEndedなど、ノートが紐づく通知にはそのノートがプレビュー表示され、上記のノート操作（返信・Renote・引用・リアクション・その他メニュー）をそのまま通知カードから行えます。

## 設定画面

設定画面の各セクションで以下を変更できます。

- **アカウント**: ログイン中アカウントの一覧、既定アカウントの切替、再認証、アカウントの削除、新規追加。
- **表示**: UIモード（OSに合わせる/PC版/モバイル版）、テーマ（ライト/ダーク/OSに合わせる、プリセット、カスタムテーマの作成・編集）、コードハイライトテーマ、新規カラムの既定幅、起動時のギャップ埋め件数、メディアサムネイルの高さ上限、絵文字スタイル（Twemoji/Fluent Emoji/OS標準）、フォント、背景画像（配置・基準点・暗さ・ぼかし・カラム不透明度）。
- **通知**: デスクトップ通知・通知音のグローバルON/OFFと通知音の選択。実際に鳴るのは、このグローバル設定とタブ側の個別設定が両方ONのときです。
- **リアクション**: 絵文字ピッカーの「ピン留め」タブに表示する絵文字の追加・削除・並べ替え。
- **NG／ミュート**: NGワード（本文/CWへの部分一致）、NGユーザー、NGインスタンスをそれぞれ複数登録できます。保存後は以降受信するノート・表示中のノートの両方に適用されます。
- **データ**: ノートキャッシュの保持件数・保持日数・サイズの上限（いずれも0で無制限。超過分は古い順に自動削除）、動作ログをファイルに残す設定（デバッグ用、次回起動から反映）。
- **このアプリについて**: バージョン・コミットハッシュ・ライセンス・リポジトリURLの表示。新しいバージョンが公開されている場合はバナーで通知されます。

## キーボードショートカット

設定画面の「キー操作」セクションで、以下のデフォルトキーマップを変更・個別リセット・一括リセットできます。

| キー | 操作 |
|---|---|
| `j` | 次のノートを選択 |
| `k` | 前のノートを選択 |
| `r` | 選択ノートに返信 |
| `t` | 選択ノートをRenote |
| `q` | 選択ノートを引用 |
| `e` | 選択ノートにリアクション |
| `o` | 選択ノートをブラウザで開く |
| `h` | 左のカラムへフォーカス |
| `l` | 右のカラムへフォーカス |
| `n` | 新規投稿 |

以下は固定（変更不可）です。

| キー | 操作 |
|---|---|
| `Ctrl`/`⌘` + `Enter` | 投稿する |
| `Esc` | モーダル／リアクションピッカーを閉じる |

## トラブルシューティング

**Linux/Wayland（Hyprland等）で描画が壊れる／`Gdk Error 71 (protocol error)` が出る**

WebKitGTKのDMABUFレンダラがwlroots系コンポジタと衝突することがあります。tsumugiはLinuxでは既定で`WEBKIT_DISABLE_DMABUF_RENDERER=1`をセットして回避していますが、それでも解決しない場合はX11フォールバックを試してください。

```sh
GDK_BACKEND=x11 cargo tauri dev
```
````

- [ ] **Step 2: リンク切れがないか確認**

Run:
```bash
test -f docs/design/filter-dsl-design.md && echo "OK: filter-dsl-design.md exists" || echo "NG: missing"
test -f README.md && echo "OK: README.md exists" || echo "NG: missing"
```
Expected: 両方 `OK` と出力される(`docs/guide/user-guide.md` からの相対リンク `../design/filter-dsl-design.md` と `../../README.md` が実ファイルを指していることの確認)。

- [ ] **Step 3: git status で内容を確認してコミット**

```bash
git status
git add docs/guide/user-guide.md
git commit -m "docs: Issue #16 ユーザーガイドを追加"
```

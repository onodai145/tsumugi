# Misskeyクライアント開発 プロンプト・ロードマップ（Rustハイブリッド版）

> **注記**: 本書と`misskey-multicolumn-client-design.md`（設計書）が矛盾する場合は**設計書を正**とする。
> 本書は設計書のレビューを経て以下の点を修正済み：フロントエンドはSvelteに確定、
> REST APIバインディングはOpenAPI(`progenitor`)からの生成を第一候補とし、ノートはSQLite永続キャッシュを採用する
> （詳細は設計書§3・§5・§6.1・§12を参照）。

Krile (StarryEyes) の設計DNAを参考に、**Tauri v2 のRustバックエンドが「Model層」を所有し、
フロント(TS)は描画・入力に徹する**ハイブリッド構成でMisskeyデスクトップクライアントを
段階的に構築するためのプロンプト集。

## アーキテクチャ方針（ハイブリッド）
- **Rust側 = KrileのModel層**：認証/トークン(secure store)、REST APIクライアント、
  Streaming(WebSocket)受信、Inbox(正規化・重複排除)、SQLiteキャッシュ、フィルタ評価器。
  正規化済みNoteをTauriイベントでフロントへ push。
- **フロント側(TS) = ViewModel + View**：カラムUI、MFM描画(mfm-js)、投稿フォーム、
  リアクションピッカー、設定画面。
- **型**：`tauri-specta` / `ts-rs` で **RustからTS型を自動生成**（二重管理を避ける・必須）。

## Krileプロジェクト → 本構成の対応
| Krile | 本構成 |
|---|---|
| Anomaly/Cadena（API層） | Rust APIクライアント（薄い手書きラッパ） |
| Casket（SQLiteキャッシュ） | Rust + SQLite |
| Receiving（受信パイプライン） | Rust Streaming受信 + Inbox + Broadcaster |
| Filters/KQL（フィルタ評価） | Rust パーサ + 評価器 |
| Models（業務ロジック） | Rust |
| ViewModels/Views | フロント(TS) |
| Nightmare(Win32)/Anomaly OAuth | 破棄 |

## 技術スタック
- Tauri v2 / Rust（tokio, reqwest, tokio-tungstenite, rusqlite or sqlx, serde）
- フロント：TypeScript + **Svelte**（確定。設計書§3.1参照）
- `mfm-js`（MFM描画・フロント）
- `tauri-specta`（Rust→TS型・コマンド生成）

## 落とし穴（実装前に共有すべき前提）
1. **StreamingはOpenAPI仕様外** → Rustで `tokio-tungstenite` を使い、channel購読プロトコルを手書きする。
2. **MisskeyのOpenAPI生成は`progenitor`をまず試す** → 設計書§6.1の通り、Dropshot以外のOpenAPI出力に対する非対応パターンが出る可能性があるため、まず`progenitor`で全体コード生成を試し、失敗したエンドポイントのみ薄い型付きラッパを手書きで補う（全面手書きを既定にはしない）。
3. **Rust↔JS境界で毎回Noteをシリアライズ** → `tauri-specta`で型自動生成し二重管理を避ける。
4. **MFMの描画は必ずフロント**（DOM）。パースはmfm-js側で。
5. **ノートはSQLiteに永続キャッシュする**（設計書§5.1・§5.3参照。当初のメモリのみ案から変更済み）。NQL（Phase 4）の`cache`ソースと過去ログ検索の前提になる。

## Twitter → Misskey 主要差分
| 観点 | Twitter (Krile) | Misskey |
|---|---|---|
| 認証 | OAuth 1.0a | MiAuth / アクセストークン(`i`) |
| REST | GET中心 | 全部POST・JSONボディ・`i`同梱 |
| リアルタイム | User Streams (HTTP chunked) | WebSocket `/streaming` + channel購読 |
| 投稿単位 | Tweet | Note（MFM・CW・可視性・drive・投票・チャンネル） |
| いいね | Favorite | リアクション（カスタム絵文字） |
| RT | Retweet | Renote / 引用Renote |
| リスト | Lists | List / Antenna / Channel |
| 連合 | なし | インスタンス連合・`@user@host` |

---

## Phase 0 — 設計とスキャフォールド

```
Misskeyデスクトップクライアントを新規に作る。構成は Tauri v2 のハイブリッド：
Rustバックエンドが「Model層」（認証・REST・Streaming・キャッシュ・フィルタ評価）を所有し、
フロント(TypeScript + Svelte)は描画と入力に徹する。
Rust→TS型は tauri-specta で自動生成する。
まずは実装せず、以下だけを行う：
1. リポジトリ構成案：src-tauri(Rust core: api/stream/store/filter/session) と
   frontend(ui/render/input) の分離をディレクトリツリーで提示
2. 主要ドメインモデルを Rust struct で下書き（Account, Note, User, Reaction, DriveFile,
   Column, FilterQuery）。tauri-specta で TS へ出す前提の serde 属性も
3. Rust↔フロントの境界設計：Tauri command 一覧（認証/投稿/リアクション等の要求系）と
   Tauri event 一覧（新着Note/接続状態等の通知系）を表で定義
4. データフロー：Streaming受信 → Inbox(正規化・重複排除) → Broadcaster → Column評価器
   → event発火 → フロント描画、を mermaid シーケンス図で
参考にしたい設計思想は「タブごとに受信元とフィルタ式を持つカラム型UI（KrileのKQL）」。
実装コードはまだ書かないこと。
```

## Phase 1 — 認証とAPIクライアント（Rust）

```
Rust側で認証とREST APIクライアントを実装する。
- MiAuth フローでインスタンスURLとトークンを取得。トークンはRustのsecure storeに保存し、
  フロント/JSには渡さない（複数アカウント対応。KrileのAccountManager相当）
- reqwest で Misskey REST を叩く薄い型付きクライアント。全エンドポイントはPOST・JSONボディに
  `i`(トークン)を同梱する共通処理を1箇所に。まず i/me と、使う予定の主要エンドポイントだけ
- レート/権限/ネットワークエラーを型付きErrorに正規化
- Tauri command として login / logout / list_accounts / switch_account / whoami を公開し、
  tauri-specta で TS 型を生成
テストを付けること。まず`progenitor`でOpenAPI全体からのコード生成を試し、失敗する場合のみ該当エンドポイントを手書きラッパで補う（設計書§6.1）。
```

## Phase 2 — Streaming受信 & ホームTL描画

```
リアルタイム受信の心臓部を作る。
【Rust】
- tokio-tungstenite で /streaming のWebSocketに接続し、'homeTimeline' channel を購読
  （connect/disconnect、切断時の自動再接続とバックオフ）
- 起動時は notes/timeline を1ページ取得。以降Streamingで追記
- Inbox：受信Noteをドメイン型に正規化し、NoteIDで重複排除してから Broadcaster へ
- 新着Noteは Tauri event で正規化済みの形でフロントへ push。接続状態も event で通知
【フロント】
- event を購読してホームTLを表示。上スクロールで過去ページを command 経由で追加取得
- Note描画は mfm-js でMFMをAST化してレンダリング（メンション/ハッシュタグ/カスタム絵文字/
  リンク/CW折り畳み）。Renote・引用・添付画像(drive)・投票の最低限表示
- 仮想スクロールで大量Noteでも重くしない
```

## Phase 3 — 投稿とリアクション

```
投稿・リアクションを実装する。
【Rust command】
- create_note：本文(MFM)・CW・可視性(public/home/followers/direct)・添付(driveアップロード)・
  投票・返信/引用Renote・renote・delete
- create_reaction / delete_reaction（カスタム絵文字対応）
【フロント】
- 投稿フォーム（可視性選択・CW・ファイル添付・投票作成・返信/引用UI）
- リアクションピッカー（カスタム絵文字一覧はRustでキャッシュして提供）
- 楽観的更新：送信直後にUI反映し、command失敗イベントでロールバック
KrileのInputプロキシのように、下書き・複数アカウント同時投稿を見据えた構造にする。
```

## Phase 4 — マルチカラム & フィルタDSL（Krileの核）

```
Krileのタブ/KQL思想を持ち込む。
【Rust】
- Column = 受信ソース + フィルタ式。ソース種別：
  home / local / hybrid / global / list / antenna / channel / user / search / mentions
- 各ソースに対応する Receiver（channel購読 or REST取得）。1本のWS接続を複数カラムで共有し、
  Broadcasterが受信Noteを各カラムの評価器へ配る
- フィルタDSL（NQL）は filter-dsl-design.md に設計確定済み。これを入力に Rust 実装：
  token.rs(字句) → parser.rs(再帰下降+型検査) → ast.rs → eval.rs(インメモリ) → sql.rs(SQLite射影)。
  §1 EBNF・§3 語彙・§7/§8 データ構造・§9/§10 SQL対応表に従う。KQL同様「パース」と「評価」を分離。
【フロント】
- 横並びカラムUI。カラムの追加/削除/並べ替え、ソース選択とフィルタ式入力。
- 評価はRust側、フロントは結果Noteの描画に徹する
```

## Phase 5 — Mute/Block/NG・通知・キーアサイン

```
運用機能を追加する。
【Rust】
- ローカルNGワード/NGユーザ/NGインスタンス＋サーバ側mute/blockの反映
  （KrileのMuteBlockManager相当）。Broadcasterと評価器の一段として差し込む
- 通知：'main' / notification channel を購読し、メンション/リアクション/フォロー/Renote を event化
- 操作ログ・エラーを流すストリーム（KrileのBackstage相当）を event で提供
【フロント】
- 通知タブUI、NG設定UI、ステータス/ログ表示
- キーアサイン（j/k移動、r返信、favでリアクション 等）を設定で変更可能に
```

## Phase 6 — 永続化・設定・仕上げ

```
永続化と設定を整える。
【Rust】
- 受信NoteのSQLiteキャッシュ。表示はDBクエリ＋インメモリ評価の二段構え（Krile Casket踏襲）。
  起動時の即時復元・オフライン閲覧
- 設定の永続化：アカウント/カラム構成/テーマ/キーアサイン。スキーマ移行も
- WS自動再接続の堅牢化、レート/エラー時のバックオフ、カスタム絵文字キャッシュ
【フロント】
- ライト/ダークテーマ、設定画面の仕上げ
```

---

## プロンプト運用のコツ
- Krileの元コードを参照させるときは局所指定（例：`Filters/Parsing/` と `Filters/Sources/` を読ませ
  「このKQL設計を Rust のMisskey Note向けフィルタDSLに翻訳して。C#→Rust、対象はTwitterStatusではなくNote」）。
  46k行あるので丸ごと読ませない。
- 「移植」ではなく「設計を参考に新規実装」と明示する。
- 各フェーズで「実装前にまず設計/型定義/command・event一覧/mermaidを出す→合意→実装」を挟む
  （Phase 0・Phase 4 で特に有効）。
- Rust↔TS境界を変えたら必ず tauri-specta の型再生成をセットで依頼する。
```

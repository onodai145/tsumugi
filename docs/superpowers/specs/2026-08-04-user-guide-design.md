# ユーザーガイド追加 + docs/ 再編成 設計書

Issue: #16「ユーザー向けドキュメントがない」

## 背景

現状 `README.md` は開発者向け(起動方法・構成・テスト・リリース手順)のみで、
エンドユーザー向けの使い方ドキュメントが存在しない。また `docs/` 直下は
設計書(`misskey-multicolumn-client-design.md` 等)とツール生成物
(`docs/superpowers/`)が混在しており、ユーザーガイドをそのまま追加すると
さらに見通しが悪くなる。

## スコープ

1. `docs/` を再編成し、既存の設計書4本を `docs/design/` に移動する。
2. `docs/guide/user-guide.md` を新設し、エンドユーザー向けの使い方ガイドを書く。
3. 移動に伴う参照更新(`CLAUDE.md` / `README.md` / Rustソースのドキュメントコメント)を行う。

対象読者は「エンドユーザー」を主眼としつつ、将来的にコントリビューター向け
情報を追加できる余地を残す(今回は書かない)。言語は日本語のみ。

## ディレクトリ再編成

```
docs/
├── design/
│   ├── misskey-multicolumn-client-design.md   (git mv)
│   ├── filter-dsl-design.md                   (git mv)
│   ├── misskey-client-prompts.md              (git mv)
│   └── phase0-scaffold.md                     (git mv)
├── guide/
│   └── user-guide.md                          (新規)
└── superpowers/                               (移動しない: brainstormingスキルの固定パス規約)
```

`docs/superpowers/` はスキル自体が `docs/superpowers/specs/...` 固定パスに
書き出す規約であるため対象外。

## 参照更新が必要な箇所

`git mv` 後、以下のファイル内のパス文字列を機械的に置換する
(`docs/misskey-multicolumn-client-design.md` → `docs/design/misskey-multicolumn-client-design.md` 等):

- `CLAUDE.md`(3箇所: 冒頭の設計書リンク、progenitor不採用の理由、specta pin の理由)
- `README.md`(`docs/` へのリンク文言。合わせてユーザーガイドへのリンクも追加)
- `src-tauri/src/filter/{mod,token,ast,parser,eval,sql}.rs` のドキュメントコメント
- `src-tauri/src/domain/{mod,note,user,reaction}.rs` のドキュメントコメント
- `src-tauri/src/api/mod.rs` のコメント(phase0-scaffold 参照)

`frontend/src/bindings/tauri.gen.ts` は `cargo test` の
`generates_frontend_bindings` で自動再生成される生成物なので直接編集しない。
上記Rustドキュメントコメントを直せば、次回生成時に自動で追随する。

## user-guide.md の構成

`docs/guide/user-guide.md` に以下の見出しで書く。実装済み機能のみ記載し、
未実装機能(TQLの `mentions` ソース等)は書かない。スクリーンショットは
今回は入れずテキストのみとする。

1. **tsumugiとは** — 概要、Krile風マルチカラムUXの説明
2. **インストール・起動** — README の内容を要約し、詳細はREADMEへリンク(二重管理を避ける)
3. **基本操作** — アカウント追加、カラム/タブの概念(視覚的カラム=ColumnGroup、その中のタブ=Column)、
   カラム追加時に選べる種別一覧(Home/Local/Global/Hybrid/Notifications/List/Search/Antenna/Channel/User/Tag/TQL)
4. **タブ・カラムの並び替え/幅調整**
5. **投稿** — 公開範囲、CW、投票、ドライブ添付(ComposeBar)
6. **リアクション/Renote/引用/返信、通知アクション** — 通知カードからの直接アクション含む
7. **TQLフィルタの基本** — `from ... where ...` の最小例のみ紹介し、
   文法の詳細は `docs/design/filter-dsl-design.md` へリンク
8. **設定画面** — 各セクション(アカウント/表示/通知/リアクション/ミュート/データ/このアプリについて)の役割
9. **キーボードショートカット** — 設定のキーバインド変更機能(`KeysSection.svelte`)の説明
10. **トラブルシューティング** — Linux/Wayland の描画問題(`Gdk Error 71`)など、READMEの注意書きを転記

## テスト

ドキュメント変更のみのため自動テストは対象外。以下を確認する:
- `git mv` 後、旧パスへの参照が残っていないか `grep -rn "docs/misskey-multicolumn-client-design\|docs/filter-dsl-design\|docs/misskey-client-prompts\|docs/phase0-scaffold" .`(除外: `docs/superpowers/`, `.git/`)で確認
- `cargo test` の `generates_frontend_bindings` を実行し、Rustコメント変更後もバインディング生成が壊れていないことを確認

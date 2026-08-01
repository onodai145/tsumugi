# Changelog

All notable changes to this project will be documented in this file.

## [0.8.0] - 2026-08-01

### 🚀 Features

- リリーススクリプトを追加 (#124)
- 汎用Modalコンポーネントを追加
- 投稿エラーをモーダル表示に変更
- UiPrefsにコードハイライト設定を追加
- ShikiハイライタのシングルトンとhighlightCodeを追加
- カスタムシンタックステーマのCSS変数反映を追加
- Storeにコードハイライト設定の読み込み・適用を配線
- CodeBlock.svelteを追加
- BlockCodeノードをCodeBlock.svelteで描画するよう変更
- 設定画面にコードハイライトテーマの選択UIを追加
- Shiki同梱テーマカードに実際の配色スワッチを追加
- ReactionUserドメイン型を追加
- Notes/reactionsレスポンスの正規化を追加
- Notes/reactions・notes/renotesを叩くAPI関数を追加
- リアクション・Renoteユーザー一覧取得コマンドを追加
- リアクション・Renoteユーザー一覧のポップオーバーを追加
- リアクション・Renoteバッジにユーザー一覧ホバーを配線
- MFM関数の引数スキーマ(FN_ARGS)を追加
- MFM補完のトリガー検出(detectTrigger)を実装
- MFM補完の候補マッチング(match*)を実装
- MFM補完の候補組み立て(buildCompletionItems)と置換計算(applyCompletion)を実装
- Textarea内キャレット座標計算(getCaretCoordinates)を追加
- MFM補完候補ポップアップ(CompletionPopover)を追加
- ComposeBarにMFM補完ポップアップを配線
- ユーザー検索/ハッシュタグ検索のRESTラッパーを追加
- Search_users/search_hashtagsコマンドを追加しTSバインディングを再生成
- メンション/ハッシュタグのトリガー検出(detectTrigger)を追加
- CompletionThumbnailにavatar種別を追加しbuildCompletionItemsの引数型をSyncTriggerに限定
- メンション/ハッシュタグ検索のIPC呼び出し(mfmSearch)を追加
- CompletionPopoverでavatarサムネイルをcustomと同じimg描画にする
- ComposeBarにメンション/ハッシュタグの非同期補完を配線

### 🐛 Bug Fixes

- Modal.svelteのEscape無効化とポータル欠如を修正
- コードハイライトのライトモード無色・言語エイリアス・テーマ背景色の抑制を修正
- コードハイライトテーマ選択をネイティブselectからテーマカードUIに変更
- 狭いカラムでのコードブロック横スクロール時の背景・余白崩れを修正
- ReactionUsersPopoverのpromise解決時に古いprops向けの結果を破棄する
- 最終レビュー指摘（Renote重複・無効ボタンhover・マルチアカウントキャッシュキー）を修正
- Renoteホバーの対象をアイコン全体に広げ件数0でも表示できるようにする
- リアクション・Renoteホバー表示のUXを改善する
- リアクションをノートキャッシュへ反映し再起動後も消えないようにする
- フォロワー限定投稿のリアクションをreaction通知経由でも反映する
- MFM補完のmatchEmojisをカスタム+Unicode結合(remaining枠埋め)に修正
- GetCaretCoordinatesのwhiteSpace上書きバグとミラー要素リークを修正
- 最終レビュー指摘(キャレット位置ズレ・Enter誤確定・スクロール追従・矢印キーラップアラウンド)を修正
- GetCaretCoordinatesのcontent幅計算でborder幅を二重減算していたバグを修正
- 絵文字候補が未選択のうちはハイライトを出さないようにする
- Enterによる確定はトリガー種別に関わらず矢印キー選択後のみに統一
- 未選択状態からの最初の矢印キー操作で先頭/末尾を選ぶよう修正
- 最終レビュー指摘(asyncCandidatesの古い候補混入・アンマウント時のタイマー未クリア)を修正
- RNボタンの連打を3秒のクールダウンで抑止 (#140)
- Android のファイル添付でcontent:// URIを読めず失敗する不具合を修正
- Content:// 由来の添付ファイル名に拡張子が無い問題をマジックナンバー判定で補完
- Content:// URIのファイル名解決にTauri coreのPathResolverを使う
- Androidの添付ピッカーをフォトピッカーからSAFドキュメントピッカーに変更
- 添付ファイルの選択フィルタを撤廃してAndroidのフォトピッカー自動リダイレクトを回避
- Unicode絵文字補完の挿入テキストを実際の文字にする

### 💼 Other

- Shikiを依存に追加

### 🚜 Refactor

- ポータル用アクションをlib/portal.tsへ切り出す

### 📚 Documentation

- エラーモーダル化のdesign specを追加
- エラーモーダル化の実装計画を追加
- コードブロックのシンタックスハイライト設計を追加
- コードハイライトの既定値をOS追従(auto)に変更
- コードブロックのシンタックスハイライト実装計画を追加
- 計画書をshiki 4.3.1の実APIに合わせて修正
- リアクション・Renoteユーザー一覧表示のspecを追加
- リアクション・Renoteユーザー一覧表示の実装計画を追加
- フロントエンド単体テスト基盤(Vitest)の設計書を追加
- カバレッジ計測を見送る理由を明記
- フロントエンド単体テスト基盤(Vitest)の実装計画を追加
- Svelteコンポーネント単体テスト(Testing Library)の設計書を追加
- 設計書をMfm.svelte単独スコープ・jest-dom不採用に更新
- Svelteコンポーネント単体テスト(Testing Library)の実装計画を追加
- MFM補完(ComposeBar) Phase1設計書を追加
- MFM補完(ComposeBar) Phase1実装計画を追加
- MFM補完(ComposeBar) Phase2(メンション/ハッシュタグ)設計書を追加
- MFM補完(ComposeBar) Phase2実装計画を追加
- CLAUDE.mdに開発フロー・マージ方針・Androidビルドを追記

### 🎨 Styling

- コードブロックのshikiテーマ切替CSSを追加

### 🧪 Testing

- Vitest基盤を導入しtime.tsのテストを追加
- Nyaize.tsのテストを追加
- BackgroundFitMode.tsのテストを追加
- BackgroundPosition.tsのテストを追加
- EmojiKey.tsのテストを追加
- Keymap.tsのテストを追加
- Mfm.tsのテストを追加
- Testing Libraryを導入しCustomEmoji.svelteのテストを追加
- Sparkle.svelteのテストを追加
- Mfm.svelteのテストを追加

### ⚙️ Miscellaneous Tasks

- Frontend-checkジョブにvitestを追加
## [0.7.0] - 2026-07-26

### 🚀 Features

- バイト列から直接アップロードするコマンドを追加
- Domain::Clip型を追加
- Notes/favorites REST ラッパを追加
- Clips系REST ラッパを追加
- お気に入り/クリップのTauri commandを追加
- お気に入り/クリップ操作をstoreに追加
- NoteMenuコンポーネントを追加
- NoteCardに⋯メニューを追加
- 403検知用のForbiddenError/unwrapAccを追加
- LogEntryに再認証アクションを持たせる
- AccountId付きIPC呼び出しをunwrapAccに統一
- 再認証時にaccount情報を上書きしログ文言を分ける
- AddAccountに再認証モードを追加
- 設定→アカウントに再認証ボタンを追加
- Backstageの403ログに再認証アクションを追加
- ペイン分割ツリー(PaneNode)のドメイン型と挿入/削除ロジックを追加
- Pane_layoutの永続化とdelete_empty_groups連動によるペイン畳み込みを追加
- Split_pane/load_pane_layout/discard_empty_groupコマンドを追加しTSバインディングを再生成
- フロントにpaneRoot状態とsplitPane/discardEmptyGroupを追加
- Pane.svelteによる木構造描画と「下に分割」ボタンを追加
- PaneNode::id/set_sizeを追加(ペイン高さの数値リサイズ用)
- Resize_paneコマンドを追加
- カラム設定に縦分割ペインの高さ(%)入力を追加
- 分割ブロック全体の幅にも自動調整(auto)を設定できるようにする
- 高さもRowの幅と同じ固定%/自動調整モデルにする(下に分割の新規2子はデフォルトauto)
- Tauri-plugin-clipboard-managerとpng crateを追加
- クリップボード画像用のPNGエンコード/ファイル名生成関数を追加
- Read_clipboard_image コマンドを追加
- 投稿欄でクリップボード画像の貼り付けに対応
- Column_noteにcreated_atを非正規化しカバリングインデックスを追加

### 🐛 Bug Fixes

- Clip.notes_countをi64+specta::Numberアノテーションに戻す
- NoteMenuのエラーハンドリングとa11y警告を修正
- リアクションピッカーとノートメニューを排他表示にする
- 最終レビュー指摘(クリップ一覧のエラー処理・お気に入り不整合・サブメニューはみ出し)を修正
- MiAuth権限にwrite:favorites/write:accountを追加
- AddAccountのstate_referenced_locally警告を抑制
- MiAuth再認証時に既存account.idを維持する
- リアクションのピン留め並べ替えをPointer Events化してAndroidに対応
- 並べ替えgripのタップ判定を拡大
- UIのハードコードされた危険/警告色をテーマ変数化してカスタムテーマにも対応
- SplitPane/discardEmptyGroupのgroups/paneRoot更新を両方成功後にまとめて反映
- PaneNode.group_idをTSバインディングでもcamelCase(groupId)に統一
- 旧キー名(group_id)のpane_layoutで起動不能になる問題を修正
- ペインのレイアウト崩れ・畳み込み漏れを修正
- Column分割内のLeafが利用可能幅いっぱいに広がるよう修正
- 新規カラムがペイン木に反映されず非表示になる問題と、分割ブロック全体の幅設定を追加
- カラム追加/分割時に自己修復ロジックと競合して二重挿入されるバグを修正
- クリップボード画像プレビューのblob URLを削除/アップロード後に解放する

### 📚 Documentation

- クリップボード画像貼り付け(#57)の設計を追加
- クリップボード画像のファイル名をミリ秒付き人間可読形式に変更
- クリップ/お気に入り機能の設計を追加(Issue #14, #15)
- クリップ/お気に入り実装計画を追加
- MiAuth再認証UIの設計ドキュメントを追加
- MiAuth再認証UIの実装計画を追加
- カラム縦分割(ペイン化)の設計ドキュメント追加(Issue #31)
- 分割ボタンの初期サイズルールを追記(Issue #31)
- ColumnSettingsでのペインサイズ数値指定(Row/Column両対応)を追記(Issue #31)
- ペインサイズの正規化ルールを整理(挿入/削除は隣接1つとだけやり取り、Columnはflex-growウェイト方式に変更)
- ペイン分割Slice1(下分割のみ)の実装計画を追加(Issue #31)
- ペイン高さの数値リサイズ Slice 2 実装計画を追加(Issue #31)
- クリップボード画像の設計をアップロード投稿時遅延方式に変更
- クリップボード画像貼り付け(#57)の実装計画を追加
- Task2テストのepochミリ秒の誤りを修正(2026-07-27→2026-07-25相当の値に)
- SQLiteチューニング(Issue #114)の設計ドキュメントを追加
- SQLiteチューニング(Issue #114)の実装プランを追加
- 設計docのインデックス作成箇所の記述を実装に合わせて修正

### ⚡ Performance

- Load_cachedをidx_cn_column_created経由のクエリに変更
- Cache.dbにPRAGMAチューニングを適用

### ⚙️ Miscellaneous Tasks

- AUR向けPKGBUILDと自動更新ワークフローを追加 (#120)
## [0.6.1] - 2026-07-20

### 🐛 Bug Fixes

- Cargo tauri android buildに--split-per-abiを追加しアーキテクチャ別APKを実際に生成する
- Universalビルドとper-ABIビルドを分けて両方生成する
## [0.6.0] - 2026-07-20

### 🚀 Features

- 動作ログをファイルに残す設定を追加(Issue #12調査用)
- 通知/通知音の発火をdebugログとしてファイルへ記録
- UiPrefsに背景画像の基準点(background_position)を追加
- 背景画像の基準点マッピングを追加
- 背景画像の基準点をCSS変数へ反映
- 背景画像のbackground-positionをCSS変数化
- 背景画像の基準点を選ぶ9点グリッドUIを追加
- ドライブのファイル/フォルダ一覧取得APIを追加
- ドライブ一覧取得コマンドを公開しTSバインディングを再生成
- ドライブ添付ピッカーのモーダルコンポーネントを追加
- 画像添付ボタンからドライブピッカーを開けるようにする
- Androidリリースにアーキテクチャ別APKを追加
- 添付ファイルのローカルプレビュー用コマンドread_attachment_previewを追加
- 添付ファイルのアップロードを投稿時まで遅延させる

### 🐛 Bug Fixes

- 同じノートが複数表示された際にリアクションピッカーも多重表示されるのを修正 (#70)
- ノートカードで本文以外の要素までコピーされてしまう問題を修正
- 入力欄以外でのCtrl+A全選択を無効化
- CW注釈のフォントサイズが未指定でブラウザ既定16pxになる問題を修正
- 添付メニューのa11y警告(tabindex不足)を修正
- ドライブピッカーのレビュー指摘4件を修正
- MiAuth要求スコープにread:driveを追加
- ドライブピッカーのサムネイルグリッドのレイアウト崩れを修正
- プレースホルダーのmisskey.ioを架空ドメインmisskey.exampleに変更
- Ctrl+Enter投稿ショートカットに二重送信防止のbusyガードを追加

### 📚 Documentation

- 背景画像の基準点設定 設計ドキュメント追加(Issue #76)
- 背景画像の基準点設定 実装計画追加(Issue #76)
- ドライブ添付ピッカーの設計仕様を追加(Issue #13)
- ドライブピッカーのファイル種別フィルタを撤廃
- ドライブ添付ピッカーの実装計画を追加
- Issue #66 添付ファイルアップロードタイミング変更の設計書を追加
- Issue #66 実装計画を追加、設計書のプレビュー方式を実態に合わせて修正
## [0.5.0] - 2026-07-18

### 🚀 Features

- 背景画像の配置方法を選択できるようにする
- 絵文字ピッカーで全Unicode絵文字の閲覧とピン留めのカスタマイズを可能にする
- ピン留めをドラッグ並べ替え・カスタム絵文字カテゴリ表示・複数インスタンス対応にする
- ブラウザ由来の右クリックメニューを隠す
- 開発ビルド限定でF12からDevToolsを開けるようにする
- モバイル版UIとPC版UIを設定画面から切り替えられるようにする

### 🐛 Bug Fixes

- 多重起動を防止する
- ピッカーを縦スクロール単一化しカスタム→絵文字順・カテゴリ表示に、ピン留めのドラッグ並べ替えを修正
- 入力欄では右クリックメニューを残す
- Android版で投稿ボタン/返信ボタン押下時に投稿フォームを開いてフォーカスする
- 通知欄のノートプレビューでリアクション内訳を非表示にする

### 🚜 Refactor

- ピン留め絵文字の編集UIを本家Misskey準拠(設定画面での追加/削除/並べ替え)にする
## [0.4.0] - 2026-07-16

### 🚀 Features

- 通知受信時にBackstageログを残す
- メディアサムネイルの高さ上限を設定可能にする
- 新バージョンリリース時にBackstageへ通知する
- ノートキャッシュの保持件数上限を設定可能にする
- ノートキャッシュの経過日数・サイズ上限にも対応、既定件数を1万に
- Renote/リプライを色分けして見分けやすくする
- Renote/リプライの色をテーマでカスタマイズ可能にする

### 🐛 Bug Fixes

- タブ設定モーダルの通知音試聴ボタン等にテーマを適用する
- 返信作成時に相手のacctを本文へ自動挿入する
- LoadMoreで取得したノートもリアクション更新を購読する
- リアクション後付けのカスタム絵文字URLをemojiプロキシで解決する
- WebSocketの死活監視(ping/timeout)を実装し無通知の再接続漏れを防ぐ
- Backstageの再接続成功ログが実質出ない不具合を修正
- 幅広カラムでメディアサムネイルの高さに上限を設ける
- サムネイル上限を200pxに調整

### 🚜 Refactor

- ノートキャッシュ設定を表示から独立した「データ」セクションへ移動
## [0.3.3] - 2026-07-15

### 🐛 Bug Fixes

- カラム設定ボタン(grip)を正方形のタップ領域に修正
- Gripの文字表示をアイコンに置き換え
- Linux版でIME未確定文字列が表示されない問題を修正
## [0.3.2] - 2026-07-15

### 🐛 Bug Fixes

- AndroidリリースからAABアップロードを除去
- Android背景画像/通知音選択でcontent:// URIが読めない不具合を修正
- Androidのステータスバー/ナビゲーションバーにUIが隠れる不具合を修正
## [0.3.1] - 2026-07-13

### 🐛 Bug Fixes

- MacOS keychainビルドとAndroid NDKシンボリックリンクのCI失敗を修正
## [0.3.0] - 2026-07-13

### 🚀 Features

- Android向けビルドをTauri mobileで有効化
- Android向けkeyring-core移行とモバイル投稿UIを追加

### ⚙️ Miscellaneous Tasks

- GitHub ActionsでAndroidビルド検証とリリース署名を追加
## [0.2.0] - 2026-07-12

### 🚀 Features

- 設定をJSON化しノートキャッシュを別SQLiteに分離

### ⚙️ Miscellaneous Tasks

- Windowsポータブル版をexe単体からDLL同梱zipに変更

### ◀️ Revert

- Windowsポータブル版をexe単体に戻す(MSVCではWebView2Loader.dll不要のため)
## [0.1.0] - 2026-07-11

### 🚀 Features

- Phase 1 認証とREST APIクライアント(Rust Core)
- Phase 2(前半) Streaming受信コア(WS)とホームTLの初期取得
- Phase 2(後半) ホームTL描画UI(Svelte)とMFM描画
- Phase 3 投稿とリアクション
- アカウント/カラム構成の永続化(SQLite)と再起動復元
- リアクション/投票/削除のリアルタイム反映(subNoteキャプチャ)
- Drive画像/動画の添付アップロード
- ノート永続キャッシュ(SQLite)と起動時の即時復元
- Phase 4(1) TQL の AST と字句解析器
- Phase 4(2) TQL パーサ(再帰下降)＋型検査
- Phase 4(3) TQL インメモリ評価器
- Phase 4(4) TQL の SQL射影(cacheソース検索用)
- Phase 4(5) カラム作成UI＋フィルタ適用＋多ソース(4TL)
- Phase 4(6) List/Search ソース対応(追加パラメータ付き)
- 通知カラム(Notifications)の実装
- カラム並べ替え/幅調整 + Krile寄りの高密度フラットデザイン
- Krileレイアウト Stage1 - 上部投稿バーとトップバー化
- Krileレイアウト Stage2 - カラム=タブの集合(グループ/タブモデル)
- Krileレイアウト Stage3 - タブのD&D(並べ替え/グループ間移動)
- Phase 5(1) ローカルNG(ミュート) - NGワード/ユーザ/インスタンス
- Phase 5(2) デスクトップ通知・通知音
- 設定を単一の設定画面に集約（通知 / NG をサイドバー切替）
- 設定画面にアカウント/表示/キー操作を追加
- キーボード操作（j/k 選択・返信/引用/Renote/リアクション/開く・カラム移動）
- Backstage（操作ログ/エラー表示）
- サーバ側ミュート/ブロックの反映＋通知へのミュート適用
- キーアサインのカスタマイズ（設定でキー変更可能に）
- カラムソース追加（Antenna/Channel/User/Tag）
- MFM 装飾・アニメーション本家準拠 + カスタム絵文字表示
- アカウント選択をアイコン付き独自ドロップダウンに
- 公開範囲をアイコン付き独自ドロップダウンに
- カラム追加モーダルの select を独自ドロップダウンに統一
- タブ名の設定とタブ編集（ダブルクリックでカラム設定を再編集）
- WebSocket にも User-Agent を付与（REST と共通化）
- *(ui)* 操作ボタンの絵文字を @lucide/svelte のline-iconに置き換え
- *(ui)* 公開範囲セレクトの絵文字もlucideアイコンに置き換え
- *(ui)* 残っていた絵文字ボタン/記号もlucideアイコンに統一
- *(ui)* 表示設定でフォントを任意に指定できるようにする
- *(ui)* 表示設定で背景画像を設定できるようにする
- *(notify)* タブごとに通知の有無・通知音を設定できるようにする
- *(notify)* 通知音を任意に設定できるようにする(プリセット+カスタム音声ファイル、グローバル/タブ両対応)
- Nyaize対応（isCatユーザの発言をにゃん語化）
- カラムの固定幅/自動調整を混在対応、タブ編集画面で切替可能に
- カラム設定に幅の数値入力を追加
- 既定で使うアカウントを設定できるように
- Unicode絵文字の表示スタイル(Twemoji/Fluent Emoji/OS標準)を選択可能に
- Unicode絵文字画像をアプリに同梱し外部通信なしで解決するように変更
- TQLのfrom節で複数ソースを1カラムに合成(エキスパートモード)
- 簡単→エキスパート切替時に選択中のソース/フィルタをTQLへ反映
- 起動時に閉じていた間のノートをRESTで遡って埋める(ギャップ埋め)
- 添付メディアを画像/動画は見られるように、それ以外はファイル名表示+ダウンロードに
- 画像ライトボックスにホイールでの拡大縮小+ドラッグでの移動を追加
- 画像/動画をアプリ内の保存ボタンでディスクへ保存できるように
- アンケートに投票できるようにする
- 投票前に確認ダイアログを出す
- 投稿モーダルを廃止し上部の投稿バーに統合(返信/引用/CW/投票対応)
- 投票の期限を無期限/日時指定/期間指定から選べるように
- 投稿窓のテキスト欄をフォーカス無し・未入力時は1行分にコンパクト化
- 起動完了時にBackstageログへ記録
- BackstageにDBノート件数/流速/起動経過時間をアイコン付きで表示
- プリセットテーマ+ユーザー作成カスタムテーマ機能を追加(著名15配色)
- 設定画面に「Tsumugiについて」セクションを追加
- ユーザー表示名に含まれるカスタム絵文字をレンダリング

### 🐛 Bug Fixes

- Dev起動時の connection refused を解消(vite を IPv4 固定)
- Linuxで WEBKIT_DISABLE_DMABUF_RENDERER を既定セット(Wayland描画エラー回避)
- MiAuth URLを開けるよう opener にURLスコープを付与
- Dev で vite-plugin-svelte の仮想CSSモジュール読込失敗を解消
- Vite 7 系へダウングレード(vite-plugin-svelte の仮想CSS読込失敗回避)
- EmitCss:false でコンポーネントCSSをJSに注入(仮想CSSモジュール警告を根絶)
- Tauri-plugin-notification を依存に追加（permission not found を解消）
- 上部バーの＋アカウントを廃止（設定に集約）
- MiAuth スコープに read:mutes/read:blocks を追加
- MiAuth スコープに read:channels を追加
- リアクションピッカーが表示されない問題を修正
- ヘッダー左のアプリ名(tsumugi)表示を削除
- OS通知/音を通知IDでグローバルに重複除去
- HMR で古い購読を破棄（dev の通知リスナー蓄積を防止）
- *(stream)* AddChannel の connect 送信失敗でカラムを永久に失わないようにする
- *(ui)* タブ下の「タイトル+接続状態」テキスト行を削除
- *(ui)* モーダル上にスクロールバーが乗ってしまう問題を修正
- *(ui)* 表示設定を保存するたびカスタムキーバインドが消えるバグを修正
- フォロワー限定/ダイレクトノートのRN・引用ボタンを本家準拠で非表示に
- ノートの公開範囲表記をテキストからアイコンに変更
- 固定/自動幅の切替UIをタブ編集モーダルからカラム設定に分離
- 表示設定の保存で既定アカウント等の未編集フィールドが消える不具合を修正
- 複数ソースTQLカラムのギャップ埋めで疎なソースが密なソースを打ち切る問題を解消
- 画像ライトボックスをカーソル位置基準ズームにし、ネイティブ画像ドラッグとの競合を解消
- エキスパートモードの通知ラベルを「タブの通知」に統一
- 投票終了通知の文言から不要な「の」を除去
- 接続状態イベントの取りこぼしでタブの接続表示が固まる問題を修正
- CW切替時に投稿窓の高さが変わらないようテキスト欄と同じ枠内に固定
- 自インスタンスに無いカスタム絵文字リアクションをクリック不可に
- Custom-protocolを既定で有効化しリリースビルドが常にdevサーバへ接続する問題を修正
- 設定モーダルの高さを画面内に収めスクロール可能にする
- 投票の選択肢に上限/削除ボタンを追加し期限指定にも対応
- 統計ポーリングタイマーの多重起動を防ぐ
- 流速をDB挿入件数ではなく投稿日時(created_at)基準に変更
- 背景画像設定時にリアクションの背景がカラムより濃く浮くのを修正
- 背景画像設定時にカラム内の各種背景要素が浮かないよう不透明度を統一
- 音声添付(mp3等)をアプリ内で再生できるようaudio要素を追加
- CIのtestワークフローを修正(pnpmバージョン明示、cargo test前にfrontendビルド)
- Pnpm/action-setupにversionを明示指定してCI失敗を修正
- Pnpm 11がNode.js 22.13+を要求するためCIのnode-versionを22に更新

### 🚜 Refactor

- *(stream)* WebSocket をアカウント単位1本＋チャンネル多重化に再設計
- 画像ライトボックスの自前実装をviewerjsライブラリに置き換え
- 投票確認をネイティブダイアログから独自モーダルに変更

### 📚 Documentation

- Phase 0 設計とスキャフォールド設計書
- README に起動方法を明記(dev起動のconnection refused対策)
- Mentionsソース未対応の理由とto_meでの代替をfilter-dsl-designに明記
- READMEの構成説明にdomainディレクトリを追記
- Progenitor不採用の実装結果をdocsに反映
- リポジトリ用CLAUDE.mdを新規作成
- V0.1.0のCHANGELOGを生成

### 🎨 Styling

- 基本テキスト色にアクセント系の色味を薄く乗せ、MFM rainbowを体感できるようにする
- 投稿窓をKrile風のカード型に変更(全要素内包・投稿ボタン右下・テキスト欄3倍高)
- アカウント選択を投稿窓の外に戻す
- 投稿バーのアカウント選択を2倍サイズに
- 投稿バーのアカウント選択(large)から下向き三角を除去
- 投稿窓のテキスト欄枠の高さを80pxに調整
- 投稿窓の余白を詰める
- 投稿窓の外枠(border/背景)を削除しappbarに溶け込ませる
- テーマ選択UIをグリッドカード表示に整理(全7色プレビュー+選択チェック)
- カスタムテーマの編集/削除をアイコンボタンに、新規作成をグリッド外の常設行に変更

### ⚙️ Miscellaneous Tasks

- フィルタDSL を TQL(Tsumugi Query Language) に改名
- GitleaksによるSecretsスキャンをGitHub Actionsに追加
- GitleaksワークフローのActionsをSHA固定し権限を最小化
- MITライセンスを追加
- Frontend/package.jsonのバージョンをsrc-tauriに合わせて0.1.0に統一
- Git-cliffによるCHANGELOG自動生成を導入しバージョニング方針を明文化
- テストCIとクロスプラットフォームリリースビルドCIを追加
- Gitleaksのpre-commitフックを追加
- Windows向けにインストーラー無しのポータブルexeもリリース資産へ追加

### ◀️ Revert

- CW切替時の投稿窓固定高さをやめ自然に伸縮する挙動に戻す

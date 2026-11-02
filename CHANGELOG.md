# Changelog

All notable changes to this project will be documented in this file.

## [0.13.0] - 2026-11-02

### 🚀 Features

- UiPrefsの背景画像フィールドをパス参照方式に変更
- 背景メディアをbackgrounds/へコピーするコマンドを追加
- 旧Base64背景画像を起動時に自動でファイルへ移行する
- 背景メディア配信用にassetProtocolを有効化
- 背景動画用のobject-fitマッピングを追加
- Storeの背景状態をbackgroundKind/backgroundPathに置き換え
- 背景の暗さオーバーレイを分離し動画レイヤーのCSSを追加
- 背景設定UIを画像/動画選択に対応させる
- カスタムCSS用のUiPrefs.customCssとis_safe_modeコマンドを追加
- カスタムCSSを<head>へ適用するapplyCustomCssとstore配線を追加
- カスタムCSSの設定UIとユーザーガイドを追加
- UiPrefsにui_scaleを追加しWebViewズームの権限を許可
- UIスケールの正規化とWebViewズーム適用を追加
- 起動時と保存時にUIスケールをWebViewへ反映
- 外観設定にUIスケールのスライダーを追加
- UIスケールのスライダー刻みを5%にする
- Backfill境界をソース単位のテーブルへ移す (#238)
- TQL複数ソースカラムのbackfillをキャッシュ優先にする (#238)
- ギャップ埋め後の境界引き上げ計算と復元一覧の絞り込み関数を追加 (#432)
- Backfillの透かし計算とパス間の取得結果マージ関数を追加 (#428)
- 透かし以上のノートだけを返すbackfillのパスループを追加 (#428)
- ノートメニューに「この投稿より前を取得」を追加し次のノートまでの穴を埋められるようにする
- サーバー検索の対応機能(SearchCapabilities)をバージョンから判定する
- 接続先サーバーのバージョン取得とAppStateキャッシュを追加する
- Notes/searchのリクエスト組み立て(SearchParams)を追加する
- サーバー検索コマンド(search_server_notes / get_search_capabilities)を追加する
- 検索モーダルにサーバー検索を追加する
- UiPrefsに翻訳先言語を追加する
- ノート翻訳のREST取得と翻訳可否のキャッシュを追加する
- ノート翻訳のTauriコマンドを追加する
- 翻訳先言語の設定とstoreメソッドを追加する
- ノートメニューから翻訳して本文の下に表示する
- カラムごとの世代とロックを持つColumnFenceを追加する
- 世代が現在の時だけキャッシュへ書く3種のヘルパーを追加する
- ColumnFenceに全カラム共通の境界の世代と書きロックを足す
- ワードミュートのルールの安定した文字列表現key()を追加する
- サーバー側ミュートの前回の集合をアカウントごとに設定へ保存する
- サーバー側ミュートを6時間ごとに同期する
- クリップボードのfile:// URIを実在ファイルパスへ変換する関数を追加
- Read_clipboard_files / read_clipboard_text コマンドを追加
- Paste横取りの同期判定shouldInterceptPasteを追加
- 投稿欄でファイルマネージャのファイル貼り付けに対応
- 予約投稿のサーバー対応判定を追加(#60)
- 本家Misskeyの予約投稿APIラッパとScheduledNote型を追加(#60)
- 予約投稿のTauri commandとバインディングを追加(#60)
- 予約投稿の日時変換・検証・ストアのラッパを追加(#60)
- ComposeBarに予約日時の設定と予約送信を追加(#60)
- 予約一覧モーダルと作成欄に戻す機能を追加(#60)
- Flatpickrベースの日時入力DateTimeInputを追加(#60)
- クライアント側予約の型とストアを追加(#60 B)
- クライアント側予約のスケジューラの中核ロジックを追加(#60 B)
- クライアント側予約のcommandとイベントと起動配線を追加(#60 B)
- E2E用にデバッグビルド限定でサーバーバージョンを上書きできるようにする(#60 B)
- ローカル予約の一覧用ヘルパとストアのラッパ・イベント購読を追加(#60 B)
- ComposeBarで予約の経路をサーバー側とローカルに振り分ける(#60 B)
- 予約一覧にローカル予約を統合し、今すぐ投稿と作成欄に戻すを対応する(#60 B)
- 取得先検証の判定ロジック(net_guard)を追加(#377)
- 検証付きDNSリゾルバとリダイレクトポリシーの取得専用クライアントを追加(#377)
- Noteに返信先ノートを浅く保持する(#287)
- キャッシュのuser参照処理を返信先にも適用する(#287)
- キャッシュの自己修復と往復テストを返信先に対応する(#287)
- 返信先プレビュー本文の決定ロジックを追加する(#287)
- 返信ノートに返信先の1行プレビューと展開を追加する(#287)
- カラムと通知で返信先をアバター付きカードで表示する(#287)
- 全画面モーダル用の履歴スタック管理を追加する(#450)
- モバイルでModalを全画面表示にし戻る操作で閉じられるようにする(#450)
- モバイルの設定画面を上部の横スクロールタブ帯にする(#450)
- モバイルの投稿画面を共通Modalの全画面表示に置き換える(#450)
- AndroidのキーボードをWebViewのCSS変数--keyboard-insetとして通知する
- 全画面モーダルの下余白にキーボード高さを含める
- モバイルの投稿画面で入力欄を伸ばしボタン群を下端に寄せる
- 返信先ノートのミュート判定を追加(Issue #486)
- ミュート対象の返信先を外すAppStateメソッドを追加(Issue #486)
- カラムの初期取得・復元・ギャップ埋めでミュート対象の返信先を外す(Issue #486)
- 追加取得・検索・通知一覧でミュート対象の返信先を外す(Issue #486)
- ストリーム受信でミュート対象の返信先を外す(Issue #486)
- NG設定の保存時に表示中のノートと通知の返信先カードを外す(Issue #486)
- MMLパーサーとドメイン型を追加(Issue #75)
- Parse_mmlコマンドと通知音保存時のMML検証を追加(Issue #75)
- ToneでMMLをWAVにレンダリングする処理を追加(Issue #75)
- Sound_choiceのmml:を再生前にWAVへ解決する(Issue #75)
- 通知音の選択UIを共通化しMML入力モードを追加(Issue #75)
- MMLに音色指定(@)を追加しNoteEventに音色を持たせる(Issue #494)
- MMLの音色プリセット表を追加(Issue #494)
- MMLレンダラーを音色ごとの音源に切り替える(Issue #494)
- MMLの音色の音量補正を実描画で校正する(Issue #494)
- MMLの上限を90秒・5000文字・3000音に広げる
- MMLの使用量の計算と上限の表を追加(Issue #500)
- MMLのエラー位置をエディタの添字に直す関数を追加(Issue #500)
- MMLの色分け用の字句解析を追加(Issue #500)
- MMLエディタのCodeMirror拡張(色分け・エラー下線)を追加(Issue #500)
- CodeMirrorを使うMML入力欄のコンポーネントを追加(Issue #500)
- MML入力欄をエディタにして、エラー下線と使用量を出す(Issue #500)
- MML入力欄にカーソル位置へ文字を入れる関数を追加(Issue #500)
- MML入力欄に音色とサンプルの挿入ボタンを追加(Issue #500)
- 通知音の再生に、生成結果を捨てる条件と完了通知を追加(Issue #500)
- MML の試聴に生成中の表示と、待たないボタンを追加(Issue #500)
- MML エディタの挿入に、空白を足さないオプションを追加(Issue #500)
- モバイルの MML 入力欄に、音符・記号・長さの入力補助キーを追加(Issue #500)
- MML にコメント(// と /* */)を追加(Issue #499)
- MML に ^ タイ(長さを足す)を追加(Issue #499)
- MML に相対音量 v+n / v-n を追加(Issue #499)
- MML にゲートタイム q を追加(Issue #499)
- MML のループに脱出 : とネストを追加(Issue #499)
- MML に連符 { }n を追加(Issue #499)
- MML の新しい記法を色分けし、コメントの中の ; をトラックに数えない(Issue #499)
- MML の入力補助キー・記法ヘルプ・ガイドを新しい記法に合わせる(Issue #499)
- MML に全トラックの初期テンポ t= を追加(Issue #499)
- MML の t= を色分けし、ヘルプとガイドに追記(Issue #499)
- MML に和音 'ceg' を追加(Issue #499)
- MML の和音を色分けし、入力補助キー・ヘルプ・ガイドに追記(Issue #499)
- MML にマクロ $name={…} を追加(Issue #499)
- MML のマクロを色分けし、トラック数・入力補助キー・ヘルプ・ガイドに追記(Issue #499)
- MML の音色の挿入ボタンを、記法の外の入力欄の下へ移す(Issue #512)
- MML に音色 @14 drumkit を追加し、音色名を最長一致で読む(Issue #499)
- MML の drumkit を、GM の割り当てで楽器ごとに鳴らす(Issue #499)
- MML に音色パラメーター y m k を追加(Issue #499)
- MML の音色パラメーターを、音源ごとに反映して描画する(Issue #499)
- MML にポルタメント p を足し、NoteEvent に glide を載せる(Issue #499)
- MML のポルタメントを、旋律の音源の周波数を滑らせて描画する(Issue #499)
- MML にリバーブ x とディレイ z を足し、NoteEvent に reverb と delay を載せる(Issue #499)
- MML のエフェクトを、トラックのバスと送りで描画する(Issue #499)

### 🐛 Bug Fixes

- 背景メディアインポートをストリーミングコピー化しパス検証を追加
- Backgrounds_dirをapp_data_dirに統一する
- 旧背景画像のBase64デコード失敗時に起動を継続する
- 背景動画が非表示時にpause()するようにする
- AssetProtocol利用にtauriのprotocol-asset featureを追加
- 背景メディア未保存時の消失とゴミ残留を起動時GCで解消
- 背景動画をblob URLで読み込み、WebKitGTKのasset://非対応を回避
- Teardown時に背景動画のvideo要素をDOMから削除する
- 背景設定画面の動画プレビューをblob URLで読み込む
- 背景メディアの種類判定を拡張子欠落時もマジックバイトで判定する
- WaylandのError 71回避を__NV_DISABLE_EXPLICIT_SYNCに変更しHWアクセラレーションを維持する
- メンションチップと本文テキストのベースラインずれを修正
- Instance Tickerのアイコンサイズと角丸を修正
- リモートユーザーのInstance Tickerアイコンにfaviconを使う
- 外部画像のRefererを既定で送らずCloudflareの403でアイコンが表示されない問題を修正する
- カスタムCSSのセーフモード判定をfail-closedにし、入力欄ラベルとサイズ上限を追加
- 他クライアントで付けた自分のリアクションのreacted/unreactedを反映する
- タブ名だけを編集した場合にノートがリセットされないようにする
- E2e実行後に隔離プロセスと一時ディレクトリが残留する問題を修正
- UIスケールの範囲外保存値を正規化し同値の再適用を抑止
- Backfillキャッシュ優先をストリーミング対応ソースのみのカラムに限定する (#238)
- Fetch_backfillにbypass_cacheを追加しギャップ埋めのキャッシュHitを避ける (#427)
- FillRemainingGapがキャッシュ優先を避けてAPIからギャップを埋める (#427)
- 打ち切られたギャップ埋め後にbackfill境界を引き上げ復元一覧を境界で絞る (#432)
- 全ソースが追いついた場合の切り捨ても境界引き上げの対象にする (#432)
- FillRemainingGapでMAX_NOTES切り捨てをやめマーカーの基準ノートを残す (#433)
- 複数ソースbackfillで透かし以上のノートだけを返し読み飛ばしを防ぐ (#428)
- Backfillの画面用を積み上げ、途中で失敗したソースの結果を凍結する (#428)
- ギャップ埋めが切り捨てを伴うとき全ソースが追いついてもマーカーを出す
- Commands/user.rs の未使用 import ColumnKind の警告を解消する
- サーバー検索で開始日時のみ指定時の順序とミュート全滅ページの終端誤判定を修正する
- 検索の追加読み込みを入力欄の現在値ではなく検索時点の条件で行う
- 検索モーダルの検索対象切替とモード切替を1行に並べる
- 同じidのノートに差し替わっても翻訳表示を保つ
- 翻訳先言語の選択をテーマ適用済みのDropdownにする
- 全ソース枯渇済みのbackfillはキャッシュ件数不足でもキャッシュで返す
- Backfill境界の読み出しエラーをFallbackBoundaryUnsetと区別して計上する
- 取得中にupdate_column/close_columnが走った場合はキャッシュへ書かない
- UpdateColumnをまたいで返ったbackfillの結果を画面へ混ぜない
- ストリームを開く処理を世代ロック内で行い古い取得は明示的に失敗させる
- Set_mute後に旧ミュートの結果に基づく境界を書き込まない
- Set_muteの境界の破棄を境界の書きロックの中で行う
- サーバー側ミュートの解除を検出してそのアカウントの境界を捨てる
- 画像ビューワーでmoveハンドルを置きはみ出し判定でパンを有効にして拡大した画像をパンできるようにする
- 画像ビューワーの回転でCropper.jsへ度の角度を渡し90度ずつ回転させる
- UpdateColumnをまたぐ通知のloadMoreの結果を捨て、旧定義のgapMarkerを消す
- 未知のカラムIDでの世代エントリのリークとアカウント削除時のミュート保存値の残りを直す
- サーバー側ワードミュートのキーを生の要素から作り、iフラグだけの解除と不正な正規表現の誤検出を直す
- 同じアカウントのサーバー側ミュート同期をアカウント単位のロックで直列にする
- Fetch_backfillの境界の延長を、ロックの中で読んだ最新の境界から計画する
- Update_columnのグループ検索とトークン取得をinvalidateの前へ移す
- MacOS/Windowsが返す生パスのファイル参照も受け付ける
- コードブロックのDOM挿入直後に同期レイアウトを強制してフッターとスクロールバーの重なりを防ぐ
- 予約投稿の最終レビュー指摘を修正(#60)
- 予約と投票締切の日時入力をflatpickrに置き換えてLinuxでも時刻を選べるようにする(#60)
- AndroidでflatpickrのonReadyが例外になる問題と時刻の手入力が遅れて反映される問題を直す(#60)
- 予約一覧ボタンを枠付きにしてクリックできると分かるようにする(#60)
- 予約投稿の送信にタイムアウトを設け、起動時の結果を通知する(#60 B)
- 任意URL取得コマンドの取得先を検証する(#377)
- Unicode表記のアカウントホストも許可リストで照合できるようにする(#377)
- 返信先プレビューの改行を畳んで1行表示を保つ(#287)
- 返信先プレビューのローカル絵文字解決とフォーカス表示を追加する(#287)
- 背景画像をblob URL経由で読み込みWebKitGTK 2.54のasset://非対応を回避する
- UIモード切替でモーダル本文が作り直されないよう単一ツリーにする(#450)
- モバイルの設定タブ帯の横スクロールバーを隠す(#450)
- モーダル履歴を着地先のstateで判定し、popstateが来なくても回復するようにする(#450)
- モバイルの検索・フォロー一覧・プロフィールでPC用の高さ上限と二重スクロールをやめる(#450)
- モバイルの設定タブ帯がスクロール領域のパディング分ずれて固定されるのを直す(#450)
- Modalのportal移動と初期フォーカスが子のフォーカスを奪わないようにする(#450)
- 投稿画面のメニューと補完候補が下に空きが無いとき上に出るようにする
- キーボードが出たとき全画面モーダル内のフォーカス欄を上へスクロールする
- モバイル投稿画面のツールバー操作でキーボードが閉じてメニューが取り残されるのを防ぐ
- 取得先許可リストのホストでも許可外ポートへの取得を拒否する(Issue #482)
- 取得先検証のIP禁止範囲にNAT64・6to4・予約済み範囲を追加する(Issue #481)
- IANA特殊用途登録簿に基づきドキュメント用・Teredo等の禁止範囲を追加する(Issue #481)
- 通知音ピッカーが非表示のときMMLの下書きを保存しない(Issue #75)
- MML着メロの全体レビュー指摘を修正(Issue #75)
- MML入力欄の読み込み失敗時のフォールバックと、編集時の下線の自動消去を追加(Issue #500)
- 入れ子の繰り返しで、足さない周の本体を複製しない(Issue #499)
- T= を並べた入力で、そのたびに命令列をなめ直さない(Issue #499)
- 和音の後の ^ を大量に並べても、そのたびに全部の音を書き直さない(Issue #499)
- MML のレビューで見送った軽微な指摘 6 件を直す(PR #513・#514)
- MML の見送った軽微な指摘 4 件を直す(PR #513〜#515)
- MML マクロの展開回数に上限を設け、展開後の長さの色分けを直す(Issue #499)
- MML の drumkit の音源を楽器ごとに使い回し、音量を校正して実機の確認を記録(Issue #499)
- MML の drumkit で、同じ楽器を同じ時刻に鳴らしても描画が失敗しないようにする(Issue #499)
- MML のデチューンを周波数の倍率にして、音源の組に数えない(Issue #499)
- MML のビブラートの深さを、速さに依らない揺れの幅(セント)にする(Issue #499)
- MML の音色パラメーターの見送った軽微な指摘 4 件を直す(Issue #499)
- MML の描画を 1 つずつ順に走らせ、エフェクトの尾の端をフェードさせる(Issue #499)
- MML のエフェクトの送りをランプで切り替え、描画失敗時も Tone のコンテキストを戻す(Issue #499)
- Webpki-roots を 1.0.11 に上げ、失効した emSign ECC Root CA - G3 を信頼から外す

### 🚜 Refactor

- 背景メディアのマジックバイト判定をinferクレートに置き換え
- SQLiteキャッシュのインスタンス情報をinstanceテーブルへ正規化する
- PostgreSQLキャッシュのインスタンス情報をinstanceテーブルへ正規化する
- MySQLキャッシュのインスタンス情報をinstanceテーブルへ正規化する
- キャッシュ書き込みをcache_fetchedに集約し回帰テストで固定する (#238)
- Flatpickrの呼び出しとテーマCSSをSearchModalから共有ファイルへ切り出す(#60)
- カード以外の返信先表示を従来の「返信」だけに戻しカード下の余白を広げる(#287)
- 取得先許可リストをホストと許可ポートの集合で持つ(Issue #482)
- SetMuteで変化のない通知のオブジェクトを再生成しない(Issue #486)

### 📚 Documentation

- 背景動画対応の設計ドキュメントを追加 (#46)
- 背景動画対応の実装計画を追加 (#46)
- 計画書のbackgrounds_dirディレクトリ不一致を修正 (#46)
- Wayland描画回避の環境変数変更をREADMEとユーザーガイドに反映
- Issue #409 instanceテーブル正規化のspecを追加
- Issue #409 instanceテーブル正規化の実装プランを追加
- CLAUDE.mdに実DBテストの実行方法・キャッシュ移行方針・Referer規約を追記する
- Streaming再接続テストの設計書を追加する
- 再接続テスト設計書の変異テスト結果を更新する
- CLAUDE.mdのfilter/sql記述とrelease.sh更新対象・コメント参照先を実装に合わせて修正
- カスタムCSS(#93)の設計書を追加
- カスタムCSS(#93)の実装planを追加
- E2E残留プロセスの自動回収についてCLAUDE.mdを更新
- UIスケール設定(#40)のspecを追加
- UIスケール設定(#40)の実装計画を追加
- UIスケールのplanとspecをレビュー対応の実装に合わせる
- TQL複数ソースカラムのbackfillキャッシュ優先化のspecを追加 (#238)
- TQL複数ソースカラムのbackfillキャッシュ優先化の実装計画を追加 (#238)
- Planの単一ソース挙動と実機確認項目を最終レビュー後の仕様に合わせる (#238)
- FillRemainingGapのキャッシュ優先バイパス設計を追加 (#427)
- FillRemainingGapのキャッシュ優先バイパス実装計画を追加 (#427)
- 打ち切られたギャップ埋め後のbackfill境界引き上げ設計を追加 (#432)
- 打ち切られたギャップ埋め後のbackfill境界引き上げ実装計画を追加 (#432)
- 複数ソースbackfillの読み飛ばしを防ぐ透かし方式の設計を追加 (#428)
- 複数ソースbackfillの透かし方式の実装計画を追加 (#428)
- 切り捨て時にマーカーが出ない限界の記述を修正済みに更新する
- サーバーサイド検索の設計書と実装計画を追加する
- ユーザーガイドにサーバー検索を追記する
- サーバー検索の設計書にページング挙動の補足を追記する
- ノート翻訳機能の設計書を追加する
- ノート翻訳の実装計画を追加しspecの未確定項目を確定する
- ノート翻訳の使い方をユーザーガイドに追記する
- スタイルガイドにネイティブselectを使わない規約を追記する
- 実DBテスト後のコンテナ削除手順にdocker rm -vを明記する
- Replace_fetch_boundariesが重複source_idxで失敗することを明記する
- 複数ソースbackfill設計に#429での判定変更を追記する
- 実DBテストはdropでコンテナを消す前提にCLAUDE.mdを直す
- 実DBテストの並列度とOOMの説明を事実だけに直す
- カラムのキャッシュ書き込みの世代フェンス設計を追加する
- 世代フェンス設計でinvalidateをf実行後の世代更新に直す
- カラムのキャッシュ書き込みの世代フェンス実装計画を追加する
- 世代フェンスの設計と計画に最終レビュー後の変更を反映する
- Set_muteと実行中の取得の競合を防ぐ境界の世代の設計を追加する
- ギャップ埋めの境界の引き上げは飛ばさないようspecを直す
- Set_muteと実行中の取得の競合を防ぐ境界の世代の実装計画を追加する
- サーバー側ミュートの解除をキャッシュ優先のbackfillへ反映する設計を追加する
- サーバー側ミュートの解除をキャッシュ優先のbackfillへ反映する実装計画を追加する
- サーバー側ミュート同期の正しさの設計(Issue #456 P2)を追加
- サーバー側ミュート同期の正しさの実装計画(Issue #456 P2)を追加
- サーバー側ミュートの既知の制限が解消したことを追記
- 世代フェンス・ミュート境界のテスト可能性とテスト追加の設計(Issue #456 P3)を追加
- 世代フェンス・ミュート境界のテスト可能性とテスト追加の実装計画(Issue #456 P3)を追加
- テスト用のAPIベース上書きの注意をコメントに足し、フィールドをprivateにする
- Fetch_backfillの境界延長の競合とupdate_columnの失敗順序の設計(Issue #456 P5)を追加
- Fetch_backfillの境界延長の競合とupdate_columnの失敗順序の実装計画(Issue #456 P5)を追加
- サーバー側ミュートの定期同期の設計(Issue #456 P4)を追加
- サーバー側ミュートの定期同期の設計に、ストリーミング通知の調査結果を追記する
- サーバー側ミュートの定期同期の実装計画(Issue #456 P4)を追加
- ファイル参照クリップボード貼り付け(#117)の設計を追加
- #117のspecにclipboardData実測結果とread_clipboard_textを反映
- ファイル参照クリップボード貼り付け(#117)の実装計画を追加
- ファイル貼り付けの対応をユーザーガイドに追記
- WebKitGTKのスクロールバー高さのレイアウトずれと単一ファイルのvitest実行をCLAUDE.mdに追記
- 予約投稿(サーバー側、#60)の設計を追加
- 予約投稿(#60)の実装計画を追加
- 予約投稿のユーザーガイドを追加しspecを実装に合わせて更新(#60)
- Specの日時変換の記述をepoch秒に直す(#60)
- 日時入力(DateTimeInput)の共有化の設計を追加(#60)
- 日時入力(DateTimeInput)の共有化の実装計画を追加(#60)
- 日時入力はネイティブinputを使わずDateTimeInputを使う旨を追記(#60)
- 予約投稿(クライアント側フォールバック、#60 B)の設計を追加
- クライアント側予約の実機確認をE2Eに切り替える設計を追記(#60 B)
- クライアント側予約(#60 B)の実装計画を追加
- クライアント側予約のユーザーガイドと構成を追記(#60 B)
- 取得系コマンドのSSRF対策の設計を追加(#377)
- SSRF対策の仕様に許可リストのキー正規化とIPリテラルの例外を追記(#377)
- SSRF対策の実装計画を追加(#377)
- 取得先検証の説明をユーザーガイドとCLAUDE.mdに追記(#377)
- 返信先プレビューの設計specと実装planを追加(#287)
- 返信先プレビューの説明を追記する(#287)
- 返信先プレビューの既知の制限とラベルを追記する(#287)
- 返信先カードの設計とユーザーガイドを更新する(#287)
- 返信先表示の仕様をカードと「返信」の2通りに更新する(#287)
- Asset://の読み込み失敗がdev(httpオリジン)に限られる旨へ記述を修正する
- モバイルのモーダル全画面化の設計を追加する(#450)
- モバイルのモーダル全画面化の実装計画を追加する(#450)
- モバイルのモーダル全画面化をスタイルガイドとユーザーガイドに反映する(#450)
- モバイルのモーダルでの高さ上限とstickyの扱いをスタイルガイドに追記する(#450)
- モバイル投稿画面の入力欄をキーボード上端まで広げる設計を追加する
- モバイル投稿画面のキーボード高さ対応の実装計画を追加する
- 取得先許可リストをポート単位にする設計(Issue #482)
- 取得先許可リストのポート単位化の実装プラン(Issue #482)
- 取得先検証の設計書を許可リストのポート単位化に合わせて改訂(Issue #482)
- 返信先カードがミュート設定を考慮する設計書を追加(Issue #486)
- 返信先カードがミュート設定を考慮する実装計画を追加(Issue #486)
- 返信先カードがミュート設定を考慮する旨をユーザーガイドに反映(Issue #486)
- Issue #75 着メロ打ち込み(MML)の設計を追加
- Issue #75 MML着メロの実装計画を追加
- Issue #75 Android での Tone.Offline 検証結果を追記
- ユーザーガイドにMMLで通知音を作る方法を追加(Issue #75)
- Issue #75 実装後の確認結果を追記
- MML着メロの音色切り替えの設計スペックを追加(Issue #494)
- MML音色切り替えの実装計画を追加(Issue #494)
- 音色校正結果の注記を計測に合わせて直す(Issue #494)
- MMLの音色指定のヘルプとユーザーガイドを追加(Issue #494)
- MML音色の実装後の確認結果を追記(Issue #494)
- MMLの上限変更をガイドとスペックに反映する
- MML上限変更後の確認結果を追記する
- 重いMMLは生成に時間がかかることをガイドに追記する
- 重いMMLの注意書きを測定の範囲に合わせて直す
- 重いMMLの記述の言い過ぎを直す
- MML入力欄のエディタ化の設計を追加(Issue #500)
- MML入力欄のエディタ化の実装計画を追加(Issue #500)
- MML入力欄のエディタ化をガイドに反映し、確認結果を追記(Issue #500)
- MML のサンプルと音色の挿入ボタンをガイドに反映(Issue #500)
- MML の試聴の生成中表示と待たないボタンをガイドに反映(Issue #500)
- MML の入力補助キーをガイドに反映(Issue #500)
- MML 構文の拡張(タイ・相対音量・コメント・連符・ループ・q)の設計を追加(Issue #499)
- MML 構文拡張の実装計画を追加(Issue #499)
- MML 構文拡張の実装後の確認を記録(Issue #499)
- MML 構文拡張のレビュー後の修正を記録(Issue #499)
- MML のテンポ共有 t= の設計を追加(Issue #499)
- テンポ共有の spec に他方言の調査結果を追記(Issue #499)
- MML のテンポ共有の実装計画を追加(Issue #499)
- MML のテンポ共有の実装後の確認を記録(Issue #499)
- MML の和音 'ceg' の設計を追加(Issue #499)
- MML の和音の実装計画を追加(Issue #499)
- MML の和音の実装後の確認を記録(Issue #499)
- MML の和音のレビュー後の修正を記録(Issue #499)
- MML のマクロ $name={…} の設計を追加(Issue #499)
- マクロの名前を、空白で区切る方式に改める(Issue #499)
- MML のマクロの実装計画を追加(Issue #499)
- MML のマクロの実装後の確認を記録(Issue #499)
- MML のドラムの割り当て @drumkit の設計を追加(Issue #499)
- MML の drumkit の実装計画を追加(Issue #499)
- MML の drumkit の割り当てを、ヘルプとユーザーガイドに追記(Issue #499)
- Drumkit の実装後の確認の日付を直す(Issue #499)
- MML の音色パラメーター y m k の設計を追加(Issue #499)
- MML の音色パラメーターの実装計画を追加(Issue #499)
- MML の音色パラメーターを、色分け・入力補助キー・ヘルプ・ガイドに追記(Issue #499)
- MML の音色パラメーターの実装後の確認を記録(Issue #499)
- MML のポルタメントの設計を追加(Issue #499)
- MML のポルタメントの実装計画を追加(Issue #499)
- ポルタメントの実装計画の音色の数を直す(Issue #499)
- MML のポルタメント p を、色分け・入力補助キー・ヘルプ・ガイドに追記(Issue #499)
- MML のポルタメントの実装後の確認を記録(Issue #499)
- MML のポルタメントの見送った軽微な指摘 2 件を直す(Issue #499)
- MML のエフェクト(リバーブ・ディレイ)の設計を追加(Issue #499)
- MML のエフェクトの実装計画を追加(Issue #499)
- MML のリバーブ x とディレイ z を、色分け・入力補助キー・ヘルプ・ガイドに追記(Issue #499)
- MML のエフェクトの実装後の確認を記録(Issue #499)
- MML のエフェクトの全体レビュー後の測り直しを記録(Issue #499)
- MML のエフェクトの軽微な指摘の対応を実機で確認した結果を記録(Issue #499)

### 🧪 Testing

- Streaming再接続の自動テストを追加しconnectionをRuntimeジェネリック化する
- Backoffリセットテストの閾値を広げてCI負荷でのflakyを避ける
- Postgres/MySQLの境界テストに枯渇行の延長と全滅pruneと重複idxを追加する
- 実DBテストのコンテナをforgetせずdropで削除する
- 同じ正規表現の再同期で境界と保存値が変わらないテストを足し、古いコメントを直す
- Apply_mute_configの、設定の差し替えが境界の破棄より先であることを固定する
- 境界の破棄が失敗したときの保存値と、sync_server_mutesのラッパを固定する
- Update_column・close_column・open_stream_and_fetchの古い場合の分岐を固定する
- 実DBで境界の書きロックの中のclear_all_fetch_boundariesが詰まらないことを確認する
- 実DBの境界ロックのテストを、合図で順序を決めて、書きロックが実行中の書き込みを待つ場面を再現する
- 古い世代のopen_stream_and_fetchのテストを、ノートを返すモックで実効性のあるものにする
- 並行テストのidの桁数を揃え、ロックの外で読む変異で落ちるようにする
- Boundaries_okのガードの回帰テストを足し、並行テストの待ちと実効性のないアサートを直す
- 境界を読めなかったときのログを足し、update_column後のテストのuntil_idを意図に合わせる
- 定期同期のテストの後始末とコメントを足し、再認証が必要なアカウントの警告をspecに書く
- カラム編集・NG設定・サーバー側ミュートの境界を実機で確認するE2Eを足す
- DateTimeInputのタイムゾーンと外部更新のテストを実効性のあるものにする(#60)
- DateTimeInputのループ経路のテストでスパイを選択前に付ける(#60)
- クライアント側予約の投稿・取り消し・作成欄に戻すのE2Eを追加(#60 B)
- 予約E2Eの予約時刻までの余裕を広げ、遅い環境での失敗原因を分かりやすくする(#60 B)
- クライアント側予約の再起動をまたぐE2EとCIステップを追加(#60 B)
- モバイル投稿画面のキーボード高さのE2Eとドキュメントを追加する
- ストリーム受信の返信先除去とキャッシュ保全のテストを追加(Issue #486)

### ⚙️ Miscellaneous Tasks

- OpenAPIスナップショットを本家Misskey 2026.10.0に更新
## [0.12.0] - 2026-09-27

### 🚀 Features

- Domain::Note に reply_user_id を追加
- RawNote から reply_user_id を抽出する
- TQL の reply_to_me 述語を実装する
- Note_cache に reply_user_id の実値を保存する
- 長押しドラッグ検知の状態機械を追加
- カラムドラッグの前後ヒント判定を追加
- カラム隣接swapの純粋関数を追加
- AppStoreにmoveColumnAdjacent/canMoveColumnAdjacentを追加
- モバイル版でタブの長押しドラッグ並び替えに対応
- モバイル版でカラムの長押しドラッグ並び替えに対応
- カラムメニューに左右移動ボタンを追加
- Misskey APIアクセスログをclient.rs::postに追加
- キャッシュhit/fallbackカウンタCacheMetricsを追加
- Resume_column/fetch_backfillにキャッシュ計装を追加
- Get_debug_metricsコマンドを追加
- Devビルドでは設定に関係なく常にロガーを登録する
- Streaming受信時のフィルタ/ミュートdropログを追加
- BackstageにキャッシュメトリクスのUIを追加
- 純粋Renoteの取り消しメニューを追加
- UiPrefsに開発者オプション解除フラグを追加
- 外部連携タブを新設しMFM検索エンジン/URLプレビュー設定を移動
- デバッグ設定を開発者オプションセクションへ分離
- バージョン表示7回タップで開発者オプションを解除する
- 設定タブを性質軸で再構成する(外部連携新設・データ統合・開発者オプション追加)
- 矢印キー選択移動専用のselectionMoveSeqをTabViewに追加
- MediaViewer用の純粋ロジックを追加
- 画像の拡大表示をviewerjsからMediaViewerへ移行
- MediaViewerに動画・音声表示(Vidstack + panzoom)を追加
- 動画・音声タイルに拡大ビューワー起動ボタンを追加
- MediaViewerに読み込み失敗時のエラー表示を追加
- MediaGridの動画・音声タイルをVidstack自前コントロールバー化
- MediaGridのインラインプレイヤーにシークバーを追加
- MediaGridにホバー展開する音量スライダーを追加
- MediaViewerの動画コントロールバーにフルスクリーンボタンを追加
- 動画クリックで再生/一時停止をトグルできるようにする
- コントロールバーに再生時間/合計時間を表示
- フルスクリーンビューワーの余白クリックで閉じられるようにする
- ビューワーを等倍時の縦スワイプで閉じられるようにする
- DriveFileにファイルサイズを追加する
- ファイルサイズ整形関数を追加する
- 非メディア添付ファイルをリスト表示して保存ダイアログで開く
- UiPrefsにcatModeを追加 (#42)
- CatModeに応じてnyaizeと猫耳を強制切替する (#42)
- 外観設定に猫化モードの選択を追加 (#42)

### 🐛 Bug Fixes

- Tauri-plugin-hapticsのCargo.tomlからlinksキーを削除しDependabotのcargo更新失敗を修正
- Linksを維持しつつbuild.rsを明示してDependabotのcargo更新失敗を修正
- Viewerjs/@lucide-svelteの破壊的型変更に追従してfrontend-checkを修正
- Jni 0.22 の Env/EnvUnowned 分離に mobile_intent.rs を追従
- Sqlx 0.9 の SqlSafeStr 監査要求に MySQL/Postgres バックエンドを追従
- Local/Hybridカラムで withReplies:true を送るようにする
- MySQL/Postgresバックエンドでreply_user_idを実値保存する
- Tauri-plugin-notificationをnpm側と同じ2.4系に更新
- タブ長押しドラッグのarmed判定順序とpointer capture漏れを修正
- タブ長押しドラッグのpointer captureを長押し成立後に遅延
- カラム並び替えをpaneRoot(movePane)経由に修正
- モバイル長押しドラッグのtouch-action・判定範囲・ヒント表示を修正
- 縦分割rootで左右移動が可能に見えるのを修正
- 実機確認で判明したセーフエリア無視とタブ長押し横スクロール競合を修正
- 実機報告を受けタッチターゲット拡大・長押し閾値緩和・文字列選択抑制
- 長押しドラッグ中にノート本文が誤って文字列選択されるのを修正
- モバイル版でnative HTML5ドラッグが長押しジェスチャーを横取りする問題を修正
- *(仮説検証)* タブのpointer captureを移動しないタブバーコンテナへ変更
- タブ長押しドラッグが途中で固着する問題を修正、診断ログを除去
- PKGBUILD自動更新ワークフローがdetached HEADで失敗する問題を修正
- Vitestカバレッジのinclude設定漏れとコメント欠落を修正
- E2E用MisskeyのIPベースsignin回数制限を無効化
- Api/filterログをrelease版でも有効にしログローテーションを緩める
- 最終レビュー指摘(開発者オプション解除の堅牢化・スタイルガイド更新)を反映
- タブ切替時に選択ノート位置へ自動スクロールしないようにする
- NoteCardのselectionMoveSeq初期値捕捉をuntrackで明示する
- キーバインドによる暗黙選択でもselectionMoveSeqを増やす
- Codecov.ymlのcomment設定をオブジェクト形式で明示する
- E2e run-app.shでWAYLAND_DISPLAYをunsetし実画面への漏れを防ぐ
- 通知カラムのWS接続完了を待ってからメンションE2Eを検証する
- Jsdom unhandled rejection対応(test-setupで VTTCue及び Observers をスタブ化)
- MediaGridの拡大・保存ボタンの重なりと絵文字アイコンを修正
- MediaPlayerのsrcにtypeを明示し拡張子なしURLでの読み込み不能を修正
- シークバーtrackの色をaccentトークン経由に修正
- シークバーのpointer-eventsをVidstackインラインstyleより優先させる
- MediaViewerのラップアラウンド送りで表示位置がずれる不具合を修正
- MediaViewerの前後送り連打で表示位置がずれる不具合を修正
- StartIndex:0でscrollendが発火せずフラグが解除されない不具合を修正
- MediaViewerのページ送りでscrollIntoViewの目的地誤判定によりズレる不具合を修正
- MediaViewerのラップアラウンド送りが常に瞬時ジャンプになる不具合を修正
- MediaViewerのcropperImageEl取り違えによる画像変形の不具合を修正
- メディアビューワー全体コードレビュー指摘(I-1〜I-3、M-2〜M-6)を修正
- 動画ビューワーのtouch-actionをパン有効状態に連動させる
- 音声タイルのグリッド行ストレッチと波形CORS/MIME不具合を修正
- 音声コントロールバーが幅を使い切らず縦上寄せになる不具合を修正
- 音声コントロールバーのボタン配置を動画側と統一
- 音声プレイヤーの波形を1.5倍に拡大しシークバーを非表示化
- 波形の再生済み部分の色が反映されない不具合を修正
- 音声波形をVidstackのaudioに干渉しない方式にしbase64をやめ生バイト取得へ変更
- フルスクリーンビューワーの動画・音声がCORSで再生できない不具合を修正
- ピンチズーム中に縦スワイプ判定が誤発動して薄くなる・閉じる不具合を修正
- ページのroleを外して既存テストを通し余白クリックのテストを追加
- 前後送りクリック領域を矢印ボタン周りに絞り左右の余白で閉じられるようにする
- メディアグリッドの各セルを個別に角丸にする
- スマホ版メディアビューワーのスワイプ・スクロールバー・送りボタン表示を修正する
- レビュー指摘(モーダル閉鎖待ち・設計書のCI記述・style-guide節番号)を反映する
- E2E用Misskeyのファイルボリューム権限を初期化して画像アップロードを可能にする
- ファイルリストの箇条書きマーカーとインデントを消す
- Vidstack未対応MIMEの動画(video/quicktime等)をvideo/mp4として渡して再生可能にする

### 💼 Other

- カラムグリップのポインターイベントに一時診断ログを追加
- タブのポインターイベントに一時診断ログを追加
- Viewerjsをcropperjs/vidstack/panzoomに入れ替え

### 🚜 Refactor

- メディアのダウンロード処理をlib/mediaDownload.tsへ共通化
- MediaGridのコントロールバーマークアップをsnippetで共通化
- セーフエリアのenv()をCSS変数経由にしE2Eから上書き可能にする

### 📚 Documentation

- Sqlx AssertSqlSafe 使用箇所に監査根拠のコメントを追記
- タイムライン返信可視性修正の設計書を追加
- タイムライン返信可視性修正の実装プランを追加
- Local/Hybridの返信全表示化をユーザーガイドに追記
- ユーザーガイドのreply_to_me例を修正
- CLAUDE.mdのstore/session/commands記述を実装に合わせて更新
- モバイル版タブ/カラム並び替え(Issue #354)のdesign docを追加
- モバイル版タブ/カラム並び替え(Issue #354)の実装計画を追加
- モバイル版タブ/カラム並び替えの操作方法を追記
- テストカバレッジ可視化の設計ドキュメントを追加
- テストカバレッジ可視化の実装計画を追加
- READMEにCodecovバッジを追加
- E2Eシナリオ網羅拡充の設計を追加
- E2Eシナリオ網羅拡充の実装計画を追加
- Issue #241デバッグログ・可観測性改善のspecを追加
- Specにフィルタ/ミュートdropログの節を追加
- SpecにAPIアクセスログのレスポンスサイズ記録を追記
- Specにログ統合を見送った理由を追記
- Specの欠落していたテスト節見出しを復元
- Issue #241の実装計画を追加
- 設定タブ分類見直し(Issue #326)の設計計画を追加
- 設定タブ分類見直し(Issue #326)の実装計画を追加
- 設定画面タブ構成の変更をユーザーガイドに反映
- タブ切替時スクロール修正(Issue #363)の設計を追加
- タブ切替時スクロール修正の設計をselectionMoveSeq方式に更新
- タブ切替時スクロール修正の実装計画を追加
- メディアビューワー移行(Issue #295)の設計ドキュメントを追加
- 画像ビューワーをCropper.js v2に一本化する設計に更新
- パン/スワイプ競合解決・Vidstackテーマ連携・実機確認手順を設計に追記
- メディアビューワー移行の実装計画を追加
- 実機フィードバックに基づきTask 11(グリッドVidstack化)を計画に追加
- 実装結果に合わせて設計ドキュメントを更新
- 音声波形表示とビューワーのcrossorigin方針を設計ドキュメントに追記
- モバイルUI E2Eの設計ドキュメントを追加する
- モバイルUI E2Eの実装計画を追加する
- モバイルE2Eの実行方法を追記する
- モバイルE2Eセーフエリア検証拡充の設計ドキュメントを追加する
- モバイルE2Eセーフエリア検証拡充の実装計画を追加する
- モバイルE2Eのセーフエリア検証範囲の記述を更新する
- セーフエリアE2Eの説明を実装に合わせて修正する
- 最終レビュー指摘(設計書の記述・compose注記・teardownガード)を反映する
- Overflow検査のカラム内容対応の設計ドキュメントを追加する
- Overflow検査のカラム内容対応の実装計画を追加する
- モバイルE2Eのoverflow検査範囲の記述を更新する
- 非メディア添付ファイルのリスト表示の設計を追加する
- 非メディア添付ファイルのリスト表示の実装計画を追加する
- Spec のsize型を実装に合わせる
- 強制猫化/強制人間化のdesign specを追加 (#42)
- 強制猫化/強制人間化の実装計画を追加 (#42)

### 🧪 Testing

- ReplyUserId フィールド追加に伴うNoteフィクスチャ修正
- MiAuthアカウント選択画面通過処理を共通ヘルパーに抽出
- StartMiauthBridgeに任意の認証情報を渡せるようにする
- MisskeyApiヘルパーにsignUp/renoteNote/deleteNoteを追加
- E2E新規シナリオ向けにdata-testid/data-account-idを追加
- Run-app.shにHOME再利用による再起動サポートを追加
- TQLフィルタカラムのE2Eシナリオを追加
- 複数アカウント同時使用のE2Eシナリオを追加
- 最終レビュー指摘事項を修正
- 音声アイテムがaudio-layoutで表示されることを確認するテストを追加
- モバイルE2Eの基盤(helper・config・script)を追加する
- モバイルレイアウト(FAB・投稿モーダル・カラム幅)のE2Eを追加する
- モバイルのセーフエリア被りを検出するE2Eを追加する
- セーフエリアE2Eのモーダル上端・右inset検証の検出力を修正する
- モバイルの意図しない横はみ出しを検出するE2Eを追加する
- モバイルのカラム領域上端と左insetのセーフエリアE2Eを追加する
- メディアビューワーのセーフエリアE2Eを追加する
- モバイルのカラム内容による横はみ出しを検出するE2Eを追加する
- Overflow-columnのマーカーノートが測定・非折りたたみであることを検証する
- MediaGridのメディア/非メディア振り分けと閲覧注意の開示を検証する

### ⚙️ Miscellaneous Tasks

- Rust側のテストカバレッジ計測(cargo-llvm-cov)を追加
- フロントエンド側のテストカバレッジ計測(Vitest v8)を追加
- Codecovへのカバレッジアップロードを統合

### ◀️ Revert

- タブバー高さ・グリップ幅・タブpaddingを元のサイズに戻す
## [0.11.0] - 2026-09-12

### 🚀 Features

- AndroidのVibrationEffect実装とVIBRATE権限を追加(Issue #26)
- Vibrateコマンドを追加してTSバインディングを生成(Issue #26)
- フロントにvibrate()ラッパーを追加(Issue #26)
- ノート投稿成功時にハプティクスを発火する(Issue #26)
- 設定画面にハプティクスON/OFFトグルを追加(Issue #26)
- スワイプでの次/前タブ・カラム解決ロジックを追加
- スワイプの軸判定・確定判定・ラバーバンド計算を追加
- AppStoreにスワイプ確定メソッドapplySwipeを追加
- モバイル版でカラム本体の左右スワイプによるタブ/カラム移動を実装
- タブScroll Snap用のスロット解決ロジックを追加
- Scroll Snapの着地スロット解決ロジックを追加
- AppStoreにfocusColumnメソッドを追加
- モバイル版のタブ横スワイプをCSS Scroll Snapで実装
- モバイル版のカラム横スワイプをCSS Scroll Snapで実装
- Modal.svelteにmaxHeight propを追加

### 🐛 Bug Fixes

- 投稿処理中の自動下書き再保存で投稿済みでも下書きが残留する不具合を修正
- Github-actionsのcooldownからsemver-major-daysを削除
- リアクションユーザー一覧ポップオーバーがModalの背後に隠れる問題を修正
- スワイプのスクロール位置ジャンプとキャンセル時のロックアウトを修正
- 最終レビュー指摘(カラム移動スクロール実装/方向反転バグ/スコープ限定修正)
- モバイルタブスロットのactiveTabId孤立時フォールバックを修正
- カラムScroll Snapの対象要素を実際にスクロールするPane.svelte側へ修正
- 最終レビュー指摘(プレビュースロットのselected漏れ/ネストした分割のpage-order食い違い/ユーザーガイド追記/チェックリスト拡充)
- モバイル版のカラム/タブスクロールバーを非表示化
- Nightly Release成果物のファイル名がリリース版バージョン番号のままになる問題を修正
- Viewer.jsの閉じるボタン/ツールバーがセーフエリアを無視する問題を修正

### 🚜 Refactor

- Pointer-drag方式のスワイプ実装を削除(Scroll Snap方式への移行準備)
- ColumnSettings.svelteを共有Modal.svelteに統一
- DrivePicker.svelteを共有Modal.svelteに統一

### 📚 Documentation

- PRテンプレートを追加
- PRテンプレートがgh pr createのbody指定時は使われない旨を明記
- ユーザーガイドにハプティクス設定の記載を追加
- モバイル版スワイプでのタブ/カラム移動の設計を追加(Issue #296)
- モバイル版スワイプ実装の計画を追加(Issue #296)
- 実機検証結果を踏まえCSS Scroll Snap版の設計に刷新(Issue #296)
- CSS Scroll Snap版の実装計画を追加(Issue #296)
- Nightly Release notes自動生成の設計を追加(Issue #333)
- Nightly Release notes自動生成の実装計画を追加(Issue #333)
- DrivePicker/ColumnSettingsのModal統一 設計ドキュメント追加(Issue #196)
- DrivePicker/ColumnSettings Modal統一 実装計画を追加(Issue #196)

### 🧪 Testing

- ハプティクステストをファイルスコープmockごと専用ファイルへ分離

### ⚙️ Miscellaneous Tasks

- Dependabot version updates(npm/cargo/github-actions)を設定
- Dependabotにcooldown(default 7日/major 14日)を設定
- Nightly Releaseの本文を専用ジョブでauto-generateする(Issue #333)
- Nightly Release作成の冪等性を上げコメントを補足する
## [0.10.0] - 2026-08-29

### 🚀 Features

- ＋カラム・設定ボタンを下部バー左端のハンバーガーメニューへ移設
- 投稿バーにリアクション受け入れ設定を追加
- Nyaize文字対応表ユーティリティを追加
- Nyaizeコピーイベントハンドラを追加
- Nyaize済みテキストに元テキストをdata属性で保持
- ノートカードにnyaizeコピーハンドラを接続
- MFM検索構文の検索エンジンを設定で切り替え可能にする
- ColumnGapFillイベントにギャップ打ち切り情報を追加
- TabViewにgapMarkerを追加しギャップ埋め打ち切りを検知する
- FillRemainingGapでギャップ埋め打ち切り分を手動取得できるようにする
- タイムラインにギャップ埋め打ち切りの区切り線とボタンを表示する
- デバッグビルド限定のdevtools用Unixソケットブリッジを追加
- Windows名前付きパイプでもデバッグブリッジを使えるようにする
- 自分の投稿をその他メニューから削除できるようにする
- カラム単位のbackfill境界(column_fetch_boundary)を追加
- カラムキャッシュのuntil_id指定取得(load_cached_before)を追加
- Backfillのキャッシュ応答可否判定(cache_backfill_page)を追加
- Fetch_backfillでbackfill境界内はキャッシュから応答する
- 通知音プリセットのWAVアセットを追加
- 通知音choiceをバイト列に解決するresolve_audio_bytesを追加
- 通知音をネイティブ再生するSoundPlayerを追加
- Play_notify_soundコマンドを追加してTSバインディングを再生成
- 通知音の再生をplay_notify_soundコマンド呼び出しに置き換え
- メンションアバターのセッション内キャッシュモジュールを追加
- 本文中のメンションにアバターアイコンを表示する
- UrlPreview型とUiPrefsのリンクプレビュー設定を追加
- Summalyプロキシからのリンクプレビュー取得・正規化を追加
- Fetch_url_previewコマンドを追加してTSバインディングを生成
- 本文からリンクプレビュー対象URLを抽出するユーティリティを追加
- リンクプレビューのセッションキャッシュを追加
- UrlPreviewCardコンポーネントを追加
- ノートにリンクプレビューを表示し設定UIを追加
- カラムヘッダーのボタンをメニューに集約
- User.instanceにInstance Ticker用インスタンス情報を追加
- Account.instanceと/api/meta取得を追加
- Refresh_instance_metaコマンドを追加
- UiPrefs.instanceTicker設定を追加
- 背景色から可読な文字色を選ぶreadableTextColorを追加
- Boot時に全アカウントのインスタンス情報を取得
- 外観設定にInstance Tickerの表示モードを追加
- NoteCardにInstance Tickerを表示
- Instance Tickerをカラム幅いっぱいのグラデーション表示に変更
- Instance Tickerのアイコン未設定時にホストのfaviconへフォールバック
- ThemeColor未設定時に本家準拠の既定グレー(#777777)へフォールバック
- 不正なthemeColor時のフォールバックもグラデーション表示に統一
- ノートメニューに内容コピー項目を追加

### 🐛 Bug Fixes

- カスタム対話要素にキーボードフォーカスの視覚表示を追加
- AppMenuの高さをBackstageバーに揃えて余白を解消
- AppMenuをログ展開時も下部バーの下端に固定
- AppMenuボタンをoutline variantにしてログボタンと揃える
- ギャップ埋め設定をデータセクションへ移動しデバッグ設定を区別
- データセクションにh3見出しを追加
- リアクション受け入れ選択肢の文言をMisskey本家に揃える
- リアクション受け入れボタンにアプリ内共通のリアクションアイコンを追加
- Nyaizeコピーの安全対策(nyaize対象外選択のフォールバック/絵文字・改行欠落/DP性能上限)を追加
- SeedスクリプトのESM対応とtypescriptバージョン固定
- MiAuthブリッジのセッション確立をlocalStorage注入+永続コンテキストへ修正
- E2E実行時のブラウザ起動先とmisskey.local名前解決を修正
- Node側fetch()のmisskey.local名前解決とCA証明書読み込みを修正
- @wdio/tauri-serviceのプラグイン未検出ポーリングをパッチで抑制
- ReqwestのTLSルート証明書をOS信頼ストア参照に変更
- E2E実行時にtsumugiのTLS検証がテストCAを信頼するよう修正
- Gnome-keyring-daemon起動とmisskey.local名前解決をLD_PRELOAD方式に統合
- Add-column-submitボタンのクリック不能問題とE2Eプロセス残留を修正
- Tokio-tungsteniteのTLSルート証明書をOS信頼ストア参照に変更
- ウィンドウレイアウト安定待ちとノート表示セレクタの曖昧さを解消
- Android向けTLSルート証明書解決失敗に備えwebpki-rootsを併用
- MiAuthブリッジのブラウザlocaleをja-JPに固定
- MFMの検索構文を検索ボックスUIとして描画する
- 最終レビュー指摘(収束条件/エラー表示/枯渇誤検知/競合)を修正
- コードブロックの横スクロールをpre自身に持たせ、スクロールバーへのドラッグ不可を軽減する
- 言語未指定のコードブロックをshikiのtext特殊言語でテーマ適用する
- 削除確認ダイアログがメニューの背景に隠れてクリックできない不具合を修正
- Clear_column_notesでbackfill境界も削除する
- Pruneでbackfill境界を生存範囲まで引き上げる
- Pruneのboundary取得でDBエラーを握りつぶさないようにする
- Backfillキャッシュ提供ページに境界下限とミュート再適用を追加
- キャッシュ取得とprune境界引き上げをid基準に揃える
- 非連続な範囲での境界延長を防ぎミュート変更時に境界を破棄する
- LoadMoreのMAX_NOTES上限到達後にbackfillが遡れなくなる不具合を修正
- LoadMoreのbackfill結果をMAX_NOTESで切り捨てないようにする
- 通知音デコードのパニック捕捉と出力デバイス再オープン処理を追加
- PlayNotifySound呼び出しにcatchを付けて共通ヘルパーへ集約
- CIにlibasound2-devを追加してrodio依存のalsa-sysビルドを通す
- Rodio(Oboe)のAAudio要件に合わせてAndroid minSdkを26に引き上げ
- MinSdkVersionはtauri.conf.json側で指定しないとcargo tauri android buildに反映されないため追加
- メンションアバターのキャッシュ設計をレビュー指摘に対応
- CachedAvatarUrlのアカウント未設定時の戻り値をコメントに合わせる
- メンションアバターをチップ化してテキストとの対応を分かりやすくする
- メンションチップの背景を単色からアクセント薄塗りに変更
- メンションのホスト部を太字にせずユーザー名のみ強調する
- リンクプレビューの危険なURLスキームを拒否する
- 最終レビュー指摘(空プレビューのキャッシュ・サムネイルのプロキシ化・URL抽出の再計算)を修正
- リンクプレビューカードの幅と高さに上限を設ける
- リンクプレビューカードを横長レイアウトに変更
- プレイヤー展開時のアスペクト比を実際のサイズから算出する
- Player.widthが無くheightのみの埋め込み(Spotify等)は固定高さを使う
- プレイヤー展開時の高さに常時200pxの下限を設ける
- ユーザー検索の重複結果を去重する
- メニューの画面外はみ出しとガイド記述のずれを修正
- メニューをタブスクロール領域から切り離しカラム右端に固定
- InstanceTicker themeColorの検証を追加してCSS注入を防止
- IsValidHexColorをreadableTextColorと同じ6桁形式に統一
- Refresh_instance_metaで実際に使われている関数のdead_code属性を削除
- 最終レビュー指摘のコメント修正・テスト堅牢化・カバレッジ追加
- スマホUIでカラムタブバーがステータスバーに被る問題を修正
- レビュー指摘を反映（ユーザーガイド更新・クリップボードエラー処理など）

### 🚜 Refactor

- 角丸をスタイルガイドのスケールに統一
- フォントサイズをスタイルガイドのスケールに統一
- アイコンサイズをスタイルガイドのスケールに統一
- 設定のDisplaySectionをテーマ・背景セクションに分割
- 表示をレイアウトに改名し絵文字・フォント・MFMアニメをテーマへ再分類
- テーマセクションを外観に改名
- Fill_gapがnewest_known_idへの到達可否を返すようにする

### 📚 Documentation

- UIスタイルガイドを追加
- スタイルガイドにタイポグラフィ・アイコン・フォーカス指針を追加
- リアクション受け入れ設定の設計書を追加
- リアクション受け入れ設定の実装計画を追加
- CLAUDE.mdにスタイルガイドへの参照を追記
- Nyaize前文字列コピー(#168)の設計spec
- Nyaize前文字列コピー(#168)の実装計画書
- Issue #132 E2E自動化設計スペックを追加
- Issue #132 CA証明書登録の冗長化対応を追記
- Issue #132 E2E自動化の実装計画を追加
- E2Eディレクトリの実行手順READMEを追加
- E2Eテストとビルドのハング回避策をCLAUDE.mdに追記
- Issue #148起動時ギャップ埋め打ち切り分の続き取得設計を追加
- Issue #148 ギャップ埋め続き取得の実装計画を追加
- Issue #232 デバッグ用devtoolsブリッジの設計を追記
- CLAUDE.mdにdebug_bridgeモジュールの説明を追加
- Issue #234 投稿削除機能の設計を追記
- Issue #234 投稿削除機能の実装計画を追加
- Issue #228 backfillキャッシュ優先化の設計を追加
- Issue #12 通知音のRustネイティブ再生移行の設計を追加
- Issue #12 通知音Rustネイティブ再生の実装計画を追加
- 設計doc内の実在しないテスト移行の記述を修正
- メンションアバターアイコン表示の設計docを追加（Issue #102）
- メンションアバターアイコン実装計画を追加（Issue #102）
- OGP/リンクプレビュー対応の設計を追加(Issue #9)
- カスタムsummalyプロキシURL設定を設計に追加(Issue #9)
- OGP/リンクプレビュー実装計画を追加(Issue #9)
- カラムヘッダーミートボールメニュー化の設計を追加
- カラムヘッダーメニュー化の実装計画を追加
- Instance Ticker実装計画を追加
- Instance Ticker実装計画を追加
- Task 5テストのWCAG輝度計算ミスを修正
- ノート本文コピー機能の設計spec追加
- メニュー項目名を「内容をコピー」に修正
- ノート本文コピー機能の実装計画追加

### 🧪 Testing

- Tauri-driverスパイクでE2E基盤の疎通を確認
- E2E用CA証明書の生成・登録スクリプトを追加
- E2E用Docker Compose Misskeyスタックを追加
- E2Eテスト管理者アカウントのseedスクリプトを追加
- E2E実行時にアプリを本番環境から分離するラッパーを追加
- MiAuth同意画面のPlaywright CDPブリッジを追加
- E2Eセレクタ用のdata-testid属性を追加
- アカウント追加→投稿→リアクションのE2Eシナリオを実装
- E2Eのノートスコープ・Xvfb起動失敗検知・seed冪等時のファイル書き込みを修正
- MiAuthフロー失敗調査用の診断ログ・スクリーンショットを追加
- 投稿削除の確認ダイアログフローをテストする
- 空accountIdでのfetchUrlPreview挙動を検証するテストを追加

### ⚙️ Miscellaneous Tasks

- E2EテストジョブをGitHub Actionsに追加
- E2E失敗アーティファクトのSHAピン誤りとログ出力先未設定を修正
- Docs-only変更でrust-test/frontend-check/android-build/e2eの実処理をスキップ
- E2eも必須ステータスチェックとしてdocs-onlyスキップ方式に統一
- TSバインディング再生成とplanドキュメントを追加
## [0.9.0] - 2026-08-13

### 🚀 Features

- 通知カラムの再接続ギャップ埋め用ウォーターマークを追加
- 通知受信のたびに再接続ギャップ埋め用ウォーターマークを更新
- 通知の再接続ギャップ埋めイベントColumnNotificationGapFillを追加
- ノート系カラムのStream再接続時ギャップ埋めを追加
- 通知カラムのStream再接続時ギャップ埋めを追加
- 通知カラムの再接続ギャップ埋め結果をフロントへ反映
- UiPrefsに絵文字使用履歴フィールドを追加
- 絵文字使用履歴リスト更新の純粋関数を追加
- Storeに絵文字使用履歴の記録メソッドを追加
- ノートへのリアクションで絵文字使用履歴を記録
- 絵文字ピッカーに最近使った絵文字セクションを追加
- NoteCardにshowActions propを追加してquotedからアクション表示を分離
- 通知カードのノートに返信/Renote/リアクションボタンを表示
- 時間帯別プレースホルダーのロジックを追加
- 投稿欄プレースホルダーを時間帯に応じて表示
- リアクションキーを投稿欄挿入テキストに変換するヘルパーを追加
- 投稿欄に絵文字ピッカーを追加
- TQL補完エンジンを追加
- Tql_completeコマンドを追加しTSバインディングを再生成
- TQL補完のフロント側ロジックを追加
- App.tqlCompleteラッパーを追加
- TqlCompletionFieldコンポーネントを追加
- AddColumnModalのTQL入力欄に補完を組み込む
- NoteDraftにchannel_idを追加してチャンネル投稿を可能にする
- コンポーズバーにチャンネル選択UIを追加
- Userドメイン型にbio/banner_urlを追加
- RawUserのdescription/bannerUrlをUser.bio/banner_urlへ正規化
- Users/show, following/create, following/deleteのAPIラッパーを追加
- Users/followers, users/followingのAPIラッパーを追加
- Get_user_profileコマンドを追加
- Follow_user/unfollow_user/get_user_notesコマンドを追加
- Get_user_followers/get_user_followingコマンドを追加
- ユーザープロフィール系コマンドをspecta_builderに登録
- プロフィールモーダルのグローバル開閉状態を追加
- AppStoreにプロフィール取得・フォロー操作メソッドを追加
- ProfileModalコンポーネントを追加
- FollowListModalコンポーネントを追加
- ProfileModalにFollowListModalを配線
- ノートのアバター/名前/mentionクリックでプロフィールモーダルを開けるようにする
- フォロー一覧・ノート一覧を既存カラムと同じスクロール自動取得に変更
- 通知欄のアバター/表示名クリックでもプロフィールモーダルを開けるようにする
- Shadcn-svelteのButtonプリミティブを追加
- AccountSelect.svelteをTailwindクラスに移行
- App.svelteをTailwindクラスに移行
- MediaGrid.svelteをTailwindクラスに移行
- DrivePicker.svelteをTailwindクラスに移行
- AddAccount.svelteをTailwindクラスに移行
- ColumnSettings.svelteをTailwindクラスに移行
- 投稿欄下のエラーバナーを廃止しエラーモーダルに置き換え

### 🐛 Bug Fixes

- アバター・カスタム絵文字をhome instanceのメディアプロキシ経由で取得する
- 引用Renoteの本文にemojiAccountIdを伝播する
- 自インスタンスのカスタム絵文字リアクションキーの表記を正規化する
- ノートキャッシュの旧形式emojisペイロードをスキップして読み込み継続する
- 再接続ギャップ埋めの多重実行を防止
- 通知の再接続ギャップ埋めと実配信の競合による通知欠落を修正
- 通知の初期REST取得結果でギャップ埋めウォーターマークを初期化
- リアクション解除時に絵文字使用履歴を記録しないよう修正
- 通知カードのRenote/返信バナーが重複表示されるのを抑制
- 引用RN内でネストされた引用元ノートのリアクションを非表示に
- 通知専用ノートへのリアクション/お気に入り/投票がサイレントに失敗する不具合を修正
- 絵文字挿入位置とEscapeキーの挙動を修正
- 絵文字ボタンをテキストエリア右上に配置しEscape・収縮の不具合を修正
- 絵文字ボタン押下時にテキストエリアのフォーカスを維持
- 絵文字ピッカーがウィンドウ外にはみ出さないよう位置をクランプ
- TQL補完ポップアップで未選択時のEnterは確定しないよう修正
- 最終レビュー指摘を修正(list/antenna/channel補完・ドット区切りフィールド補完・不要CSS)
- TQL補完欄が未フォーカスでもポップアップが出てしまう不具合を修正
- TQL補完欄はフォーカス直後ではなく入力を始めてから候補を出すよう修正
- TQL補完欄を全消しした際に補完が居座らないよう空欄判定に一本化
- チャンネル選択の未フォロー表示・未選択投稿・アカウント切替時の不整合を修正
- チャンネル選択UIの表示調整(公開範囲のグレーアウト・連合なし固定表示・チャンネルドロップダウンの位置とサイズ)
- プロフィール実装計画をレビュー指摘に基づき修正
- 実装計画のinvokeMockモックが二重ラップになっていた誤りを修正
- Task13にProfileModalへのFollowListModal配線手順を追加
- FollowListModalがprops変化時に再ロードしない不具合を修正
- フォロー一覧のページングカーソル・ノート追加取得のエラー処理・プロフィール絵文字表示を修正
- フォロー一覧の表示名の文字色・カスタム絵文字表示・アバター角丸を修正
- リモートユーザーの負のカウント値でのデシリアライズ失敗と、プロフィール系操作のエラー二重表示を修正
- プロフィール系操作のエラーをバナー表示なしでBackstageに記録するよう変更
- エラー中の無限スクロールで再試行が連続発火するのを防ぐ
- @mentionクリックでのユーザー解決失敗時にバナーとモーダル内エラーが二重表示されるのを修正
- Shadcn-svelte CLIが上書きした既存テーマ変数とpremature CSSを復元
- 最終レビュー指摘を反映(Preflight除外/auto時dark:バリアント対応/tsconfig警告解消)
- 最終レビュー指摘を反映(タブ閉じるボタン/接続状態ドットのクラス衝突を解消)
- Preflight除外環境下でbuttonの素の見た目を最小限リセット
- FollowListModal.svelteのスペーシングを元CSSに合わせて修正
- スクロールテストのセレクタをスタイル用クラスからdata-testidに分離
- Preflight除外環境でのUAデフォルトmargin復活を修正(FollowListModal/ProfileModal)
- MFMアニメーションを設定でOFFにできるようにする(Issue #175)
- ConfirmDialogのメッセージ上マージンを明示してUAデフォルト復活を防ぐ
- TQL補完ポップアップがModalの下に隠れる問題を修正
- ノートフッターのボタン高さをアイコン基準に揃える
- ノートフッターの各ボタンに固定高さを与えて確実に揃える
- リアクション/その他ボタンのラッパーdivも固定高さのflexにする
- TQL補完確定後に再検証が走らない不具合を修正
- Modal.svelteのwidth未生成バグとSettings.svelteのレイアウト微調整を修正
- カスタムテーマ編集フォームの保存ボタンをsmサイズにしてキャンセルとの高さ差を縮める
- キャンセルボタンもsmサイズにして保存ボタンとの高さ差を解消
- 試聴/音声選択/新規作成/画像選択ボタンもsmサイズに統一
- 絵文字ボタンのサイズ/hover挙動とComposeBarアイコンサイズの微調整
- Bind:refの初期値をundefinedからnullに修正しComposeBarのマウント失敗を解消
- 投稿ボタンをsmサイズにしてツールバーの高さ差を縮める
- ツールバー系ボタンをsm/icon-smサイズに統一し窮屈さと投稿ボタンとの差を解消
- AccountSelectのlargeアバターサイズをButtonのコンテンツ領域に収める
- DrivePickerの選択枠にoutlineユーティリティを明示追加
- AddAccountのボタン高さとホバー色をadvisor指摘に沿って修正
- エラーモーダル表示中はグローバルキーバインドを無効化

### 💼 Other

- Tailwind CSS v4をViteに統合
- Shadcn-svelteを初期化

### 🚜 Refactor

- Acct/displayNameヘルパーをlib/userDisplay.tsへ共通化
- テーマ切替をdata-theme属性から.dark/.lightクラスに変更
- エラー通知をバナーからモーダル/ログ振り分けに変更

### 📚 Documentation

- CLAUDE.mdにリリース手順を追記
- Android buildSrc設定失敗時のJDKバージョン既知問題を追記
- Stream再接続時ギャップ埋め設計spec追加(Issue #147)
- Stream再接続時ギャップ埋め実装計画追加(Issue #147)
- 絵文字ピッカー使用履歴(Issue #108)の設計を追加
- 絵文字ピッカー使用履歴(Issue #108)の実装計画を追加
- Issue #50 通知ノートアクション有効化の設計spec追加
- Issue #50 通知ノートアクション有効化の実装計画追加
- Issue #16 ユーザーガイド追加の設計書追加
- Issue #16 実装計画追加
- 設計書をdocs/design/へ再編成
- Issue #16 ユーザーガイドを追加
- 最終レビュー指摘の修正(リンク切れ・古いパス参照)
- 投稿バーの位置に関する記述を修正(画面上部)
- READMEにバッジ・特徴・OS別ダウンロードリンクを追加
- READMEの構成を利用者向け/開発者向けに整理、リリース手順をdocs/design/へ分離
- ユーザーガイドにダウンロードセクションへの導線を追加
- ダウンロードセクションのテーブルをOS横並びに整理
- 冒頭の紹介文をシンプルにし、Krileへの言及を特徴セクションへ移動
- READMEにネタバッジを追加
- 投稿欄プレースホルダー時間帯変更の設計書を追加
- プレースホルダー文言を各区分7パターンに拡充
- 投稿欄プレースホルダー時間帯変更の実装計画を追加
- 投稿欄絵文字ピッカー追加の設計を追加
- 投稿欄絵文字ピッカー追加の実装計画を追加
- TQL入力補完の実装計画を追加
- TQL入力補完の実装計画(タスク分解)を追加
- Issue #95 チャンネル投稿機能の設計を追加
- Issue #95 チャンネル投稿機能の実装計画を追加
- ユーザープロフィールページの設計spec追加 (Issue #91)
- ユーザープロフィールページの実装計画を追加
- Tailwind + shadcn-svelte導入の設計を追加(Issue #170)
- テーマ変数移行方針を修正、Rust構造体は不変に(Issue #170)
- Tailwind + shadcn-svelte基盤導入の実装計画を追加(Issue #170)
- レイアウト系コンポーネントのTailwind移行設計を追加(Issue #174)
- レイアウト系コンポーネントのTailwind移行実装計画を追加(Issue #174)
- モーダル基盤コンポーネントのTailwind移行設計を追加(Issue #174)
- Dialogプリミティブ化を見送る理由を明記(Issue #174)
- モーダル基盤コンポーネントのTailwind移行実装計画を追加(Issue #174)
- Destructiveバリアントの実際の配色に合わせてspec記述を修正(Issue #174)
- ProfileModal/FollowListModalのTailwind移行設計を追加(Issue #174)
- ProfileModal/FollowListModalのTailwind移行実装計画を追加(Issue #174)
- DMABUF無効化時のフォールバック経路の説明を訂正
- AddColumnModal Tailwind移行のspec追加(Issue #174 第4バッチ)
- AddColumnModal Tailwind移行の実装計画を追加(Issue #174 第4バッチ)
- NoteCard周辺Tailwind移行のspec追加(Issue #174 第5バッチ)
- NoteCard周辺Tailwind移行の実装計画を追加(Issue #174 第5バッチ)
- 入力系ウィジェットTailwind移行のspec追加(Issue #174 第6バッチ)
- 入力系ウィジェットTailwind移行の実装計画を追加(Issue #174 第6バッチ)
- 設定画面Tailwind移行のspec追加(Issue #174 第7バッチ)
- Settings.svelteシェル本体もspec対象に追加
- 設定画面Tailwind移行の実装計画を追加(Issue #174 第7バッチ)
- ComposeBar.svelte Tailwind移行のspec追加(Issue #174 継続バッチ)
- ComposeBar.svelte Tailwind移行の実装計画を追加(Issue #174 継続バッチ)
- AccountSelect Tailwind移行のdesign specを追加
- App.svelte Tailwind移行のdesign specを追加
- App.svelte migration specに意図的な見た目差分を明記
- MediaGrid Tailwind移行のdesign specを追加
- DrivePicker Tailwind移行のdesign specを追加
- AddAccount Tailwind移行のdesign specを追加
- ColumnSettings Tailwind移行のdesign specを追加
- エラーバナー→モーダル移行のdesign specを追加
- エラーバナー→モーダル移行の実装計画を追加

### 🎨 Styling

- ProfileModal/FollowListModalを既存UIのデザイン言語に合わせて調整
- フォロー一覧を円形アバター+行区切り線で見やすく調整
- Shadcn標準トークンを既存CSS変数にブリッジする@themeを追加
- Pane.svelteをTailwindクラスに移行
- Backstage.svelteをTailwindクラス+Buttonプリミティブに移行
- Column.svelteをTailwindクラス+Buttonプリミティブに移行
- Modal.svelteをTailwindクラス+Buttonプリミティブに移行
- ConfirmDialog.svelteをTailwindクラス+Buttonプリミティブに移行
- FollowListModal.svelteをTailwindクラス+Buttonプリミティブに移行
- ProfileModal.svelteをTailwindクラス+Buttonプリミティブに移行
- AddColumnModal.svelteを共通Modal+Tailwindクラスに移行
- NoteCard.svelteをTailwindクラスに移行
- NotificationCard.svelteをTailwindクラスに移行
- ReactionPicker.svelteをTailwindクラスに移行
- NoteMenu.svelteをTailwindクラスに移行
- ReactionUsersPopover.svelteをTailwindクラスに移行、z-indexをModal.svelteより前面に引き上げ
- TqlCompletionField.svelteをTailwindクラスに移行
- CompletionPopover.svelteをTailwindクラスに移行
- Sparkle.svelteをTailwindクラスに移行
- Modal.svelteにwidth propを追加しSettings.svelteをTailwind移行
- AboutSection.svelteをTailwindクラスに移行
- AccountsSection.svelteをTailwindクラスに移行
- DataSection.svelteをTailwindクラスに移行
- KeysSection.svelteをTailwindクラスに移行
- MuteSection.svelteをTailwindクラスに移行
- NotifySection.svelteをTailwindクラスに移行
- ReactionSection.svelteをTailwindクラスに移行
- DisplaySection.svelteをTailwindクラスに移行
- 設定画面のボタンをButtonプリミティブに統一
- ComposeBar.svelteをTailwindクラス+Buttonプリミティブに移行
- VisibilitySelect.svelteをTailwind移行しButtonプリミティブに統一
- Dropdown.svelteをTailwind移行しButtonプリミティブに統一

### 🧪 Testing

- ComposePlaceholder.test.tsの型エラーを修正
- NoteCardのプロフィール導線テストをimg/name/acctにも拡充
- エラーモーダル対象/非対象操作の回帰テストを追加

### ⚙️ Miscellaneous Tasks

- SDD作業用ディレクトリをgitignoreに追加
- Worktreeスクラッチディレクトリをgitignoreに追加
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

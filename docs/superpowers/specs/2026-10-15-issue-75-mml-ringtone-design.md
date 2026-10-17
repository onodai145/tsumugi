# 着メロ打ち込み(MML)機能の設計

Issue #75(関連: #12 / `2026-08-25-issue-12-native-notify-sound-design.md`)

## 目的

通知音を、既製の音声ファイルを選ぶだけでなく、アプリ内で MML(Music Macro Language)のテキストとして打ち込んで作れるようにする。Issue 本文に具体的な要件はなく、「着メロ打ち込み機能をつけよう」という趣旨のみ。優先度は低いので、最小構成から始める。

成功条件:

- 設定画面の「通知音」と、カラムごとの通知音の両方で、MML を入力して試聴・保存できる。
- 保存した MML は通知時に鳴り、再起動後も同じ音で鳴る。後から MML を編集し直せる。
- 構文エラーは位置と理由付きで表示し、不正な MML は保存できない。
- MML の解釈や合成に失敗しても、通知は止めず `beep` で代替する。
- 音を出すのは Rust(rodio)のまま。フロントは音を再生せず、波形データを作るだけにする。

## 決定事項

| 決定 | 内容 | 理由 |
|---|---|---|
| 入力方式 | MML テキスト | UI が小さい。内部表現をノート列にするので、ピアノロールは後から足せる |
| MML の解釈 | Rust(純粋関数) | Model 層は Rust。`cargo test` で網羅できる |
| 音の生成 | フロントで Tone.js(`Tone.Offline`)→ WAV | 音色(FM 音源など)の豊かさ。FM 音源は昔の着メロらしい音でもある |
| 再生 | Rust(rodio、既存の `play_notify_sound`) | #12 の `AudioContext` の自動再生問題を避ける |
| 保存形式 | `sound_choice` に `mml:<テキスト>` | 設定が小さく、MML の元データが残る。`NotifyConfig` と `ColumnDef` にフィールドを足さない |

Tone.js を使わず Rust で矩形波などを直接合成する案(C案)は、音色が素朴になるため見送った。ただし後述の Android 検証に失敗した場合は、C案に切り替える。

## 前提(コードの現状)

- `NotifyConfig.sound_choice`(グローバル)と `ColumnDef.notify_sound_choice`(カラムごと)は、プリセット ID か `data:` URL を取る文字列。空文字はプリセット `beep`、カラム側の空文字はグローバルを継承する。
- `play_notify_sound(choice)`(`commands/sound.rs`)は常に `Ok` を返し、失敗は `warn` ログだけ。`resolve_audio_bytes` は、`data:` 以外の未知の文字列を `beep` として扱う。
- フロントは、通知イベントの 2 か所(`store.svelte.ts` の `#resolveSoundChoice` を使う箇所)から `ipc.ts` の `playNotifySound(choice)` を呼ぶ。試聴ボタンも同じ関数を呼ぶ。
- 通知音の選択 UI は、`ui/settings/NotifySection.svelte` と `ui/AddColumnModal.svelte`(デスクトップ版・モバイル版の 2 か所)に、ほぼ同じ内容が重複している。
- 保存は `set_notify`(`commands/mute.rs`)と `set_column_notify`(`commands/column.rs`)が行い、中身の検証はしていない。

## 設計

### 1. `sound_choice` の形式

```
""             既定(beep)
"beep" 等      プリセット ID
"data:..."     カスタム音声ファイル(既存)
"mml:<text>"   MML(新規)。<text> は MML の生テキスト
```

### 2. MML の方言(v1)

大文字小文字と空白は無視する。

| 要素 | 記法 | 備考 |
|---|---|---|
| 音符 | `c d e f g a b` | 後ろに `+`/`#`(シャープ)、`-`(フラット) |
| 長さ | 音符・休符の直後に `1 2 4 8 16 32 64` | 付点 `.` は最大 2 個。省略時は `l` の値 |
| 休符 | `r` | 長さの指定は音符と同じ |
| タイ | `&` | `c4&c8` のように、同じ高さの音符を連結。異なる高さや休符との連結はエラー |
| オクターブ | `o<n>`、`>`(上げる)、`<`(下げる) | 既定は `o4`(`o4 c` が MIDI 60)、範囲は 1〜8 |
| 既定の長さ | `l<n>` | 既定は `l4`。`n` は上の長さの集合のみ |
| テンポ | `t<n>` | 既定は `t120`、範囲は 32〜255 |
| 音量 | `v<n>` | 0〜15、既定は `v10`。ベロシティは `n/15` |
| 繰り返し | `[ ... ]n` | ネストなし。`n` は 2〜99、省略時は 2。`;` を含められない |
| トラック区切り | `;` | 最大 4 トラック(和音)。全トラックで同じ音色 |

- 各トラックは独立した状態(テンポ・オクターブ・長さ・音量)を持ち、時刻 0 から始まる。テンポは `t` をトラックごとに書く必要がある(v1 の制約)。
- `>` で上げる、`<` で下げる、という向きは実装によって逆のものがあり、確証のないまま多数派の向きを採った。
- 上限: MML は 1000 文字以下、全トラックの再生時間は 10 秒以下。繰り返しはネストできないので、展開後の命令数は入力サイズ(1000 文字 × 最大 99 回)で抑えられる。時間の積算は音符・休符ごとに行い、10 秒を超えた最初の音でエラーにする。

  > 注記(2026-10-16): デモ用に再生時間の上限を 90 秒に広げたため、上限は 5000 文字・90 秒・3000 音(新設。全トラック合計、タイで連結した音は 1 つと数える)に変わった。90 秒の `data:` URL は約 5.3MB になるため、フロントのメモ化は 16 件から 8 件に減らした。

v1 に入れないもの: 音色切り替え(`@`)、エンベロープやビブラートなどの装飾、`n<番号>` 形式の音符指定、ループのネスト、トラックごとのテンポ共有、向きの反転設定。

### 3. Rust

- `src-tauri/src/mml.rs`(新規): 外部に依存しない純粋なパーサー `parse(text: &str) -> Result<MmlScore, MmlError>`。繰り返しの展開・タイの連結・テンポの解決をここで済ませ、時刻は秒で持つ。
- `src-tauri/src/domain/mml.rs`(新規、`specta::Type`):
  - `NoteEvent { track: u8, start_sec: f64, duration_sec: f64, midi: u8, velocity: f64 }`
  - `MmlScore { notes: Vec<NoteEvent>, total_sec: f64, track_count: u8 }`
  - `MmlError { position: u32, message: String }`。`position` は入力テキストの文字インデックス(0 始まり)。UI とエラーメッセージ(`validate_sound_choice` を含む)では 1 始まりの「N 文字目」として表示する。
  - `MmlParseResult`: `{ status: "parsed", score }` か `{ status: "invalid", error }` の列挙型(`#[serde(tag = "status")]`)。UI が位置を使うので、構文エラーはコマンドの `Err` ではなくデータとして返す。
- `commands/sound.rs` に `parse_mml(text: String) -> MmlParseResult` を追加し、`specta_builder()` に登録する。
- 保存時の検証: `set_notify` と `set_column_notify` は、`mml:` で始まる `sound_choice` をパースし、失敗なら `Error::Invalid` で保存を拒否する(UI の表示とは別に、保存の権威的な検証をここで持つ)。それ以外の形式は従来どおり検証しない。
- `resolve_audio_bytes`: `mml:` が届いた場合は、`beep` を返して `warn` ログを残す(フロントのレンダリング失敗時の最後の砦)。既存のシグネチャは変えない。

### 4. フロントエンド

- `frontend/package.json` に `tone` を追加する(バージョンは固定。spike では 15.1.22 を使った)。
- `lib/mmlRender.ts`(新規):
  - `renderScore(score): Promise<string>` は、`tone` を遅延 import し、`Tone.Offline` でノート列をレンダリングして WAV の `data:` URL を返す。
  - 音色は `PolySynth(FMSynth)`(spike と同じパラメータから開始)。サンプルレートは 22.05kHz、モノラル、16bit。長さは `total_sec` に余韻を足したもの。
  - WAV エンコーダは純粋関数として切り出す。
- `lib/soundChoice.ts`(新規): `resolveSoundChoice(choice): Promise<string>`。`mml:` 以外はそのまま返す。`mml:` なら `parse_mml` → `renderScore` の順に処理し、結果を `choice` 全体をキーにメモ化する(最大 16 件、古い順に破棄)。どの段階で失敗しても `"beep"` を返し、デバッグログに理由を残す。`prewarmSoundChoice(choice)` は、結果を捨てて同じ処理を先に走らせる。
- `ipc.ts` の `playNotifySound(choice, onError?): void` は、内部で `resolveSoundChoice` を待ってから `commands.playNotifySound` を呼ぶ。解決に失敗したら `onError` に理由を渡して `beep` を鳴らす。`onError` は省略可能なので、既存の呼び出しは互換。`store.svelte.ts` の通知時の呼び出しは、`onError` でデバッグログに理由を残す。
- 先読み: グローバル設定の読み込み時と `setNotify` の保存後に、`prewarmSoundChoice` を呼ぶ。カラム側の上書きは、初回の再生時に遅延レンダリングする。
- UI: 3 か所に重複している選択 UI を、`ui/settings/SoundChoicePicker.svelte` に切り出す(`NotifySection.svelte` と `AddColumnModal.svelte` の両方で使う。カラム側は「グローバルを継承」を選べるオプションを持つ)。ここに「MML」モードを足す。
  - 内容: テキストエリア(等幅)、構文エラーの表示(位置と理由)、試聴ボタン、再生時間の表示、記法の折りたたみヘルプ。
  - 入力中の検証は `parse_mml` を 300ms のデバウンスで呼ぶ。
  - ピッカーは、構文が通っていない間 `onvalidity` で親に通知して保存ボタンを止める。ピッカーが非表示のとき(通知音をオフにしたときなど)は、入力途中の未検証な下書きではなく、直前に保存済みの値を送る(`AddColumnModal` の `effectiveSoundChoice`、`NotifySection` も同様)。
  - モバイルでは、ソフトキーボードの `--keyboard-inset` の扱いを、既存のモーダルに合わせる。
  - 見た目は `docs/design/style-guide.md` に従い、一回限りの値は作らない。

### 5. エラー処理

- 保存時: 構文エラーや上限超過は、位置と理由を表示して保存できないようにする。
- 再生時: `parse_mml` の失敗、Tone.js の読み込み失敗、レンダリング失敗は、すべて `beep` で代替する。保存済みの MML が、将来のパーサー変更で通らなくなった場合もここに入る。通知は止めない。理由はフロントのデバッグログと Rust の `warn` ログに残す。
- 初回の再生が遅れる場合は、先読みで隠す(spike では、1.6 秒の音のレンダリングが初回 58ms、2 回目 12ms だった)。

## テスト

- Rust(単体): パーサーを網羅する。音符・長さ・付点・タイ・オクターブ・テンポ・音量・繰り返し・マルチトラック・上限(文字数、時間、繰り返しを含む場合の時間上限での打ち切り)・エラー位置。`resolve_audio_bytes("mml:...")` が `beep` を返すこと。保存時の検証は `validate_sound_choice` の単体テストで確認する。`set_notify` / `set_column_notify` そのもののコマンドレベルのテストは書かない(`State<AppState>` を取るため構築が重い)。この逸脱は記録済みで、コントローラが承認した。コマンド経由の拒否は手動確認で補う。`generates_frontend_bindings` で新しい型の出力を確認する。
- フロント(Vitest): jsdom に Web Audio はないので、`tone` はモックする。`resolveSoundChoice` の分岐(通常の値の素通し、メモ化、各段階の失敗時のフォールバック、件数上限)と、WAV エンコーダ(ヘッダ・長さ・クリップ)を確認する。
- 実機(手動):
  - Xvfb と隔離したインスタンスで、MML の入力・試聴・保存・通知時の再生を確認する(WAYLAND_DISPLAY を unset し、`dbus-run-session` で隔離する)。
  - レビューが通っただけでは、動作確認の代わりにならない。
  - Android は可能な範囲で確認する。

## 実装前の検証ゲート

`Tone.Offline` が Android WebView で動くかは未検証。実装計画の最初のタスクとして、Android(エミュレーター)で最小の `Tone.Offline` が動き、WAV が得られることを確認する。動かない場合は、この設計を C案(Rust で直接合成)に切り替える。その場合、パーサーと `MmlScore` はそのまま使える。

検証結果(2026-10-15): 合格。Android 14 エミュレーター(sdk_gphone64_x86_64)の Chrome 113 で、Tone.js 15.1.22 の `Tone.Offline`(FMSynth、0.5 秒、22.05kHz)が `renderMs` 168、`peak` 0.316 で完了し、エラーは出なかった。`Tone.getContext().state` は `suspended`。UA は `Mozilla/5.0 (Linux; Android 14; sdk_gphone64_x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/113.0.0.0 Mobile Safari/537.36`。実 WebView での最終確認は Task 8。

## spike の結果(参考)

WebKitGTK 2.54.1 を Xvfb で動かし、`Tone.Offline`(FMSynth と PolySynth)を検証した。

- ユーザー操作なしで動作し、エラーも警告も出なかった。
- 出力は有効な PCM WAV(22.05kHz、モノラル、16bit、1.6 秒で 70KB)で、`ffprobe` が読めた。無音ではない(最大 -11.7dB)。
- Tone.js のビルド済みファイルは 345KB。
- 検証コードが `Tone.getContext()` を呼んだ時点で、グローバルの AudioContext の状態は `interrupted` だった。import だけで作られるのかは切り分けていない。D案ではこのコンテキストから音を出さないので、`Tone.start()` と `toDestination()` 以外の出力経路は使わない。
- 未検証: Android / macOS / Windows の WebView、rodio での実再生、vite バンドル(ESM)経由。

## ドキュメント

- `docs/guide/user-guide.md` の通知の節に、MML の説明と記法の表を追加する。
- CHANGELOG は git-cliff が生成するので、編集しない。

## スコープ外

- 通知の種類ごとに別の音を割り当てる機能。
- 音色の切り替え、ピアノロール、ステップシーケンサー。
- MML の共有・インポート・エクスポート。
- 通知音以外の用途。

## 実装後の確認

2026-10-16、`feat/issue-75-mml-ringtone` の HEAD 273d832 で確認した。実アプリは Xvfb(:87)・`dbus-run-session`・隔離した XDG_*(/tmp 直下)で起動し、`WAYLAND_DISPLAY` は unset した(実画面には出していない)。

確認できたこと:
- 自動テスト: `cargo test` は 721 passed / 0 failed / 61 ignored、`pnpm check` は 0 errors(既存の `UrlPreviewCard.svelte` の警告 1 件のみ)、`pnpm test` は 83 files / 882 tests passed。
- `cargo fmt --check`: 差分は既存ファイルに広く既にあり(main でも同様)、今回追加・変更した `mml.rs` / `domain/mml.rs` / `commands/sound.rs` には出ない。
- 設定→通知で「通知音を鳴らす」ON、種類「MML（打ち込み）」、`t120 o5 l8 cdefgab>c` を入力すると「再生時間 2.0 秒」が出た。
- 試聴: IPC(`ipc://localhost/…` への fetch)を記録した。`play_notify_sound` の `choice` は `data:audio/wav;base64,UklGR…` で、長さは 152,975 文字(WAV 約 112KB、RIFF/22050Hz/mono/16bit を先頭バイトで確認)。`beep` には落ちず、「試聴に失敗しました」は出なかった。別の MML(`… r8 g`)では `parse_mml` 2 回(ピッカーの検証と再生前の解決)の後に `play_notify_sound`(182,375 文字)が続いた。stderr とログファイルに WARN/ERROR はなかった。
- 不正な `c4 x`: 「4 文字目: 不明な文字 `x` です」(赤字)、保存ボタンと試聴ボタンの `disabled` 属性が付いた。
- 保存: `settings.json` の `notify.soundChoice` が `"mml:t120 o5 l8 cdefgab>c"` になった。アプリを再起動して設定→通知を開くと、入力欄・モード(MML)・「通知音を鳴らす」ONが復元された。
- カラム追加モーダル(通知音 ON → 継承の選択肢つき → MML): `c4 x` で追加ボタンと試聴が無効、`t120 o5 l8 efg` で「再生時間 0.8 秒」・追加ボタンが有効。

確認できなかったこと:
- 実際に音が鳴ること(Xvfb には音声デバイスがない)。Rust 側の再生は、`data:` URL を受け取り警告が出ていないことまでを見た。
- カラム追加モーダルでの保存と再編集: 確認用のアカウントにトークンがなく、`unauthorized: no token for account` でカラムが作れなかった。
- 実サーバーの通知カラムに新着を出して鳴らす経路。
- Android の実機・エミュレーターの WebView、macOS / Windows。

気づいたこと:
- 隔離起動のデバッグビルドは `frontend/dist` を埋め込むため、古い dist(MML 以前)のままだった。`pnpm build` で dist を作り直してから `cargo build` した(dist は gitignore 対象)。
- `window.__TAURI_INTERNALS__.invoke` は書き換え不可で、差し替えは無言で失敗する。呼び出しの記録は `window.fetch` の `ipc://` を包んで行った。
- 同じ MML の 2 回目の試聴は `resolveSoundChoice` のメモ化により `parse_mml` を呼ばない(仕様どおり)。

## 上限変更(90 秒・5000 文字・3000 音)後の確認

2026-10-17、`feat/mml-limit-90s` の HEAD 722dcb4 で確認した。描画は WebKitGTK(Xvfb :97)で本物の `renderScore` を、実アプリは Xvfb(:88)・`dbus-run-session`・隔離した XDG_*(/tmp 直下)で確認した。どちらも `WAYLAND_DISPLAY` は unset し、実画面には出していない。このセッションでソースコードは変更していない。実アプリの確認ではユーザーの別の開発インスタンスが同時に動いており(負荷平均は 2〜3)、描画時間の数値はその影響を含む。

### 描画(`renderScore`、1 ケース 1 回ずつ、ページは毎回読み直し)

スコアは本物の `mml::parse` の出力(JSON)をそのまま渡した。`renderMs` は `renderScore` の呼び出しから戻るまで、`data:` URL の長さは文字数、最大停止は描画中の 50ms の `setInterval` の最大の間隔。全ケースで例外なし・NaN なし(`bad` 0)。

| ケース | 音数 / トラック / 長さ | renderMs | data: URL の長さ | ピーク | 最大停止 |
|---|---|---|---|---|---|
| デモ曲(`@epiano` / `@bass` / `@bell`) | 210 / 3 / 85.7 秒 | 4,263 | 5,075,362 | 0.1823 | 424 ms |
| `fm` 3000 音(`t255 l64`) | 3000 / 1 / 44.1 秒 | 27,066 | 2,629,478 | 0.2996 | 138 ms |
| `lead` 3000 音(`t255 l64`) | 3000 / 1 / 44.1 秒 | 74,635 | 2,629,478 | 0.3983 | 270 ms |
| `noise` 3000 音(`t255 l64`) | 3000 / 1 / 44.1 秒 | 95,595 | 2,629,478 | 0.4857 | 1,604 ms |
| `lead` 3000 音を 4 トラックに(750 × 4) | 3000 / 4 / 11.0 秒 | 31,642 | 683,890 | 0.3929 | 669 ms |
| `fm` 3000 音(`t255 l32`) | 3000 / 1 / 88.2 秒 | 45,319 | 5,223,598 | 0.2710 | 388 ms |
| `lead` 3000 音(`t255 l32`) | 3000 / 1 / 88.2 秒 | 122,936 | 5,223,598 | 0.3333 | 409 ms |
| `noise` 3000 音(`t255 l32`) | 3000 / 1 / 88.2 秒 | 187,857(注) | 5,223,598 | 0.4154 | 1,344 ms |

(注)`noise` の 88.2 秒・3000 音は、120 秒を超えたら中断する取り決めどおり 125 秒の監視で 1 回打ち切った(タイムアウトとして記録)。そのあと、数値を取るためだけに監視を 450 秒に延ばして 1 回測り直すと、187,857 ms で完了した。

3000 音の MML は、`t255 l64 @<音色>`(88.2 秒のケースは `l32`)の後ろに `[c]99` を 30 個並べて `[c]30` を 1 個足した形(198〜201 文字)で、`mml::parse` が OK(3000 音、44.118 秒または 88.235 秒)を返すことを確認してから使った。4 トラックのケースは `t255 l64 @lead` の `[c]99` を 7 個と `[c]57` を 1 個並べたトラックを `;` で 4 つ並べた(254 文字)。

基準との比較:
- 満たした: 例外なし、NaN なし、ピーク 1 未満(最大 0.4857)。デモ曲は 20 秒以内(4.3 秒)。`data:` URL は最長でも 5,223,598 文字(約 7MB の目安の内側)。メインスレッドの最大停止は 1.6 秒で 5 秒以内。
- 外れた: 3000 音の最悪ケースは描画に 2 分前後以上かかる(`lead` 88.2 秒で 122.9 秒、`noise` 88.2 秒で 187.9 秒。44.1 秒でも `noise` は 95.6 秒)。音数が多いほど、1 音あたりの時間も長くなる傾向だった(#494 の確認では、200 音の `lead` が WebKitGTK で約 1.6 秒)。ページは応答し続けた(最大停止は 1.6 秒)ので、UI は固まらないが、試聴や保存後の先読み(`prewarmSoundChoice`)は、その間ずっと裏で描画を続ける。今回は直さず、数値だけを残す(上限の 3000 音を使い切る MML が実際にどれだけ使われるかは分からない)。

### 実アプリ(隔離インスタンス、設定→通知)

- デモ曲を MML 欄に入れると「再生時間 85.7 秒」が出て、保存と試聴は有効だった(スクリーンショットで目視)。「試聴」を押すと、`parse_mml` の呼び出しに続いて、押してから 4,480 ms 後に `play_notify_sound` が記録された。`choice` は `data:audio/wav;base64,UklGRiAV…`(RIFF/WAVE、22050Hz、モノラル、16bit を先頭バイトで確認)で長さ 5,075,362 文字(IPC 本文は 5,075,375 文字)。`beep` には落ちず、「試聴に失敗しました」も出なかった。長さは描画ハーネスの値(5,075,362)と一致し、約 5MB の文字列が IPC で Rust に渡って再生経路に入った。実アプリのログ(`tsumugi.log`)の WARN / ERROR は 0 件。stderr の WARNING は、隔離環境に AT-SPI のバスがないことによる GTK の警告と、Xvfb に DRI3 がないことによる MESA の警告だけ(アプリ由来ではない)。
- 上限のエラー表示(いずれも赤字で、保存と試聴の `disabled` が真になった。表示は画面で目視):
  - デモ曲の最初の 2 トラックの繰り返しを 10 から 11 にした MML: 「38 文字目: 再生時間は 90 秒以下にしてください」。
  - 3000 個を超える 3001 音の MML(`t255 l64 @fm` と `[c]99` を 30 個と `[c]31`): 「195 文字目: 音符は 3000 個までです」。
  - 5001 文字の MML(`c ` を並べたもの): 「5001 文字目: MML は 5000 文字以下にしてください」。
  - 「記法」の末尾の行は「全体で 5000 文字・90 秒・3000 音まで」になっている(DOM のテキストで確認)。
- 保存と復元: デモ曲を保存すると、`settings.json` の `notify.soundChoice` が `"mml:t112 o5 l4 @epiano [e g a >c< b a g2 a g e d c1]10; t112 o3 l2 @bass [c g g d a e c c]10; t112 o6 l1 @bell [r r r c]10"` になった。アプリを再起動して設定→通知を開くと、入力欄にデモ曲が復元され、モードは MML、「通知音を鳴らす」は ON、「再生時間 85.7 秒」が出た(スクリーンショットで目視)。
- 互換: `t120 o5 l8 cdefgab>c` の試聴で渡る `choice` は 152,962 文字(IPC 本文 152,975 文字)で、#75 / #494 で記録された値と同じ。
- 再起動後にデモ曲の試聴を押すと、`play_notify_sound` は押してから 8 ms で記録された(その前に `parse_mml` もなし)。起動時に保存済みの通知音を先読みする `prewarmSoundChoice`(`store.svelte.ts`)がデモ曲を描画済みだったためと考えられる(この点は描画が裏で走ったことまでは直接確認していない)。

### 確認できなかったこと

- 実際に音が鳴ること、聞こえ方(Xvfb にも音声デバイスがない)。Rust 側の再生は、`data:` URL を受け取り、警告が出ていないことまでを見た。
- Android の WebView(エミュレーターを含む)での 90 秒・3000 音の描画。#494 の確認では、200 音の `lead` がエミュレーターで 4.2 秒と WebKitGTK の約 2.6 倍だった。今回の WebKitGTK の数値から見て、3000 音級の MML は Android ではさらに遅い可能性が高いが、測っていない。
- macOS / Windows。
- カラム追加モーダルでの保存と再編集(確認用アカウントにトークンがなく、カラムが作れない)。確認は設定→通知の既定の通知音だけ。
- 実サーバーの通知カラムに新着を出して鳴らす経路。
- 90 秒の通知音が鳴り終わるまでの間に次の通知が来たときの挙動。今回は変えておらず、試してもいない(再生はチャンネル経由でバックグラウンドのスレッドが行う、というところまでをコードで見ただけで、重なるのか待たされるのかは確認していない)。
- 3000 音のうち、`lead` / `noise` 以外の音色の組み合わせや、複数トラックで 90 秒かつ 3000 音の最悪ケース(測ったのは 4 トラックで 11 秒のものまで)。

### 気づいたこと

- 隔離起動のデバッグビルドは `frontend/dist` を埋め込むので、`pnpm build` → `cargo build`(`src-tauri` で)の順に行った。ユーザーの開発インスタンスと `src-tauri/target/debug` を共有している。
- 同じ MML は 2 回目以降の試聴で `parse_mml` も描画も走らない(`resolveSoundChoice` のメモ化。今回 8 件に増やした)。

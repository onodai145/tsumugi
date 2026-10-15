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
- 上限: MML は 1000 文字以下、全トラックの再生時間は 10 秒以下。繰り返しは展開しながら時間を積算し、10 秒を超えた時点でエラーにする(展開しきってから判定しない)。

v1 に入れないもの: 音色切り替え(`@`)、エンベロープやビブラートなどの装飾、`n<番号>` 形式の音符指定、ループのネスト、トラックごとのテンポ共有、向きの反転設定。

### 3. Rust

- `src-tauri/src/mml.rs`(新規): 外部に依存しない純粋なパーサー `parse(text: &str) -> Result<MmlScore, MmlError>`。繰り返しの展開・タイの連結・テンポの解決をここで済ませ、時刻は秒で持つ。
- `src-tauri/src/domain/mml.rs`(新規、`specta::Type`):
  - `NoteEvent { track: u8, start_sec: f64, duration_sec: f64, midi: u8, velocity: f64 }`
  - `MmlScore { notes: Vec<NoteEvent>, total_sec: f64, track_count: u8 }`
  - `MmlError { position: u32, message: String }`。`position` は入力テキストの文字インデックス(0 始まり)。
  - `MmlParseResult`: `{ status: "ok", score }` か `{ status: "error", error }` の列挙型(`#[serde(tag = "status")]`)。UI が位置を使うので、構文エラーはコマンドの `Err` ではなくデータとして返す。
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
  - モバイルでは、ソフトキーボードの `--keyboard-inset` の扱いを、既存のモーダルに合わせる。
  - 見た目は `docs/design/style-guide.md` に従い、一回限りの値は作らない。

### 5. エラー処理

- 保存時: 構文エラーや上限超過は、位置と理由を表示して保存できないようにする。
- 再生時: `parse_mml` の失敗、Tone.js の読み込み失敗、レンダリング失敗は、すべて `beep` で代替する。保存済みの MML が、将来のパーサー変更で通らなくなった場合もここに入る。通知は止めない。理由はフロントのデバッグログと Rust の `warn` ログに残す。
- 初回の再生が遅れる場合は、先読みで隠す(spike では、1.6 秒の音のレンダリングが初回 58ms、2 回目 12ms だった)。

## テスト

- Rust(単体): パーサーを網羅する。音符・長さ・付点・タイ・オクターブ・テンポ・音量・繰り返し・マルチトラック・上限(文字数、時間、繰り返しの展開途中での打ち切り)・エラー位置。`resolve_audio_bytes("mml:...")` が `beep` を返すこと。`set_notify` と `set_column_notify` が不正な MML を拒否すること。`generates_frontend_bindings` で新しい型の出力を確認する。
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

# Android 版で触覚入り ogg の通知音を振動つきで鳴らす 設計 (Issue #592)

## 背景と目的

Android には、ogg(Vorbis)に触覚データを持たせ、音と同期して端末を振動させる仕組みがある。触覚データは、音声ストリームの末尾の追加チャンネルとして入り、Vorbis コメントの `ANDROID_HAPTIC=<触覚チャンネル数>` で示す(AOSP「UX foundation for haptic framework」)。再生は `MediaPlayer` / `SoundPool` が対応し、`AudioAttributes.Builder.setHapticChannelsMuted(false)` を指定したときだけ振動する(既定は触覚チャンネルがミュート)。

tsumugi の通知音は Rust の `SoundPlayer`(rodio)で鳴らしており、触覚の概念がない。触覚入り ogg を選んでも、振動せず、3ch 以上の音声として扱われる。

この設計は、Android で、触覚入り ogg を通知音に選んだときに、音と一緒に端末を振動させる。

## 実機での事前確認(スパイク)

Pixel 8 Pro(Android 17 / SDK 37)で確認した(2026-11-17)。

- `AudioManager.isHapticPlaybackSupported()` は `true`。振動子の能力に `EXTERNAL_CONTROL` と `AMPLITUDE_CONTROL` がある。
- 3ch(L・R・触覚)で `ANDROID_HAPTIC=1` の ogg を `MediaPlayer` で再生した。

  | 条件 | システムの振動履歴(`dumpsys vibrator_manager`) |
  |---|---|
  | 触覚ミュートを解除 | `external` 振動が 1693ms 記録された(usage=NOTIFICATION)。体感でも振動した |
  | 触覚ミュートのまま | 記録なし |
  | 触覚タグなしの 2ch ogg | 記録なし |

- Context の無い環境では、パス指定・ファイルディスクリプタ指定の `setDataSource` が NullPointerException になる。メモリ上のバイト列を渡す `MediaDataSource` は動いた。この設計はバイト列で渡す。

## 決定事項(ブレインストーミングで合意済み)

- Android だけが対象。触覚入りと判定した音だけを `MediaPlayer` で再生し、それ以外は今まで通り rodio で再生する(Android の通知音をすべて `MediaPlayer` に切り替える案は、動作実績のある rodio の経路を捨てるため採らない)。
- 触覚の ON/OFF は、既存の設定「ハプティクス(振動)を有効にする」(`UiPrefs.haptics_enabled`)に従う。OFF のときは触覚をミュートしたまま、音だけ鳴らす。通知音用の別トグルは作らない。
- ogg から触覚チャンネルを取り出して `VibrationEffect` の波形に変換する案は採らない(音との同期が粗く、品質も低い)。

## 目標 / 非目標

目標:

- Android で、触覚入り ogg(Vorbis)を通知音に選ぶと、通知時と「試聴」で、音と一緒に振動する。
- 触覚なしの音(プリセット、MML、普通の mp3 / wav / ogg)は、今までと同じ経路・同じ鳴り方のまま。
- 触覚を ON/OFF する設定は、既存のハプティクス設定ひとつ。

非目標:

- MML やプリセットの音への触覚の付与、OS 通知チャンネルの振動(OS 通知は `tauri-plugin-notification` の `"default"` チャンネルのままで、アプリ側では振動を設定していない)。
- デスクトップ対応(振動デバイスがなく、触覚入りの ogg は今まで通り rodio に渡される)。
- Opus など Vorbis 以外の触覚入り音声(実機で確認していない)。
- 触覚入り ogg の作成・編集機能(ユーザーガイドに作り方の例を書くだけ)。
- 複数の通知音を重ねて鳴らすこと(触覚入りの音は後勝ち。後述)。

## 構成

### 全体の流れ

```
フロント playNotifySound(choice)            ← 変更なし。mml: を data: に解決して渡す
  └─ Rust play_notify_sound(choice)
       ├─ resolve_audio_bytes(choice) → バイト列
       ├─ [Android && has_haptic_channel(bytes)]
       │    └─ プラグイン haptics: play_haptic_audio(bytes, haptics_enabled)
       │         ├─ 成功 → 終わり
       │         └─ 失敗 → warn ログ + beep を rodio で再生
       └─ [それ以外] state.sound.play(bytes)    ← 今まで通り(rodio)
```

### 1. 判定(`src-tauri/src/sound.rs`・純粋関数)

`pub(crate) fn has_haptic_channel(bytes: &[u8]) -> bool` を追加する。副作用がなく、全プラットフォームでコンパイルでき、単体テストできる。

true になる条件(すべて満たす):

1. 先頭 4 バイトが `OggS`。
2. 先頭 64KiB の範囲に、Vorbis のコメントヘッダ(パケット種別 `0x03` + `vorbis`)がある。
3. コメントヘッダを Vorbis の仕様どおり(ベンダー文字列の長さ、コメント数、各コメントの長さ付き UTF-8)に読み、キーが `ANDROID_HAPTIC`(大文字小文字を区別しない)で値が 1 以上の整数のコメントがある。

上記以外(WAV・MP3・Opus・壊れたデータ・値が 0 や数値でないもの)は false。範囲外を読みそうになったときも false(パニックしない)。

既知の制約: コメントヘッダが 64KiB を超える(カバー画像などを含む)か、Ogg のページ境界をまたぐ場合は、判定できずに false になる。その場合は触覚なしの音として rodio で鳴る。

### 2. 振り分け(`src-tauri/src/commands/sound.rs`)

`play_notify_sound` に `app: AppHandle` を足す(tauri-specta は `AppHandle` / `State` を TS の引数に出さないので、フロントのバインディングは変わらない)。

`resolve_audio_bytes` の後に、`#[cfg(target_os = "android")]` で次を行う。

1. `has_haptic_channel(&bytes)` が false なら、今まで通り `state.sound.play(...)`。
2. true なら、`haptics_enabled` を `state.settings.load_ui()` から読む(読めなければ既定値の `true`。`UiPrefs` の既定値と同じ)。
3. `app.haptics().play_haptic_audio(bytes, haptics_enabled)` を `tauri::async_runtime::spawn_blocking` 内で呼ぶ(`run_mobile_plugin` は同期でブロックするため)。
4. `Err` なら、`log::warn!` を残して beep(`PRESET_BEEP`)を `state.sound.play` で鳴らす。

コマンド全体は、今まで通り常に `Ok(())` を返す(失敗は通知フローを止めない)。デスクトップでは、このブロックは存在せず、常に `state.sound.play`。

### 3. プラグイン(`src-tauri/plugins/tauri-plugin-haptics/`)

既存の `tauri-plugin-haptics` に、音声の再生メソッドを足す(音声再生のプラグインを新設するほどの規模ではないので、ここに置く)。

- `src/mobile.rs`: `Haptics::play_haptic_audio(&self, data: &[u8], haptics_enabled: bool) -> Result<()>` を追加する。`data` は base64 文字列にして `run_mobile_plugin("playHapticAudio", ...)` へ渡す。
- `src/models.rs`: リクエスト型 `PlayHapticAudioRequest { data: String, haptics_enabled: bool }`(camelCase)を追加する。
- JS に公開するコマンドは増やさない(Rust からだけ呼ぶ)。`build.rs` の `COMMANDS` と `permissions/` は変更しない。
- デスクトップ側(`desktop.rs`)には追加しない。呼び出しは `cfg(target_os = "android")` の中だけ。

### 4. Kotlin(`.../haptics/HapticsPlugin.kt`)

`@Command fun playHapticAudio(invoke: Invoke)` を追加する。

- 引数: `data`(base64)、`hapticsEnabled`(boolean)。
- `MediaPlayer` を使う。生成・操作・コールバックは、すべてメインスレッドで行う(`activity.runOnUiThread`)。
- `AudioAttributes` は、usage=`USAGE_NOTIFICATION`、contentType=`CONTENT_TYPE_SONIFICATION`、`setHapticChannelsMuted(!hapticsEnabled)`(実機で確認した組み合わせ)。
- データは `MediaDataSource`(base64 をデコードしたバイト列)で渡す。ファイルは作らない。
- `prepareAsync()` で準備し、`onPrepared` で `start()` して `invoke.resolve()`。`onError` や準備のタイムアウト(5 秒)では `invoke.reject(...)`。どの経路でも必ず resolve / reject のどちらか一度だけ呼ぶ(Rust 側がブロックしたままにならないようにする)。
- 再生の終了(`onCompletion`)とエラーで `release()` する。
- 後勝ち: 再生中の `MediaPlayer` があれば、新しい要求の処理の前に `stop()` + `release()` する。保持するのは最新の 1 つだけ。
- `Build.VERSION.SDK_INT < 29`(`setHapticChannelsMuted` が使えない)の端末では reject する(Rust 側が beep で代替)。

### 5. フロント・UI

変更なし。

- 音声ファイルの選択は ogg に対応済みで、「試聴」も `playNotifySound` を通るため、同じ経路に乗る。
- 設定のハプティクス ON/OFF(モバイルタブ)も変更しない。説明文の追記だけ、ユーザーガイドで行う。

### 6. ドキュメント

`docs/guide/user-guide.md` の通知音の節に、次を追記する。

- Android で、触覚入りの ogg(Vorbis、`ANDROID_HAPTIC` タグ付き)を選ぶと、音と一緒に振動すること。
- 振動は設定のハプティクスに従うこと(OFF なら音だけ)。
- 端末が音声連動の触覚に対応している必要があること(非対応なら音だけ鳴る)。
- 触覚入り ogg の作り方の例(ffmpeg で、3ch 入力に `-metadata ANDROID_HAPTIC=1` を付けて libvorbis で書き出す)。
- 触覚入りの音は、複数の通知が続いたとき、前の音を止めて最新の音だけ鳴らすこと。
- システムの通知音量・マナーモードに従うこと(下の「挙動の変化」を参照)。

## 挙動の変化

触覚入りの音だけが、次の点で、今までの rodio の再生と変わる。

- **音量・マナーモード:** `USAGE_NOTIFICATION` で鳴らすため、システムの「通知」の音量に従い、マナーモードや「おやすみモード」では音が出ない(振動はシステムの通知の振動設定に従う)。rodio の通知音は「メディア」相当の経路。
- **重ね再生:** 触覚入りの音同士は後勝ち(前の再生を止める)。rodio の音とは別の経路なので、触覚なしの音とは重なって鳴る。

`USAGE_NOTIFICATION` を選んだのは、実機で振動を確認した組み合わせだからである。`USAGE_MEDIA` での振動の挙動は確認していない。

## エラー処理

| 状況 | 挙動 |
|---|---|
| 触覚入りではない音 | 今までと同じ(rodio) |
| 触覚入りだが、設定のハプティクスが OFF | 触覚をミュートして、`MediaPlayer` で音だけ鳴らす |
| 端末が音声連動の触覚に非対応 | 未確認(非対応端末が手元にない)。公式ドキュメントの想定では、触覚は再生されず音だけ鳴る。実機で確認できるまで、ユーザーガイドには「端末による」とだけ書く |
| Kotlin 側の失敗(デコード不可、準備のタイムアウト、API 28 以下) | `warn` ログ + beep を rodio で再生。通知フローは止めない |
| `has_haptic_channel` の判定不能 | false(rodio) |

`log::warn!` は、これまで通り `enable_file_logging` が有効なときだけファイルに残る。

## テスト

Rust(自動):

- `has_haptic_channel` の単体テスト(`src-tauri/src/sound.rs`)。fixture は、ffmpeg で作った短い(0.1 秒程度)ogg を `src-tauri/tests/fixtures/` に置く。
  - 触覚入り(3ch・`ANDROID_HAPTIC=1`)→ true
  - 触覚タグなしの 2ch → false
  - `ANDROID_HAPTIC=0` → false
  - キーが小文字(`android_haptic=1`)→ true
  - 数値でない値(`ANDROID_HAPTIC=abc`)→ false
  - 先頭が `OggS` でないバイト列、WAV、空、先頭だけ切れた ogg → false(パニックしない)
- 既存の `resolve_audio_bytes` のテストは変更しない。

Kotlin(自動テストの基盤がない): CI の `android-build` ジョブでビルドが通ることだけを確認する。

実機(手動・Pixel 8 Pro。受け入れ基準):

1. 触覚入り ogg を通知音に設定して通知を受けると、音と一緒に振動する。「試聴」でも同様。
2. 設定のハプティクスを OFF にすると、同じ ogg で音だけ鳴る。
3. プリセット・MML・触覚なしの音は、今までどおり鳴る(rodio)。
4. 触覚入りの通知が連続しても、前の再生が残らず(後勝ち)、アプリが落ちない。
5. 壊れた ogg(触覚タグだけある不正なデータ)を設定しても、beep で代替されて通知が止まらない。

## リスクと未確認事項

- **base64 の受け渡しのコスト:** 通知音の上限は 5MB(`MAX_NOTIFY_SOUND_BYTES`)なので、最大で約 6.7MB の文字列を、通知のたびに JNI(JSON)で渡す。触覚入りの通知音は数十 KB が普通なので、当面は許容する。実機で遅い場合は、キャッシュのファイルのパスを渡す方式に切り替える。
- **端末依存:** 実機で確認したのは Pixel 8 Pro だけ。ほかの端末の触覚の挙動は未確認。
- **`USAGE_MEDIA` の挙動:** 未確認(上記)。
- **Opus など:** 対象外。`has_haptic_channel` は Vorbis だけを true にする。

## 影響を受けるファイル

- `src-tauri/src/sound.rs`(`has_haptic_channel` とテスト)
- `src-tauri/src/commands/sound.rs`(振り分け)
- `src-tauri/plugins/tauri-plugin-haptics/src/{mobile.rs,models.rs}`、`android/src/main/java/com/onodai/tsumugi/haptics/HapticsPlugin.kt`
- `src-tauri/tests/fixtures/`(ogg の fixture)
- `docs/guide/user-guide.md`

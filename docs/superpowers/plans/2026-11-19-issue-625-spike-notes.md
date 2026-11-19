# Issue #625 スパイクの記録

## (a) 外部エンジンの互換性
- VOICEVOX ENGINE(イメージ `voicevox/voicevox_engine:cpu-latest`、`/version` = `0.26.2`): `/speakers` は 44 件の話者を返し、スタイルは `{name, id, type}`(`id` は数値)。`/audio_query` の応答に `speedScale` と `volumeScale` が存在する。
- `speedScale`: 1.0 で 3.16 秒、2.0 で 1.6 秒(`/synthesis` に効く)。`volumeScale`: 振幅のピークが 1.0 → 10533、0.5 → 5266、0.1 → 1053(比例して効く)。WAV の先頭は `RIFF`。
- AivisSpeech Engine: **未検証**(起動済みの環境の URL が無かったため。イメージ名やポートは推測で使わない)。VOICEVOX 互換 API を持つことは README の記述のみで確認(`ExternalEngine` は応答を不透明な JSON として扱うので、差異に強い設計にしてある)。

## (b) Android の平文 HTTP
- `gen/android/app/src/main/AndroidManifest.xml` は `android:usesCleartextTraffic="${usesCleartextTraffic}"`(プレースホルダ)。`gen/android/app/build.gradle.kts` が、既定(release)で `"false"`、`debug` ビルドタイプで `"true"` に設定している。`networkSecurityConfig` は無い。
- 結論: **debug ビルドでは LAN の `http://` に接続できる。release ビルド(配布版)では平文 HTTP が禁止される**(Android 9 以降の既定)。配布版の Android からは、`https://` の URL が必要。UI(設定 → 読み上げ)に「Android では https:// の URL が必要なことがある」注意書きを出す(計画どおり)。LAN 内の平文を許可するには `networkSecurityConfig` の追加(CIDR 指定ができず、ドメイン単位)が必要で、このスコープ外。必要なら別 Issue にする。

## (c) rodio Player の読み上げレーンが Android で動くか
- 未実施(Task 11 で確認する)。

## 結合確認(実エンジン・実アプリ。Task 11)
- 実エンジン(VOICEVOX ENGINE 0.26.2、Docker): `#[ignore]` の `tts::external::tests::real_voicevox_engine`(`TSUMUGI_TEST_SPEECH_URL`)が PASS。声の一覧・合成・話速 2.0 で WAV が短くなること・記号と改行を含む本文の合成を確認。
- 実アプリ(隔離した `cargo tauri dev` を Xvfb + `dbus-run-session` で起動、debug bridge から操作。アカウント・カラム無しの最小構成): `get_speech_settings` / `set_speech_settings`(不正な URL は拒否、前後の空白と末尾のスラッシュを落として保存、`settings.json` に反映)/ `list_speech_voices` / `test_speech` / `get_speech_status` が動作し、エンジンのログに `/audio_query` と `/synthesis` の要求が届いた。エンジンを止めると、3回目の失敗で `error` に変わり、`SpeechStatusChanged` が届く。再起動すると次の項目で `ok` に戻る。
- **実環境で見つけた不具合(修正済み・テスト追加)**: (1) reqwest のエラーの文面にリクエスト URL(クエリに読み上げる本文が入る)が含まれ、設定画面・ログ・イベントにノートの本文が漏れた → `without_url()` で落とした(`errors_do_not_leak_the_request_url_or_the_note_text`)。(2) 失敗のたびに文面が変わるため、`error` イベントが毎回再通知された → 既に `Error` なら通知しない(`three_consecutive_failures_report_an_error_once…` の偽エンジンも本文入りの文面に変更)。修正後は、イベントは `ok → error(1回) → ok`。
- 確認できていない: スピーカーからの実際の再生(Xvfb にオーディオデバイスが無い。`SoundPlayer` の再生スレッドは出力デバイスが開けないと終了し、`speak` は `Failed` で返る)。カラム単位の経路(`speak_note` → `columnNote` リスナー)は、単体テスト(`commands::speech`)で確認済みだが、実アプリでは未確認(アカウント・Misskey のストリームが要るため)。**ユーザーの環境で聞いて確認してもらう項目。**

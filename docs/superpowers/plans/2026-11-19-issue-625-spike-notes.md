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

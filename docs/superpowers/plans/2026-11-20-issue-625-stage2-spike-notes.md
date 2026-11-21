# 段階2 スパイクノート(Issue #625)

plan 作成時(2026-11-20)に、上流の実物を取得して Linux x86_64 で確かめた結果。実装の根拠として残す。

## ① 取得物

| 種類 | 名前 | サイズ(バイト) | SHA-256 | アーカイブ内のパス |
|---|---|---|---|---|
| CORE | `voicevox_core-linux-x64-0.17.1.zip` | 1,457,717 | `844b49ce48040ecd85c8b2baaf75f77855277a0f92f6e87ea7ec45c820943323` | `voicevox_core-linux-x64-0.17.1/lib/libvoicevox_core.so` |
| ONNX Runtime | `voicevox_onnxruntime-linux-x64-1.17.3.tgz` | 6,765,720 | `72b5287fdd48dc833a9929f6e9e3826e793b54ce1202181be93f63823a222f58` | `voicevox_onnxruntime-linux-x64-1.17.3/lib/libvoicevox_onnxruntime.so.1.17.3` |
| 辞書 | `open_jtalk_dic_utf_8-1.11.tar.gz` | 23,646,843 | `fe6ba0e43542cef98339abdffd903e062008ea170b04e7e2a35da805902f382a` | `open_jtalk_dic_utf_8-1.11/`(`sys.dic` など) |
| 声 | `0.vvm`(`voicevox_vvm` 0.17.1) | 約 59 MB | `ecd35374d4182cd883cba5040376f7f888cc6ba248b1c2f4cea07cdb34bb1318` | — |

他の環境の値は `catalog.rs` の表(Task 2)。VVM は 27 個(`0`〜`25`、`n0`)、約 1.65 GB、読み上げ用スタイル 137 個(ID の重複なし)。`s0`(130 MB)は歌声用(`frame_decode` / `sing`)だけなので含めない。

## ② Linux での実測

| 項目 | 結果 |
|---|---|
| ONNX Runtime | 1.17.3 をパス明示でロードできた(CORE 0.17.1 の推奨は 1.23.2 の prerelease)。ロード 0.01 秒 |
| VVM のロード | `0.vvm`(約 59 MB)で 0.6 秒 |
| 合成 | 5.1 秒ぶんの音声が約 1.0 秒(2 回目 0.84 秒)。推論 8 スレッド・16 コア |
| 最大メモリ | 278 MB |
| WAV | 24 kHz・モノラル・16 bit。最大振幅 9310、ほぼ全区間が無音ではない |
| システムライブラリ | `libstdc++`・`libgcc_s`・`libm`・`libc` だけ(`libgomp` は不要) |
| VVM のハンドル | ロード直後に `voicevox_voice_model_file_delete` で閉じても合成できる |

## ③ 規約・クレジット

- ONNX Runtime の規約は、リリース本文だけでなく tgz 内の `TERMS.txt`(938 バイト。Windows 版は 960 バイトで改行コードが違う)。同意版は改行コードを除いてハッシュする。
- 音声モデルの規約は `voicevox_vvm` の `TERMS.txt`(15,990 バイト)。
- クレジットは規約の `## キャラ名` の節の「…」から取り出す(例: 「VOICEVOX:ずんだもん」「VOICEVOX:Voidoll(CV:丹下桜)」)。`n0` は「VOICEVOX Nemo」。個別の節がない `雨晴はう` は全体の規約に従い「VOICEVOX:雨晴はう」。

## ④ spec との差(Task 1 で spec を訂正済み)

1. VVM は 28 個・約 1.7 GB ではなく、27 個・約 1.65 GB。
2. 声の一覧は、新しいコマンド `list_speech_core_voices`(既存の `list_speech_voices` は URL を引数に取る)。
3. `cfg(desktop)` で囲むのはネイティブの呼び出し(`NativeCore`)だけ。コマンドとイベントを環境で変えると TS バインディングが環境ごとに変わる。
4. ONNX Runtime の規約の場所は、リリース本文ではなく tgz 内の `TERMS.txt`。

## 自動テストの件数(Task 12 時点)

- Rust: `cargo test --lib` 1347 件成功(実物・実機のものは `ignored`)。
- フロント: `vitest` 1591 件成功(134 ファイル)。`pnpm check` はエラー 0。

## 実アプリ(Xvfb)での確認

Xvfb + 新しい D-Bus セッション + 隔離した XDG で `cargo tauri dev` を起動し、デバッグブリッジ(kick-and-poll)で確かめた(Linux x86_64)。

- `get_speech_core_info`: `supported: true`、`termsAccepted: false`、声は 27 件。
- `start_speech_core_download` に誤った版・空の版・`s0` を渡すと拒否される(`規約への同意が必要です`)。`termsAccepted` は `false` のまま。
- 正しい版で `engine` を取得: イベント `SpeechCoreStatusChanged` が `downloading → verifying → ready` と遷移した(17 件)。続けて声 `0` も同様に `ready`。
- `set_speech_settings`(`engine: "voicevoxCore"`、`defaultVoiceId: "3"`、URL 空)を保存でき、`list_speech_core_voices` が取得済みの声を返した。
- **不具合を発見**: `test_speech` が `SPEECH_ENGINE_NOT_CONFIGURED` で拒否された(段階1の `enqueue_note_core` / `test_speech_core` が、エンジンの種類を見ずに外部 URL の空を拒否していた)。`engine_is_configured()` で直し、`commands::speech::tests::the_built_in_engine_needs_no_url_to_queue_notes_or_the_test_phrase` で RED→GREEN にした。ホットリビルド後に再確認した。
- 声 `0` を削除すると `notInstalled` に戻り、`test_speech` を 3 回続けると、状態が `error`(`invalid: SPEECH_CORE_NOT_READY`)になった(フロントの `speechErrorText` が日本語にする)。
- 声 `0` を取得し直して `test_speech` を呼ぶと、合成が走り(`voicevox_core::synthesizer: CPUを利用します`)、`get_speech_status` が `ok` になった。
- 小さな挙動: 取得の開始直後の最初の `SpeechCoreStatusChanged` は `termsAccepted=false` で届く(同意の記録が開始通知より後のため)。次のイベントで `true` になる。UI は開始後に状態を引き直すので実害はない。

## `#[ignore]` の実物のテストの結果

- `tts::voicevox::ffi::real_tests::native_core_loads_and_synthesizes_a_real_wav`: 成功(約 3.4 秒)。
- `tts::voicevox::real_tests::real_end_to_end`(実際の GitHub から取得): 成功(約 15 秒。エンジン一式 + 声 `0` で 181 MB)。話速 2.0 は 1.0 より十分短く、音量 0.1 は 2.0 よりピークが小さい。取得していない声は `ERR_NOT_READY`。
- `tts::voicevox::catalog::tests::the_embedded_terms_match_upstream`: 成功(規約の写しが上流と一致)。

## 未確認

- スピーカーからの実際の音(Xvfb には再生デバイスがない)。
- macOS・Windows・Linux arm64 での実行(取得物の SHA-256 とライブラリ名は、実物から表に入れたが、読み込みは未確認)。
- release ビルドでの実行。
- Android(段階3)。
- ネイティブ側のクラッシュ(segfault)は、アプリごと落ちる(panic は `Err` にする)。

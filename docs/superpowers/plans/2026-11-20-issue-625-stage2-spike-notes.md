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

## Task 12 で追記する

### 実アプリ(Xvfb)での確認

(Task 12 で書く)

### `#[ignore]` の実物のテストの結果

(Task 12 で書く)

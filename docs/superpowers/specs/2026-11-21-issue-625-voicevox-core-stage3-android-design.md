# カラム読み上げ 段階3: Android 向けの内蔵 VOICEVOX CORE (Issue #625)

段階2(PR #663、`docs/superpowers/specs/2026-11-20-issue-625-voicevox-core-stage2-design.md`)で、デスクトップ向けの内蔵エンジン(公式の C API を実行時に取得して `libloading` でロード)を入れた。この spec は段階3として、**Android(arm64・x86_64)でも同じ内蔵エンジンを使えるようにする**。この段階で Issue #625 は完了する。

## 決定事項(合意済み)

- 仕組みは段階2をそのまま使う。実行時に公式の C API(CORE・ONNX Runtime・辞書・VVM)を取得し、`libloading` でロードする。`catalog.rs` に Android の行を足す形にする。
- 対象は **arm64(`aarch64`)と x86_64** だけ。armv7 と i686 は、これまで通り `supported = false`(設定で「この環境では使えません」)。
- ONNX Runtime は **Android だけ 1.23.2(prerelease)**。1.17.3 の Android 版は 4KB 整列で、16KB ページ端末で読み込めない恐れがある(下記)。デスクトップは 1.17.3 のまま。
- `libvoicevox_core.so` は共有の `libc++_shared.so` を要求する。アプリは静的 libc++ なので、NDK r27c の `libc++_shared.so` を **APK に同梱**する(arm64・x86_64、約 1.8 MB)。
- 声(VVM)・規約・同意・クレジット・UI の流れは段階2と同じ。同意版は変わらない(ONNX Runtime 1.23.2 の `TERMS.txt` は、埋め込み済みの 1.17.3 のものと一致した。938 バイト)。

## 目標 / 非目標

目標:

- Android で、設定の「内蔵 VOICEVOX CORE」を選び、エンジン一式と声を取得すると、外部エンジンなしで読み上げられる。
- デスクトップの挙動(取得物・版・ディスク配置・同意版)を変えない。
- モバイル通信・従量制では、取得の前に確認を出す(翻訳モデルと同じ)。
- 内蔵に対応しない Android の ABI(armv7・i686)では、選べず理由が分かる。

非目標:

- GPU(NNAPI など)、armv7・i686、iOS。
- アプリがバックグラウンドのときの読み上げ(段階1と同じ。アプリが動いているときだけ)。
- ONNX Runtime 1.23.2 のデスクトップへの展開(別の判断)。
- 取得物の APK 同梱(「アプリ内での dlopen」の関門が通らなかったときの退避策。その場合は spec を改訂する)。

## 実機スパイクの結果(2026-11-21)

Pixel 8 Pro(Android 17、arm64、ページサイズ 4KB の状態、9 コア)の `/data/local/tmp` で、NDK r27c でビルドした C のプローブ(`voicevox_core.h` を直接呼ぶ。使い捨て)を動かした。声は `0.vvm`(スタイル 3)。

| 項目 | 結果 |
|---|---|
| ロード | CORE 0.17.1 + ONNX Runtime 1.17.3 でも 1.23.2 でも成功(ORT のロード 0.06〜0.09 秒、声のロード約 1 秒) |
| 速度(スレッド 4) | 8.1 秒の音声を約 4.7 秒で合成(音声の約 0.58 倍の時間)。短文(3 秒)は約 1.8 秒 |
| 速度(スレッド数違い、1.17.3) | スレッド 2: 約 1.4 倍(8.1 秒の音声に 11.3 秒)、スレッド 8: 約 0.9 倍(7.2 秒)。**4 が最速**(9 コアの半分) |
| ONNX Runtime 1.23.2(スレッド 4) | 8.1 秒の音声を約 4.7 秒。1.17.3 と同等 |
| ピークメモリ | 約 485〜510 MB(ロード直後 約 260 MB) |
| WAV | 24 kHz・モノラル・16 bit、最大振幅 12758 |
| ELF 整列 | CORE 0.17.1(arm64): 16KB(0x4000)。ONNX Runtime 1.17.3(arm64): **4KB(0x1000)**。ONNX Runtime 1.23.2(arm64): 16KB |
| 依存するライブラリ | CORE: `libc++_shared.so`・`libdl`・`libm`・`libc`。ONNX Runtime: `libdl`・`liblog`・`libm`・`libc` |

実機の空きは約 17 GB(声の 27 個すべてで約 1.65 GB)。

## 設計

### 1. 取得物(`catalog.rs`)

Android の行(`("android", "aarch64")` と `("android", "x86_64")`。`std::env::consts` の値)を足す。URL の形は段階2と同じで、**ONNX Runtime の版だけ環境ごと**に持つ。

| 種類 | `android` + `aarch64` | `android` + `x86_64` |
|---|---|---|
| CORE | `voicevox_core-android-arm64-0.17.1.zip`(1,306,346 バイト、SHA-256 `069f2901…c639d37`) | `voicevox_core-android-x86_64-0.17.1.zip`(1,366,415 バイト、`43e8ef75…cdc2`) |
| ONNX Runtime | `voicevox_onnxruntime-android-arm64-1.23.2.tgz`(6,459,115 バイト、`43e6fc2a…78d46`) | `voicevox_onnxruntime-android-x64-1.23.2.tgz`(7,115,144 バイト、`807e2ea5…2d24`) |
| ライブラリ名 | CORE `libvoicevox_core.so`、ONNX Runtime `libvoicevox_onnxruntime.so`(版なし) | 同じ |

SHA-256 の全文は実装で `catalog.rs` に入れる(上の値は実物から計算済みの先頭と末尾)。辞書・VVM・規約は共通。

- `ORT_VERSION` と `ENGINE_SET`(定数)を、**環境ごとの値**にする。`EngineAssets` に ORT の版を持たせ、`ENGINE_SET` は `EngineAssets` から作る(デスクトップは今まで通り `core0.17.1-ort1.17.3-dict1.11`、Android は `core0.17.1-ort1.23.2-dict1.11`)。`store.rs` の `engine-<ENGINE_SET>`・`READY` の内容・古い版の掃除は、この値を使う。デスクトップのディスク上の名前は変わらない。
- アーカイブ内のパスは、CORE が `voicevox_core-android-<arch>-0.17.1/lib/libvoicevox_core.so`、ONNX Runtime が `voicevox_onnxruntime-android-<arch>-1.23.2/lib/libvoicevox_onnxruntime.so`(arm64 で確認済み。x86_64 は実装時に実物で確認する)。
- カタログのテスト(「`android` は非対応」を確かめている)を、「armv7・i686 は非対応、arm64・x86_64 は対応」に直す。
- ONNX Runtime 1.23.2 は prerelease。CORE 0.17.1 の推奨版なので組み合わせは公式の想定どおり。

### 2. ネイティブのロード

- `ffi.rs` の `NativeCore` と、`mod.rs` の `native_factory` は、`cfg(desktop)` から **`cfg(not(target_os = "ios"))`** に広げる(Android も含める)。`Cargo.toml` の `libloading` も、`not(any(android, ios))` から `not(ios)` にして Android に入れる。iOS は引き続き `supported = false`。
- `unsafe` は `ffi.rs` だけ、という段階2の規則は変えない。ライブラリを `Box::leak` で外さない、全アクセスを Mutex で直列化する、も同じ。
- **`libc++_shared.so` の同梱**: `app/build.gradle.kts` に、NDK(`android.ndkDirectory`、r27c)の `sysroot/usr/lib/<triple>/libc++_shared.so` を、生成するディレクトリの `jniLibs/arm64-v8a`・`jniLibs/x86_64` へコピーするタスクを足し、`sourceSets["main"].jniLibs` に加える。ビルド時にコピーするので、バイナリはリポジトリに入れない。`gen/android/.../generated/` は編集しない。armv7・i686 には入れない(内蔵を使わないため)。
- 取得した `.so` は `app_data_dir/voicevox/engine-<SET>/` に置く(段階2と同じ。展開後に読み取り可能であればよい)。

### 3. 動作

- 推論スレッド数は `ffi::default_cpu_threads()`(コア数の半分、最低 1)のまま。実測で最速(Pixel 8 Pro: 9 コアで 4)。
- アイドル解放は、`translate/manager.rs` と同じ流儀で、**Android は 3 分、デスクトップは 10 分**(`engine.rs` の定数を `cfg(target_os = "android")` で分ける)。ピーク約 500 MB のメモリを、使っていないときに持ち続けない。
- 失敗の扱い(`SPEECH_CORE_NOT_READY` / `LOAD_FAILED` / `UNSUPPORTED`、連続 3 回失敗で `Error`)、同意の Rust 側での強制、取得の同時実行 1 つ、キャンセル・再開、`READY` の原子的な置き換えは、段階2のまま。
- `supported` は、`engine_assets()` が `Some` を返す環境(デスクトップ 5 組 + Android 2 組)で `true`。

### 4. UI

- 設定の「内蔵」ラジオを、`supported = true` の Android で選べるようにする(コードの変更は不要。`get_speech_core_info` の結果で決まる)。armv7・i686 は今まで通り無効で、理由を出す。
- **取得の前の確認**: エンジン一式(約 31 MB)・声(約 55〜75 MB)の取得を始める前に、`lib/localModel.ts` の `downloadWarning(net, sizeBytes)` で、モバイル通信・従量制・判定不能なら確認を出す(翻訳モデルの取得と同じ作り。回線は `get_network_status`)。デスクトップは `Unknown` なので出ない。確認の文面の「約 N MB」は、そのとき取得する物の合計(声ならその声のサイズ)。
- 声の管理画面(検索・スタイルの `<details>`・一覧の並び)が、スマホ幅で崩れないことを実機で確認する。崩れたら、スマホ幅の見た目だけを直す(機能は変えない)。
- `docs/guide/user-guide.md` の「内蔵」の節に、Android でも使えること(arm64・x86_64、約 500 MB のメモリを使うこと、モバイル通信で確認が出ること)を足す。`CLAUDE.md` の `tts/voicevox` の段落を、Android を含めて更新する。

### 5. 先に確かめる関門(plan の最初のタスク)

実装に入る前に、実機で次の 3 つを確かめる。どれかが通らなければ、止めて spec を見直す。

1. **アプリ内での dlopen**(targetSdk 36): 取得した `.so` を、アプリのデータディレクトリから `libloading` で開けるか。スパイクは `shell` 権限の `/data/local/tmp` だったので、アプリの SELinux ドメインで制限されないかは未確認。通らなければ、取得をやめて APK に同梱する方式へ切り替える(spec を改訂する)。
2. **静的 libc++ と共有 libc++ の同居**: アプリ本体(llama.cpp を含む `libtsumugi_lib.so`)は静的 libc++、CORE は共有 `libc++_shared.so`。同じプロセスで両方が動くか(読み上げ中に翻訳も動かして確かめる)。
3. **16KB ページ端末**: 16KB ページのエミュレーター(Android の 16KB システムイメージ)で、ONNX Runtime 1.23.2 のロードと合成ができるか。

## テストと検証

自動テスト(段階2の流儀):

- `catalog.rs`: Android 2 組が `Some`(URL・サイズ・SHA-256 が表のとおり)、armv7・i686・iOS が `None`、`ENGINE_SET` が環境ごとに期待どおり(デスクトップは今までの文字列のまま)、規約の写しの版が変わらない。
- `store.rs`: `ENGINE_SET` を引数で受ける形でも、デスクトップの配置・READY・古い版の掃除が変わらない。
- `engine.rs`: アイドル解放の時間が環境で切り替わる(定数のテスト)。
- フロント: 取得の開始前に、モバイル通信・従量制で確認が出て、キャンセルすると取得が始まらない。`Unknown` では出ない。

実機での確認(手動。結果を spec に追記する):

- 関門の 3 つ(上記)。
- Pixel 8 Pro の実アプリで、取得(エンジン一式 + 声 1 つ)→選択→試し聞き→カラムの読み上げ(実際に音が出る)。
- 取得中のキャンセル・再開、アプリの再起動後の状態、声の削除。
- スマホ幅の設定画面の見た目。

CI: `android-build`(コンパイル・リンク)が通ること。`scripts/check-android-page-size.sh` は APK の `libtsumugi_lib.so` だけを見る(取得するライブラリは APK に入らない)ので、変更しない。

## リスクと未確認事項

- **アプリ内の dlopen**(関門 1): 最大のリスク。通らなければ APK 同梱へ。
- **Google Play での配布**: 実行時にネイティブコードを取得する構成は、Play のポリシーに触れる可能性がある(未確認)。現在の配布は GitHub Release の APK のみ。Play で配ることになったら、APK 同梱へ切り替える。
- **ONNX Runtime 1.23.2 は prerelease**: 実機で動くことは確かめたが、長期の安定性は未確認。デスクトップに広げるのは別の判断。
- **メモリ**: ピーク約 500 MB。低メモリ端末では OOM でアプリごと落ちうる(ネイティブ側のクラッシュはアプリごと落ちる、という段階2と同じ受け入れ済みのリスク)。アイドル解放で緩和する。
- **x86_64 のアーカイブ内のパス**・**実機の x86_64**: エミュレーターでの確認になる。速度・メモリの目安にはならない。
- **armv7・i686**: 内蔵は使えない(外部エンジンは使える)。

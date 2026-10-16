# MML 着メロの音色切り替えの設計

Issue #494(関連: #75 / `2026-10-15-issue-75-mml-ringtone-design.md`、PR #493)

## 目的

#75 で入れた MML 着メロは、全トラックが固定の FM 音源(`PolySynth(FMSynth)`)で鳴る。MML から音色を選べるようにし、トラックごと・途中でも切り替えられるようにする。

成功条件:

- MML に `@<番号>` または `@<名前>` と書くと、以降の音がその音色になる。
- 音色は 14 種。全音色の聞こえる大きさが、現行の FM 音とほぼ同じにそろっている。
- `@` を使わない保存済みの MML は、今と同じ音のまま鳴る(互換)。
- 不明な音色や範囲外の番号は、位置と理由つきの構文エラーになり、保存と試聴が止まる。
- 音を出すのは Rust(rodio)のまま。フロントは `Tone.Offline` で波形を作るだけにする(#75 と同じ)。

## 決定事項

| 決定 | 内容 | 理由 |
|---|---|---|
| 音色の数 | 14 種(下表) | ユーザーが「楽器っぽい音色も多め(10 種以上)」を選択。spike で描画できて実用的なものに絞った |
| 指定方法 | `@番号` と `@名前` の両方 | ユーザー選択。番号は着メロ MML の慣習、名前は覚えやすさのため |
| 状態の扱い | トラックごとに独立、途中で切り替え可、先頭は `@0`(`fm`) | `o` / `l` / `v` / `t` と同じ。`@0` が現行の音なので互換 |
| 番号・名前の定義元 | Rust の `Timbre` 列挙型だけ | パーサーが検証に使う。バインディングでフロントに渡るので、フロントは `Record<Timbre, …>` で網羅性を型で保証できる |
| 音量 | 音色ごとの `gainDb` の固定表 | spike で、音色間の差が最大 20 dB 以上あった |
| 保存形式 | 変更なし(`mml:<テキスト>`) | `NotifyConfig` / `ColumnDef` を変えない |

## 前提(コードの現状)

- パーサー(`src-tauri/src/mml.rs`)は、入力から**空白を除いた**文字列を読む。小文字化もする。したがって `@bell c` と `@bellc` は同じ入力になる。
- `Op` 列挙型に命令を足し、`run_track` が状態(オクターブ・長さ・テンポ・音量)を持って `NoteEvent` を作る。`NoteEvent { track, start_sec, duration_sec, midi, velocity }` と `MmlScore` は `domain/mml.rs` にあり、`specta::Type` でフロントに渡る。
- レンダラー(`frontend/src/lib/mmlRender.ts`)は、トラックごとに `PolySynth(FMSynth)` を 1 つ作り、各ノートを `note.track` の音源で鳴らす。複数トラックのときだけ `gainToDb(1 / trackCount)` をかける。余韻用に `TAIL_SEC = 0.6` を足す。
- `@` は今、「不明な文字」エラーになる。したがって既存の保存済み MML に `@` は含まれない。

## 設計

### 1. 音色表

| 番号 | 名前 | 作り方(初期案) | 備考 |
|---|---|---|---|
| 0 | `fm` | `PolySynth(FMSynth)` 現行のパラメータ | 既定。`gainDb` は 0 |
| 1 | `square` | `PolySynth(Synth)`、矩形波 | |
| 2 | `pulse` | `PolySynth(Synth)`、パルス波(幅 25%) | |
| 3 | `triangle` | `PolySynth(Synth)`、三角波 | |
| 4 | `saw` | `PolySynth(Synth)`、ノコギリ波 | |
| 5 | `sine` | `PolySynth(Synth)`、サイン波 | |
| 6 | `organ` | `PolySynth(Synth)`、倍音を足した波形、サステイン 1 | |
| 7 | `epiano` | `PolySynth(FMSynth)`、harmonicity 2、modulationIndex 3 | |
| 8 | `bell` | `PolySynth(FMSynth)`、harmonicity 5.07、modulationIndex 12、長めの減衰 | |
| 9 | `am` | `PolySynth(AMSynth)` | |
| 10 | `lead` | `PolySynth(DuoSynth)`、harmonicity 1、ビブラートあり | |
| 11 | `bass` | `PolySynth(MonoSynth)`、ノコギリ波とローパス | 低音向き。C6 以上は音量が落ちる |
| 12 | `drum` | `PolySynth(MembraneSynth)` | ドラム風。音程は下降する |
| 13 | `noise` | `NoiseSynth`(1 音ごとにインスタンスを作る) | 音程なし。ハイハットやスネア向き |

エンベロープは**全プリセットで明示する**。Tone.js の既定はリリースが約 1 秒で、`TAIL_SEC`(0.6 秒)で切れてしまう。全プリセットのリリースは 0.6 秒以下にする。

`pluck`(`PluckSynth`)と `metal`(`MetalSynth`)は入れない。spike で、`PluckSynth` は 1 音ごとにインスタンスを作らないと和音で失敗し、C5 以上が無音になり、200 音で 8.2 秒かかった。`MetalSynth` は音程があいまいで、200 音で 10.4 秒かかった。弦をはじく音が欲しければ、短いエンベロープの `FMSynth` プリセットとして後から足せる。

### 2. MML の文法

- `@<数字>` : 番号で指定する。範囲は 0〜13。数字は `o` や `t` と同じく連続する数字を 1 つの数として読む(`@10` は 10 番)。`@3c` のように、数字の直後に音符を続けてもよい。
- `@<名前>` : 上の表の名前で指定する。**表の名前に最長一致**して読み、残りは次の命令として読む。空白は先に除かれているので、`@bell c` と `@bellc` は同じ意味になる(`bell` + `c`)。どの名前も他の名前の接頭辞ではないので、一意に決まる。
- 状態: トラックごとに独立で、各トラックの先頭は `@0`。`;` でトラックを分けても引き継がれない。`[...]n` の中に書いてもよく、その場合は展開後の各回で再度適用される(`o` / `v` と同じ)。
- エラー(位置は 0 始まりの文字位置。UI は 1 始まりの「N 文字目」で表示する):
  - `@` の直後が数字でも既知の名前でもない: 「不明な音色です」。位置は `@` の次の文字。
  - 番号が 14 以上: 「音色は 0〜13 です」。位置は数字の先頭(`o9` の扱いと同じ)。
  - `@` が末尾にある、または直後に何もない: 「`@` の後に音色の番号か名前が必要です」。位置は `@`。
- 既存の上限(1000 文字・10 秒・4 トラック)は変えない。

### 3. Rust

- `domain/mml.rs`: `Timbre` 列挙型(14 値、`specta::Type`、`serde(rename_all = "camelCase")`)を追加し、`NoteEvent` に `timbre: Timbre` を足す。`MmlScore` / `MmlError` / `MmlParseResult` は変えない。
- `mml.rs`: `Timbre` に、番号からの変換(`from_number`)、名前からの変換(`match_name`、最長一致で名前と読み進めた長さを返す)、名前の一覧を持たせる。`Op::Timbre(Timbre)` を足し、`run_track` が現在の音色を持って各 `NoteEvent` に入れる。
- 互換: `@` を含まない MML は、全ノートの `timbre` が `fm` になる以外、出力が変わらない。既存のテストは、期待値に `timbre: Timbre::Fm` を足す。
- `commands/sound.rs`、`set_notify`、`set_column_notify` は変えない(検証は `parse` を通すので、`@` の誤りも自動で拒否される)。
- 生成バインディング(`tauri.gen.ts`)に `Timbre` の文字列ユニオンが出る。`generates_frontend_bindings` で確認する。

### 4. フロントエンド

- `lib/mmlTimbres.ts`(新規): `TIMBRES: Record<Timbre, TimbrePreset>`。`TimbrePreset` は、表示名・番号・音源の作り方・`gainDb`・`releaseSec` を持つ。番号と名前の一覧(ヘルプ用)も、この表から作る。
- `lib/mmlRender.ts`: 音源を `(トラック, 音色)` ごとに最初に使うときに作る。`noise` だけは 1 音ごとに作る。音源の音量は `preset.gainDb + (trackCount > 1 ? gainToDb(1 / trackCount) : 0)`。それ以外(サンプルレート、モノラル、`TAIL_SEC`、WAV 化)は変えない。
- `gainDb` の決め方: C4 の単音の RMS を現行の FM 音(−25.3 dB)にそろえる。ただし補正後のピークが 0.36 を超えるなら、0.36 以下になるまで下げる。初期値は spike の実測から次のとおり(既定のエンベロープで測った値で、プリセットを確定したあとに実装計画の最初の校正タスクで測り直して確定する)。

  | 音色 | gainDb | 音色 | gainDb |
  |---|---|---|---|
  | fm | 0.0 | bell | 0.0 |
  | square | −10.2 | epiano | 0.0 |
  | pulse | −11.6 | am | −1.4 |
  | triangle | −6.9 | lead | −20.3 |
  | saw | −5.3 | bass | −14.5 |
  | sine | −8.7 | drum | −6.8 |
  | organ | −14.3 | noise | −4.5 |

- UI: `SoundChoicePicker.svelte` の「記法」ヘルプに、音色の一覧(番号・名前)と書き方(`@3` または `@bell`)を足す。一覧は `TIMBRES` から作るので、ヘルプと実装がずれない。音色ごとの試聴ボタンは作らない。
- ドキュメント: `docs/guide/user-guide.md` の「MML で通知音を作る」に、音色の表と注意(`bass` は低音向き、`noise` は音程なし)を足す。

### 5. エラー処理

#75 の方針のまま。パース失敗・Tone.js の失敗・レンダリング失敗は `beep` で代替する。音色が原因で描画に失敗した場合(Android の WebView で特定の音源が動かない場合など)も同じ経路に入る。

## テスト

- Rust(単体): `@番号` / `@名前` の読み取り。最長一致(`@bellc`、`@amc4`)。トラックごとの独立。途中での切り替え。`[...]n` 内の `@`。`@` なしの MML の全ノートが `fm`。エラー位置(`@14`、`@xyz`、`@` 単独、`@` が末尾)。全 14 種の番号と名前の対応。名前が互いに接頭辞でないこと。
- フロント(Vitest): 全 `Timbre` にプリセットがあり、`releaseSec ≤ 0.6` で `gainDb` が有限であること。`tone` をモックして、`(トラック, 音色)` ごとに音源を作ること、`noise` が 1 音ごとに作られること、音量が `gainDb` とトラック減衰の合計になることを確認する。ヘルプの一覧が `TIMBRES` と一致すること。
- 実機(手動):
  - 実際の `renderScore` で 14 種すべてを WebKitGTK(Xvfb)で描画し、NaN・無音・ピーク超過がないこと、`gainDb` の校正基準(C4 の RMS が FM 音の ±1 dB 以内、ピーク 0.36 以下)を満たすことを確認する。
  - Xvfb と隔離したインスタンスで、`@` つきの MML の入力・試聴・保存・再起動後の復元を確認する(#75 と同じ手順)。
  - Android エミュレーター(Chrome 113)で、14 種が描画できることを確認する。`DuoSynth` / `MonoSynth` / `MembraneSynth` / `NoiseSynth` は、現行の FM 音源が使っていないノードを使うため。

## spike の結果(2026-10-16)

WebKitGTK 2.54.1 を Xvfb で動かし、`http://127.0.0.1` 起点のページで `Tone.Offline`(Tone.js 15.1.22、22.05kHz、モノラル)を測った。スクラッチパッドのみでの検証で、リポジトリは変更していない。

- 14 種(`pluck` と `metal` を除く)は描画できた。NaN も例外もなかった。
- C4 の単音の RMS は、`lead` −5 dB 〜 `noise` −27 dB の範囲に散らばった(`fm` は −25.3 dB)。`lead` は単音でもピークが 1.6 で、クリップする。
- 200 音(4 トラック × 50 音)の描画時間は、`organ` 0.3 秒 〜 `lead` 3.2 秒。大半は約 1 秒以下。
- `file://` 起点のページでは、AudioWorklet を使う音源(`PluckSynth`)が CORS で拒否された。選んだ 14 種は AudioWorklet を使わないので影響しない。
- `bass` は C6 以上で音量が急に落ちる。`membrane` の基本周波数は下降する。`noise` は音程がない。

## 未確認(実装後も残る見込み)

- 聞こえ方。作業者は音を聞けない。初期案のパラメータ(特に `bell` / `epiano` / `lead` / `organ`)は、実装後にユーザーの耳で調整する前提。
- 複数トラックで音色が違うときの音量バランス(聞き比べていない)。
- 実際の Android WebView、macOS、Windows。

## スコープ外

- 音色パラメータのユーザー調整、エンベロープやビブラートの MML 指定。
- `pluck` / `metal`、サンプル音源。
- 音色ごとの試聴ボタン、ピアノロール。
- 通知の種類ごとに別の音を割り当てる機能、MML の共有・インポート・エクスポート。
